# The compiled binary is normalised to this path so that later stages do not
# need to know the Rust target triple it was built for.
ARG BINARY_PATH=/out/app

FROM lukemathwalker/cargo-chef:latest-rust-alpine AS chef
ARG TARGETARCH
RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static upx
# Map the Docker architecture to the Rust target triple. Unsupported architectures fail the build
# early.
RUN set -eu; \
    case "${TARGETARCH}" in \
        amd64) machine=x86_64 ;; \
        arm64) machine=aarch64 ;; \
        *) echo "unsupported architecture: ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    echo "${machine}-unknown-linux-musl" > /etc/target-triple
# Copied from Sentry's image, pinned by digest, which Renovate bumps like the base images. The
# download it replaces asked the GitHub API for `releases/latest` without authentication: a
# rate-limited answer failed the build, and two builds of one commit could ship different CLIs.
# The index is multi-platform, so BuildKit picks the binary for the architecture being built.
COPY --from=getsentry/sentry-cli:3.8.2@sha256:633122f5fbd774716a0ebbcd7ece9c0584aa781cc61a84e7ce57964c635d6559 \
    /bin/sentry-cli /usr/local/bin/sentry-cli
# The upload runs only when a token is supplied, which CI never does. Without this, a CLI that
# cannot start on the build architecture would surface only in the release job.
RUN sentry-cli --version
WORKDIR /app

# The compiler `rust-toolchain.toml` pins, installed before any stage forks. Without it the
# toolchain would be whichever release the base image happens to carry, and only the stages that
# `COPY . .` would see the pin: `cargo chef cook` and `cargo build` would then compile with
# different rustc releases, and the build would recompile every dependency the cook just cached.
# Copied on its own so that no other change to the tree invalidates this layer.
COPY rust-toolchain.toml ./
RUN rustup toolchain install --no-self-update

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
ARG BINARY_PATH
COPY --from=planner /app/recipe.json recipe.json
# `--locked` on both: the recipe carries `Cargo.lock`, and a manifest it no longer satisfies fails
# here instead of being re-resolved into versions nobody reviewed.
RUN set -eu; \
    cargo chef cook --release --locked --target "$(cat /etc/target-triple)" --recipe-path recipe.json
COPY . .
# `target/` is removed in the same step, so this layer holds the binary and not the build tree that
# changes with every commit and would otherwise be exported to the layer cache on every run.
RUN set -eu; \
    target=$(cat /etc/target-triple); \
    cargo build --release --locked --target "${target}"; \
    install -D "target/${target}/release/s3_bucket_perma_link" "${BINARY_PATH}"; \
    rm -rf target

# Upload debug symbols to Sentry before stripping
ARG SENTRY_ORG
ARG SENTRY_PROJECT
ARG VERSION

RUN --mount=type=secret,id=sentry_token \
    if [ -f /run/secrets/sentry_token ]; then \
        sentry-cli debug-files upload \
            --auth-token $(cat /run/secrets/sentry_token) \
            --org ${SENTRY_ORG} \
            --project ${SENTRY_PROJECT} \
            --include-sources \
            ${BINARY_PATH}; \
    fi

# Strip and compress after uploading symbols
RUN strip --strip-all ${BINARY_PATH} && \
    upx --best --lzma ${BINARY_PATH}

# The configuration contract: every key this binary reads, everything it reads outside the
# loader's namespace, and a JSON Schema for the file an operator mounts. Generated from
# `src/config.rs` by the same toolchain and the same source tree as the binary, so it cannot
# describe a different service than the one shipping beside it.
#
# `--format contract` and `--format labels` come out of one generator binary in one layer. That is
# the whole guarantee: the labels the image carries and the document they point at are two
# renderings of a single value, and nothing in between can make them disagree.
#
# `FROM chef`, not `FROM builder`. The generator is an example and links the dev-dependencies,
# which turn on features the binary does not use (`terrace-config/testing`, `bitflags/std`). On
# `builder`'s release `target/` that recompiled `terrace-config` and every crate above `bitflags`
# with fat LTO, 78 to 90 s after the binary had finished. Cooked here in the dev profile, the
# stage runs beside `builder`.
FROM chef AS contract-builder

# A debug build whose only output is two text files, so debug info would be dead weight in the
# cooked layer below.
ENV CARGO_PROFILE_DEV_DEBUG=0

COPY --from=planner /app/recipe.json recipe.json

# The selection must match the `cargo run` below (profile, features, examples), or the run
# recompiles what this step cooked.
RUN cargo chef cook --locked --examples --recipe-path recipe.json

COPY . .

# Deliberately without the default the runtime stage below gives the same name. An unset release
# is *absent* from the document, where `unknown` would be a claim about which release it
# describes — and `--version` refuses an empty value rather than publishing one.
ARG version

# `target/` is removed in the same step, so the layer holds `/out` and not the build tree.
RUN set -eu; \
    mkdir -p /out; \
    if [ -n "${version:-}" ]; then set -- --version "${version}"; else set --; fi; \
    cargo run --locked --example config-schema -- --format contract "$@" > /out/contract.json; \
    cargo run --quiet --locked --example config-schema -- --format labels > /out/contract.labels; \
    rm -rf target

# The two generated files and nothing else, so exporting them with `--output type=local` costs a
# two-file layer rather than an export of the whole builder filesystem. This is what the label
# check reads, and what `oras attach` publishes.
FROM scratch AS contract
COPY --from=contract-builder /out/contract.json /out/contract.labels /

FROM alpine:3.24@sha256:294b683cb724975bec92580e1e685676bd4b50bda910ddb8c51d4cabeaec77e6 AS env

RUN apk update && \
    apk upgrade --no-cache && \
    apk add --no-cache ca-certificates mailcap tzdata

RUN update-ca-certificates

RUN adduser \
    --disabled-password \
    --gecos "" \
    --home "/nonexistent" \
    --shell "/sbin/nologin" \
    --no-create-home \
    --uid "10001" \
    "appuser"

FROM scratch AS runtime

ARG BINARY_PATH

ARG version=unknown
ARG release=unreleased

LABEL version=${version} \
      release=${release}

# The offline copy of the configuration contract, for a `docker save` tarball, an air-gapped
# mirror or an in-cluster reader. The chart repository reads whichever of this and the copy
# attached to the digest it can reach, and refuses the image when the two differ.
COPY --from=contract-builder /out/contract.json /config/contract.json

# Generated by `cargo run --example config-schema -- --format dockerfile`. Do not edit by hand:
# the `rust/config-contract` action diffs the region below against that command's output, and then
# checks the labels the *image* actually carries against what the same generator wrote inside the
# build.
#
# A `LABEL` key cannot be interpolated, and the document is produced inside a builder stage the
# host's `docker build` command line cannot reach — so hand-writing these is unavoidable, and the
# check on the built image is what makes it safe.
#
# The markers are what the drift check cuts on, and they are the crate's own: cutting by line
# count instead reads correctly right up until a fourth label is added, and then compares two of
# three lines and passes.
# terrace-config:labels:begin
LABEL dev.terrace.config.contract.version="1" \
      dev.terrace.config.contract.path="/config/contract.json" \
      dev.terrace.config.prefix="S3_PERMA_LINK_"
# terrace-config:labels:end

COPY --from=env /etc/passwd /etc/passwd
COPY --from=env /etc/group /etc/group
COPY --from=env /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=env /usr/share/zoneinfo /usr/share/zoneinfo

WORKDIR /app
COPY --from=builder --chown=root:root ${BINARY_PATH} ./app

USER 1000:1000

CMD ["./app"]
