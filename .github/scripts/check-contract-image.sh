#!/usr/bin/env bash
#
# Check a built image against the contract the same build generated.
#
#   check-contract-image.sh <image-ref> <export-dir>
#
# `image-ref` is an image that is already present locally — built with `--load`, or pulled by
# digest after a push. `export-dir` is what `docker buildx build --target contract --output
# type=local` wrote: `contract.json` and `contract.labels`, both produced by one invocation of the
# generator inside the same build.
#
# Two things are checked, and they fail for different reasons:
#
#   the labels   the image's `dev.terrace.config.*` must be the three this contract expects, or
#                nothing downstream can discover the document from the image's config blob
#   the copies   `/config/contract.json` inside the image must be byte-identical to the document
#                about to be attached to the digest, or the image and its attachment describe
#                different builds
#
# The second is the cross-check a hash label would have bought. The design dropped that label —
# it was the only dynamic one, and a multi-stage build cannot feed it from a generator running
# inside a builder stage — on the grounds that the *build* is the one place holding both copies
# and can compare them for free. This is that comparison. Skipping it would leave a stale
# in-image copy as exactly the failure nothing downstream can see.
#
# A `FROM scratch` image has no shell, which is why this uses `docker create` and `docker cp`
# rather than running anything in the container.

set -euo pipefail

if [ "$#" -ne 2 ]; then
    echo "usage: ${0##*/} <image-ref> <export-dir>" >&2
    exit 2
fi

image="$1"
export_dir="$2"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

labels="$(mktemp)"
embedded="$(mktemp)"
probe="contract-probe-$$"
cleanup() {
    rm -f "$labels" "$embedded"
    docker rm --force "$probe" >/dev/null 2>&1 || true
}
trap cleanup EXIT

# `.Config.Labels`, capital C — this is `docker inspect`. `crane config` reports the same map
# under `.config.Labels`, and reading the wrong one yields `null`, which a careless comparison
# treats as "nothing to compare" and passes. `check-contract-labels.sh` refuses a `null`.
docker image inspect --format '{{json .Config.Labels}}' "$image" > "$labels"

status=0
"$here/check-contract-labels.sh" "$labels" "$export_dir/contract.labels" || status=1

docker create --name "$probe" "$image" >/dev/null
docker cp "$probe:/config/contract.json" "$embedded"

if cmp --silent "$embedded" "$export_dir/contract.json"; then
    echo "ok: the document inside the image is the one this build generated"
else
    echo "error: /config/contract.json inside $image is not the document this build exported." \
         "The image and the artifact about to be attached to its digest describe different" \
         "builds, and the chart repository refuses an image whose two copies disagree." >&2
    # The documents run to tens of kilobytes; the first differing byte is the actionable part.
    cmp "$embedded" "$export_dir/contract.json" >&2 || true
    status=1
fi

exit "$status"
