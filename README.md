<!--
Generated from .github/templates/README.md.hbs — edit that file, not this one. CI renders it on
every pull request and commits the result back to the branch; a push to master whose README.md
does not match its template fails the `readme` check in .github/workflows/docs.yml.

Variables come from `cargo run --example readme-variables`, which reads them off the code they
describe:

    repo, branch, build_workflow, docker_image   the repository coordinates, defined once
    prefix, config_var, secrets_dir_var,
    indirection_suffix                           the loader's own dialect, from src/config/loader.rs
    loader_table, keys_table                     generated from Config, via terrace-config's
                                                 `schema` feature

That is what keeps the configuration reference honest: a key added to `Config` without a `///`
comment shows an empty Purpose cell in the pull request that adds it, and a key removed from it
leaves this page on the same commit.
-->
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
![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/TimSchoenle/s3-bucket-perma-link/build.yaml)
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

Every generation logs which layer supplied each key, so an unexpected value can be traced to the
file or variable it came from without attaching a debugger.

See [config.example.toml](config.example.toml) for a complete file.

#### The variables read before the configuration exists

| Variable | Role | Default | Purpose |
|---|---|---|---|
| `S3_PERMA_LINK_CONFIG` | config | `config.toml` | Names the TOML layer: a file, or a directory whose `*.toml` files are all merged in name order. |
| `S3_PERMA_LINK_SECRETS_DIR` | secrets dir | — | Names a directory of key-named files — a mounted Kubernetes `Secret` volume. Each file supplies the key its name spells. |

#### The keys

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

The same through the environment, one variable per field — note that the environment layer
lowercases keys, so a path with uppercase or `-` in it has to come from the TOML layer:

```
S3_PERMA_LINK_BUCKET__ENTRIES__CHANGELOG__BUCKET=media
S3_PERMA_LINK_BUCKET__ENTRIES__CHANGELOG__OBJECT=releases/CHANGELOG.md
```

## Contributing

`README.md` is generated. Edit [.github/templates/README.md.hbs](.github/templates/README.md.hbs)
instead — CI renders it on every pull request and commits the result back to the branch, and a
push to `master` whose `README.md` does not match its template fails.

The configuration tables in it are generated from `Config` in
[src/config.rs](src/config.rs), so a key is documented by documenting the field:

```bash
cargo run --example config-schema -- --format markdown
```

## License

See [LICENSE](https://github.com/TimSchoenle/s3-bucket-perma-link/blob/master/LICENSE) for
more information.
