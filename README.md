<!--
Generated from .github/templates/README.md.hbs. Edit that file, not this one.

CI renders it on every pull request and commits the result back to the branch. A push to master
whose README.md does not match its template fails the `README` job in
.github/workflows/docs.yml.

The payload is two halves. The repository coordinates, the release read off Cargo.toml and the
table of documents come from TimSchoenle/actions/actions/common/readme-variables. The
configuration tables and the loader's own spellings come from one command in this repository,
merged in as that action's `extra` input:

    cargo run --quiet --example readme-variables

That is what keeps the configuration reference honest: a key added to `Config` without a `///`
comment shows an empty Purpose cell in the pull request that adds it, and a key removed from it
leaves this page on the same commit.

Nothing in this comment may contain a mustache that is not a real reference.
-->

# s3-bucket-perma-link

Serves S3 objects under fixed request paths, so a published link survives the object behind it changing.

[![Release](https://img.shields.io/github/v/release/TimSchoenle/s3-bucket-perma-link?sort=semver)](https://github.com/TimSchoenle/s3-bucket-perma-link/releases)
[![Build](https://img.shields.io/github/actions/workflow/status/TimSchoenle/s3-bucket-perma-link/build.yaml?branch=master)](https://github.com/TimSchoenle/s3-bucket-perma-link/actions/workflows/build.yaml)
[![Coverage](https://codecov.io/gh/TimSchoenle/s3-bucket-perma-link/branch/master/graph/badge.svg?token=dDUZjsYmh2)](https://codecov.io/gh/TimSchoenle/s3-bucket-perma-link)
[![License](https://img.shields.io/github/license/TimSchoenle/s3-bucket-perma-link)](LICENSE)

## What this is

A read-only front for an S3 bucket. Each block under `bucket.entries` binds one request path to
one bucket and one object key, and a `GET` on that path streams the object back through the
service. A path with no block answers 404 before any S3 call is made, so the bucket is never
listed and never browsable.

The configuration tables below are generated from the `Config` type the binary loads, as is the
contract the image publishes at `/config/contract.json`. Documenting a field with a `///` comment
documents its key here, in the commit that adds the field.

## Quick start

```bash
docker run --rm -p 8080:8080 \
  -v "$PWD/config.example.toml:/app/config.toml:ro" \
  timmi6790/s3-bucket-perma-link:v1.1.1
```

The example file boots the service. Its credentials are placeholders, so replace `s3.access_key`
and `s3.secret_key` before `GET /docs/handbook` returns anything but a 500.

## Table of contents

- [Features](#features)
- [Installation](#installation)
- [Usage](#usage)
- [Configuration](#configuration)
- [Operations](#operations)
- [Compatibility](#compatibility)
- [Documentation](#documentation)
- [Contributing](#contributing)
- [Security](#security)
- [License](#license)

## Features

- The router has two entries: `/health`, and a catch-all that resolves the request path against
  `bucket.entries`. Health is registered first, so no permanent link can be spelled `/health`.
- Objects are streamed from the bucket to the client rather than buffered, so the process holds a
  chunk of the object and never the object.
- **A rotated credential is re-read, not restarted around.** When a watched file changes, the
  supervisor rebuilds the bucket clients and the listener in place. `telemetry.*` is the
  exception: the log subscriber and the Sentry client are installed once per process.
- Configuration arrives in five layers, and the last three are mutually exclusive per key. A key
  supplied by both an environment variable and a mounted secret fails the boot instead of letting
  one of them win.
- **The image describes its own configuration.** `/config/contract.json` lists every key the
  binary reads and the six variables its dependencies read outside the loader's prefix, and three
  OCI labels point at it. The chart repository reads that copy and the one attached to the image
  digest, and refuses the image when the two differ.

## Installation

### Docker

```bash
docker pull timmi6790/s3-bucket-perma-link:v1.1.1
```

Published as a multi-platform manifest for `linux/amd64` and `linux/arm64`. Every release is
signed with cosign, and its configuration contract is attached to the manifest digest as a signed
referrer.

### Helm

```bash
helm repo add timschoenle https://timschoenle.github.io/helm-charts
helm install s3-bucket-perma-link timschoenle/s3-bucket-perma-link
```

The chart pins the image by digest and is bumped by this repository's release. Its values are at
[charts/s3-bucket-perma-link](https://github.com/TimSchoenle/helm-charts/tree/main/charts/s3-bucket-perma-link).

### From source

```bash
git clone https://github.com/TimSchoenle/s3-bucket-perma-link.git
cd s3-bucket-perma-link
cargo build --release
```

`just` with no arguments lists the recipes. `just verify` runs the formatter, clippy and the
tests in one go.

## Usage

The service binds nothing until it has a configuration. Copy the example, fill in the four
required `s3.*` keys, and run it:

```bash
cp config.example.toml config.toml
cargo run --release
```

Credentials do not belong in that file on a deployment. Point the loader at a directory of
key-named files instead, which is what a Kubernetes `Secret` volume mounts:

```bash
S3_PERMA_LINK_SECRETS_DIR=/etc/s3-perma-link/secrets cargo run --release
```

Two recipes cover the generated files and the checks:

```bash
just regenerate   # docs/config.contract.json and the Dockerfile LABEL block
just verify       # fmt, clippy, tests
```

## Configuration

Configuration is layered, via [terrace-config](https://github.com/TimSchoenle/terrace-config).
Lowest precedence first:

1. Built-in defaults.
2. TOML at `$S3_PERMA_LINK_CONFIG` — a file, or a directory whose `*.toml` files are merged in
   name order. Defaults to `config.toml` in the working directory.
3. `S3_PERMA_LINK_`-prefixed environment variables, `__` separating nesting levels.
4. `$S3_PERMA_LINK_SECRETS_DIR` — a directory of key-named files, which is what a Kubernetes
   `Secret` volume looks like. File name `s3__secret_key` sets `s3.secret_key`.
5. `S3_PERMA_LINK_<KEY>_FILE` — one key, read from the file the variable names.

The last three are **mutually exclusive per key**: a key supplied by two of them fails the boot
rather than being resolved by precedence, because a stale environment variable silently
outranking a rotated mounted secret is how a service keeps serving on a credential its operator
believes they replaced.

Mounted files are watched. When one changes, the service rebuilds its bucket clients and listener
in place, so a rotated S3 credential needs no restart. `telemetry.*` is the exception: the log
subscriber and the Sentry client are installed once and still need one.

Every generation logs which layer supplied each key, so an unexpected value can be traced to the
file or variable it came from without attaching a debugger.

See [config.example.toml](config.example.toml) for a complete file.

### The variables read before the configuration exists

| Variable | Role | Default | Purpose |
|---|---|---|---|
| `S3_PERMA_LINK_CONFIG` | config | `config.toml` | Names the TOML layer: a file, or a directory whose `*.toml` files are all merged in name order. |
| `S3_PERMA_LINK_SECRETS_DIR` | secrets dir | — | Names a directory of key-named files — a mounted Kubernetes `Secret` volume. Each file supplies the key its name spells. |

### The keys

Every key below is spelled the same way in all four layers: `__` separates nesting levels and
case is folded. `s3.secret_key` is `S3_PERMA_LINK_S3__SECRET_KEY` as an environment
variable, `S3_PERMA_LINK_S3__SECRET_KEY_FILE` as file indirection, and
`s3__secret_key` as a file name inside `$S3_PERMA_LINK_SECRETS_DIR`.

| TOML | Type | Environment | Default | Flags | Purpose |
|---|---|---|---|---|---|
| `server.host` | `String` | `S3_PERMA_LINK_SERVER__HOST` | `0.0.0.0` | — | Address to listen on. `0.0.0.0` in a container, which is the deployment this ships as. |
| `server.port` | `u16` | `S3_PERMA_LINK_SERVER__PORT` | `8080` | — | Port to listen on. |
| `s3.access_key` | `SecretString` | `S3_PERMA_LINK_S3__ACCESS_KEY` | — | required, secret | S3 access key. Mount it rather than setting it in a file that is committed. |
| `s3.secret_key` | `SecretString` | `S3_PERMA_LINK_S3__SECRET_KEY` | — | required, secret | S3 secret key. Mount it rather than setting it in a file that is committed. |
| `s3.host` | `String` | `S3_PERMA_LINK_S3__HOST` | — | required | The endpoint, e.g. `s3.eu-central-1.amazonaws.com`. |
| `s3.region` | `String` | `S3_PERMA_LINK_S3__REGION` | — | required | The region the endpoint serves, e.g. `eu-central-1`. |
| `bucket.entries` | `HashMap<String, BucketEntry>` | `S3_PERMA_LINK_BUCKET__ENTRIES` | — | required | One `[bucket.entries.<request path>]` block per permanent link, each carrying a `bucket` and an `object`. |
| `telemetry.log_level` | `String` | `S3_PERMA_LINK_TELEMETRY__LOG_LEVEL` | `info` | — | How much the service says: `trace`, `debug`, `info`, `warn` or `error`. |
| `telemetry.sentry_dsn` | `SecretString` | `S3_PERMA_LINK_TELEMETRY__SENTRY_DSN` | unset | secret | Sentry DSN. Absent disables Sentry entirely. |

`bucket.entries` is one row above rather than one per link, because the key paths under it are
route names no type knows ahead of time. Each is a table keyed by request path:

```toml
[bucket.entries."docs/handbook"]
bucket = "media"
object = "handbook.pdf"
```

The same through the environment, one variable per field. That layer lowercases keys, so a path
carrying uppercase or `-` has to come from the TOML layer:

```bash
S3_PERMA_LINK_BUCKET__ENTRIES__CHANGELOG__BUCKET=media
S3_PERMA_LINK_BUCKET__ENTRIES__CHANGELOG__OBJECT=releases/CHANGELOG.md
```

## Operations

`GET /health` returns 200 once the listener is bound. It carries no body and reaches nothing
downstream, so it reports that the process is up rather than that S3 is.

`SIGTERM` and `SIGINT` both drain: in-flight downloads finish before the listener releases the
address. Actix's own signal handling is off, because the reload supervisor owns the process
lifecycle and a listener that stopped itself would be rebuilt rather than shut down.

The runtime image is `FROM scratch` and holds the binary, CA certificates, time zone data and
`/config/contract.json`. There is no shell, so `kubectl exec` has nothing to run. It runs as
`1000:1000` and writes nothing outside stdout, so it needs no writable filesystem.

## Compatibility

| | Supported |
| --- | --- |
| Rust | edition 2024 |
| Platforms | `linux/amd64`, `linux/arm64` |
| Image | `timmi6790/s3-bucket-perma-link` |
| Helm chart | `timschoenle/s3-bucket-perma-link` |

## Documentation

| Document | Summary |
| --- | --- |
| [`docs/config.contract.json`](docs/config.contract.json) | — |

That table is walked out of `docs/` rather than maintained by hand, which is why a document with
no prose in it lands there with nothing to summarise. That one is the configuration contract the
image publishes and the chart validates against.

## Contributing

Issues and pull requests are welcome. Commits follow Conventional Commits, and release-please
reads them to open the release pull request, so the type on a commit decides the next version.

`README.md`, `docs/config.contract.json` and the `LABEL` block in the `Dockerfile` are generated.
Each says so in its first lines, and CI reverts an edit made to the output instead of the source.
Run `just verify` before pushing; it is what the pull request is going to run anyway.

## Security

Do not open a public issue for a vulnerability. [SECURITY.md](SECURITY.md) has the reporting
route and which versions get fixes.

## License

`GPL-3.0-only`. [LICENSE](LICENSE) has the terms.
