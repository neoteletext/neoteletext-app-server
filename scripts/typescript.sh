#!/usr/bin/env bash
# The TypeScript stage of the gates: the channels' programs and their
# reference kit (docs/applications.md section 5), type-checked with every
# strict option, warnings as errors, and tested with Node's own runner.
# scripts/check.sh runs it in both gates, and the full gate's Linux stage
# runs it there as well, on the Node the deploy host has. Nothing is
# skipped: a missing or old Node is a failure.
#   scripts/typescript.sh
set -euo pipefail
cd "$(dirname "$0")/.."

# Node runs .ts files itself by stripping their types from 22.18 on, so
# the programs have no build step and run as they are checked.
readonly NODE_LEAST="22.18.0"
# The tests, by Node's own glob.
readonly TESTS=("kits/**/*.test.ts" "channels/**/*.test.ts")
# The lockfile's SHA-256 when node_modules was installed from it, so a
# changed lockfile installs again.
readonly INSTALLED_FROM="node_modules/.installed-from.sha256"

step() { printf '\n== %s\n' "$*"; }
fail() {
    printf '\nFAILED: %s\n' "$*" >&2
    exit 1
}

sha256_of() {
    if command -v sha256sum >/dev/null; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

step "TypeScript: Node $NODE_LEAST or later"
command -v node >/dev/null || fail "node is not installed; the TypeScript stage needs Node $NODE_LEAST or later"
version="$(node --version)"
node -e '
const [least, found] = process.argv.slice(1).map((text) => text.replace(/^v/, "").split(".").map(Number));
const newer = least.findIndex((part, index) => found[index] !== part);
process.exit(newer === -1 || found[newer] > least[newer] ? 0 : 1);
' "$NODE_LEAST" "$version" || fail "Node $version is older than $NODE_LEAST, which runs TypeScript by stripping its types"
printf 'node %s\n' "$version"

lock="$(sha256_of package-lock.json)"
if [[ ! -f "$INSTALLED_FROM" || "$(cat "$INSTALLED_FROM")" != "$lock" ]]; then
    step "TypeScript: install the locked tools (npm ci)"
    # No package's install scripts run: the tools need none.
    npm ci --ignore-scripts --no-audit --no-fund
    printf '%s\n' "$lock" >"$INSTALLED_FROM"
fi

step "TypeScript: type check, every strict option, errors fail"
npx --no -- tsc --version
npx --no -- tsc -p tsconfig.json

step "TypeScript: tests"
node --test "${TESTS[@]}"
