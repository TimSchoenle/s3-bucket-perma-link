#!/usr/bin/env bash
#
# Regenerate everything derived from `src/config.rs` and fail on a difference.
#
#   check-contract-drift.sh            # report drift
#   check-contract-drift.sh --write    # rewrite the generated files instead
#
# Two artefacts are generated from the configuration types and then committed, which means both
# can go stale:
#
#   docs/config.contract.json   the contract the image publishes
#   Dockerfile                  the LABEL block that makes it discoverable
#
# This is the cheap half of the check. The build checks the *image* against the same generator's
# labels, which is the half that catches a base image overriding a label or a `LABEL` line
# deleted on a branch nobody diffed — but it needs an image, and it reports a prefix rename one
# step later than this does. Here, a pull request that renames a key or changes the loader's
# prefix carries the diff for it, in the same commit that caused it, for a reviewer to see.
#
# `--format contract` is generated without `--version`, `--revision` or `--created`, so it is
# byte-reproducible across rebuilds and releases: the committed copy describes the configuration
# surface, and the copy in an image additionally names the build it came from.

set -euo pipefail

write=0
case "${1:-}" in
    --write) write=1 ;;
    "") ;;
    *)
        echo "usage: ${0##*/} [--write]" >&2
        exit 2
        ;;
esac

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

contract="docs/config.contract.json"
dockerfile="Dockerfile"
begin="# contract-labels:begin"
end="# contract-labels:end"

generate() {
    cargo run --quiet --example config-schema -- --format "$1"
}

status=0

# --------------------------------------------------------------------------------------------
# The document
# --------------------------------------------------------------------------------------------

generated="$(mktemp)"
trap 'rm -f "$generated" "${block:-}" "${committed:-}"' EXIT

generate contract > "$generated"

if [ "$write" -eq 1 ]; then
    mkdir -p "$(dirname "$contract")"
    cp "$generated" "$contract"
elif ! diff -u "$contract" "$generated" --label "$contract (committed)" --label "$contract (generated)"; then
    echo "error: $contract no longer matches the configuration types it is generated from." >&2
    echo "       Run '${BASH_SOURCE[0]} --write' and commit the result." >&2
    status=1
fi

# --------------------------------------------------------------------------------------------
# The LABEL block
# --------------------------------------------------------------------------------------------
#
# Delimited by comments rather than matched by pattern: a Dockerfile carries other `LABEL`
# instructions, and a check that guessed which one it meant would pass whenever it guessed wrong.

if ! grep -qF "$begin" "$dockerfile" || ! grep -qF "$end" "$dockerfile"; then
    echo "error: $dockerfile carries no '$begin' … '$end' block, so the generated LABEL" \
         "instruction has nowhere to be compared against." >&2
    exit 1
fi

block="$(mktemp)"
committed="$(mktemp)"
generate dockerfile > "$block"
sed -n "/^${begin}\$/,/^${end}\$/p" "$dockerfile" | sed '1d;$d' > "$committed"

if [ "$write" -eq 1 ]; then
    # `sed` cannot substitute a multi-line block portably, so the file is rewritten around the
    # markers instead: everything up to `begin`, the generated block, everything from `end`.
    rewritten="$(mktemp)"
    {
        sed -n "1,/^${begin}\$/p" "$dockerfile"
        cat "$block"
        sed -n "/^${end}\$/,\$p" "$dockerfile"
    } > "$rewritten"
    mv "$rewritten" "$dockerfile"
elif ! diff -u "$committed" "$block" --label "$dockerfile (committed)" --label "$dockerfile (generated)"; then
    echo "error: the LABEL block in $dockerfile no longer matches" \
         "'--format dockerfile'. Run '${BASH_SOURCE[0]} --write' and commit the result." >&2
    status=1
fi

if [ "$write" -eq 1 ]; then
    echo "ok: rewrote $contract and the LABEL block in $dockerfile"
elif [ "$status" -eq 0 ]; then
    echo "ok: $contract and the LABEL block in $dockerfile match the configuration types"
fi

exit "$status"
