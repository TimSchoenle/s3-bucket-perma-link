#!/usr/bin/env bash
#
# Check that a built image carries the configuration-contract labels its own contract expects.
#
#   check-contract-labels.sh <labels.json> <contract.labels>
#
# `labels.json` is a JSON object of the image's labels, exactly as a registry or a daemon reports
# them. `contract.labels` is what `config-schema --format labels` wrote in the same build that
# produced the image — the expectation, generated rather than restated.
#
# This mirrors `Contract::verify_labels`: presence and equality of the three
# `dev.terrace.config.*` labels, nothing more. Extra labels are ignored on purpose. Every image
# carries `org.opencontainers.image.*` and whatever its base contributed, and none of that is
# this document's business.
#
# # Why the image and not the Dockerfile
#
# The `LABEL` block in the Dockerfile is hand-written, because a `LABEL` key cannot be
# interpolated and the document it describes is produced inside a builder stage. A source diff
# cannot see a base image that overrode a label, a `LABEL` line deleted on a branch nobody
# diffed, or a build argument that silently failed to interpolate. This reads what was actually
# built, which is what a registry will serve.
#
# # Reading the labels out of the right place
#
# Two spellings, and picking the wrong one yields `null` — which a careless comparison treats as
# "nothing to compare" and passes:
#
#   docker inspect --format '{{json .Config.Labels}}' "$image"          # capital C
#   crane config --platform linux/amd64 "$image" | jq -c '.config'      # lowercase c, then .Labels
#
# `crane config` against a multi-platform index without `--platform` does not fail loudly in
# every version, so always pass it. Labels live in each manifest's own config blob: a per-platform
# base image or build argument can leave one architecture carrying them and another not, so every
# platform in the index has to be checked, not just the one the runner happens to be.
#
# Every violation is reported before exiting. A build that names one missing label and hides two
# is a second round trip.

set -euo pipefail

if [ "$#" -ne 2 ]; then
    echo "usage: ${0##*/} <labels.json> <contract.labels>" >&2
    exit 2
fi

labels="$1"
expected="$2"

for file in "$labels" "$expected"; do
    if [ ! -s "$file" ]; then
        echo "error: '$file' is missing or empty; there is nothing to check against" >&2
        exit 2
    fi
done

# A `null` here is the trap above: the image reported no label map at all, or the wrong JSON path
# was read. Either way, checking against it would pass for the wrong reason.
if [ "$(jq -r 'type' "$labels")" != "object" ]; then
    echo "error: '$labels' is not a JSON object of labels but $(jq -c '.' "$labels"). The image" \
         "carries no labels at all, or the wrong field was read — '.Config.Labels' for" \
         "'docker inspect', '.config.Labels' for 'crane config'." >&2
    exit 2
fi

# The generator writes one `NAME=value` per line and neither half can contain a newline, so the
# first `=` separates them and the rest of the line is the value verbatim.
status=0
checked=0
while IFS='=' read -r name value; do
    [ -n "$name" ] || continue
    checked=$((checked + 1))
    actual="$(jq -r --arg n "$name" '.[$n] // ""' "$labels")"
    if [ "$actual" = "$value" ]; then
        continue
    fi
    if [ -z "$actual" ]; then
        echo "error: the image carries no '$name', so nothing can discover this contract from" \
             "its config blob. The Dockerfile's LABEL block is what emits it." >&2
    else
        echo "error: the image's '$name' is '$actual', and this contract's is '$value'. A label" \
             "that disagrees with the document is a contract a pipeline will look for in the" \
             "wrong place, or not recognise at all." >&2
    fi
    status=1
done < "$expected"

if [ "$checked" -eq 0 ]; then
    echo "error: '$expected' named no labels, so this checked nothing" >&2
    exit 2
fi

if [ "$status" -eq 0 ]; then
    echo "ok: the image carries all $checked contract labels this build generated"
fi

exit "$status"
