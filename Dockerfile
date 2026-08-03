# The compiled binary is normalised to this path so that later stages do not
# need to know the Rust target triple it was built for.
ARG BINARY_PATH=/out/app

FROM lukemathwalker/cargo-chef:latest-rust-alpine AS chef
ARG TARGETARCH
RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static upx curl jq
# Map the Docker architecture to the machine name used by Rust target triples and
# by upstream release artifacts. Unsupported architectures fail the build early.
RUN set -eu; \
    case "${TARGETARCH}" in \
        amd64) machine=x86_64 ;; \
        arm64) machine=aarch64 ;; \
        *) echo "unsupported architecture: ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    echo "${machine}" > /etc/target-machine; \
    echo "${machine}-unknown-linux-musl" > /etc/target-triple
# Install sentry-cli
RUN set -eu; \
    machine=$(cat /etc/target-machine); \
    latest_version=$(curl -sSf https://api.github.com/repos/getsentry/sentry-cli/releases/latest | jq -r .tag_name); \
    wget -qO /usr/local/bin/sentry-cli "https://downloads.sentry-cdn.com/sentry-cli/${latest_version}/sentry-cli-Linux-${machine}"; \
    chmod +x /usr/local/bin/sentry-cli
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
ARG BINARY_PATH
COPY --from=planner /app/recipe.json recipe.json
RUN set -eu; \
    cargo chef cook --release --target "$(cat /etc/target-triple)" --recipe-path recipe.json
COPY . .
RUN set -eu; \
    target=$(cat /etc/target-triple); \
    cargo build --release --target "${target}"; \
    install -D "target/${target}/release/s3_bucket_perma_link" "${BINARY_PATH}"

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

FROM alpine:3.24@sha256:28bd5fe8b56d1bd048e5babf5b10710ebe0bae67db86916198a6eec434943f8b AS env

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

COPY --from=env /etc/passwd /etc/passwd
COPY --from=env /etc/group /etc/group
COPY --from=env /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=env /usr/share/zoneinfo /usr/share/zoneinfo

WORKDIR /app
COPY --from=builder --chown=root:root ${BINARY_PATH} ./app

USER 1000:1000

CMD ["./app"]
