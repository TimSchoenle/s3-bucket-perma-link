<br/>
<p align="center">
  <h3 align="center">S3 Bucket Permanent Permanent Link</h3>

  <p align="center">
    <a href="https://github.com/TimSchoenle/s3-bucket-perma-link/issues">Report Bug</a>
    .
    <a href="https://github.com/TimSchoenle/s3-bucket-perma-link/issues">Request Feature</a>
  </p>
</p>

<div align="center">

![Docker Image Version (latest semver)](https://img.shields.io/docker/v/timmi6790/s3-bucket-perma-link)
![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/TimSchoenle/s3-bucket-perma-link/build.yml)
![Issues](https://img.shields.io/github/issues/TimSchoenle/s3-bucket-perma-link)
[![codecov](https://codecov.io/gh/TimSchoenle/s3-bucket-perma-link/branch/master/graph/badge.svg?token=dDUZjsYmh2)](https://codecov.io/gh/TimSchoenle/s3-bucket-perma-link)
![License](https://img.shields.io/github/license/TimSchoenle/s3-bucket-perma-link)
[![wakatime](https://wakatime.com/badge/github/TimSchoenle/s3-bucket-perma-link.svg)](https://wakatime.com/badge/github/TimSchoenle/s3-bucket-perma-link)

</div>

## About The Project

A simple web server to allow pre-defined urls to always access specific S3 bucket resources.

### Installation - Helm chart

- [Helm chart](https://github.com/TimSchoenle/helm-charts/tree/main/charts/s3-bucket-perma-link)

### Configuration

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

Mounted files are watched. When one changes, the service rebuilds its bucket clients and
listener in place — a rotated S3 credential needs no restart. `telemetry.*` is the exception:
the log subscriber and the Sentry client are installed once and still need one.

See [config.example.toml](config.example.toml) for a complete file.

| Key                   | Required | Environment variable                    | Description                                    | Example                                  |
|-----------------------|----------|-----------------------------------------|------------------------------------------------|------------------------------------------|
| `s3.access_key`       | X        | `S3_PERMA_LINK_S3__ACCESS_KEY`          | S3 access key                                  | `e25a2fd93e1049a4bb48d00907d6f4bf.access` |
| `s3.secret_key`       | X        | `S3_PERMA_LINK_S3__SECRET_KEY`          | S3 secret key                                  | `a5990007b7a54f83b52594a86c4d520e`       |
| `s3.host`             | X        | `S3_PERMA_LINK_S3__HOST`                | S3 endpoint                                    | `s3.amazon.com`                          |
| `s3.region`           | X        | `S3_PERMA_LINK_S3__REGION`              | S3 region                                      | `eu-central-1`                           |
| `bucket.entries`      | X        | see below                               | Request path to bucket and object key          | see below                                |
| `server.host`         |          | `S3_PERMA_LINK_SERVER__HOST`            | Listen address [default: `0.0.0.0`]            | `0.0.0.0`                                |
| `server.port`         |          | `S3_PERMA_LINK_SERVER__PORT`            | Listen port [default: `8080`]                  | `9090`                                   |
| `telemetry.log_level` |          | `S3_PERMA_LINK_TELEMETRY__LOG_LEVEL`    | `trace`/`debug`/`info`/`warn`/`error` [default: `info`] | `info`                          |
| `telemetry.sentry_dsn`|          | `S3_PERMA_LINK_TELEMETRY__SENTRY_DSN`   | Sentry DSN; absent disables Sentry             |                                          |

`bucket.entries` is a table keyed by request path:

```toml
[bucket.entries."docs/handbook"]
bucket = "media"
object = "handbook.pdf"
```

The same through the environment, one variable per field — note that the environment layer
lowercases keys, so a path with uppercase or `-` in it has to come from the TOML layer:

```
S3_PERMA_LINK_BUCKET__ENTRIES__CHANGELOG__BUCKET=media
S3_PERMA_LINK_BUCKET__ENTRIES__CHANGELOG__OBJECT=releases/CHANGELOG.md
```

## License

See [LICENSE](https://github.com/TimSchoenle/s3-bucket-perma-link/blob/main/LICENSE.md) for
more information.
