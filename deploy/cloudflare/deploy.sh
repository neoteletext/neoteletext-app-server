#!/bin/bash
#
# Deploy the NeoTeletext server's Cloudflare host (decision 20, DEC-4434):
# the Worker and its Durable Objects in crates/teletext-cloudflare, at the
# address wrangler.toml routes (cf.neoteletext.com). In place of a hand-run
# `wrangler deploy` (TODO-1304):
#   1. the tools at their pinned versions: worker-build from
#      scripts/tools.sh in the repository's tools/, its own downloads
#      (wasm-bindgen, wasm-opt, esbuild) in tools/cache, and wrangler from
#      the crate's package-lock.json, installed with `npm ci`; nothing for
#      the whole machine (DEVELOPMENT.md section 13);
#   2. the Worker's version now serving, written down before anything
#      changes: the version to roll back to, with the command that does it;
#   3. the deployment, built by wrangler with worker-build --release, the
#      Cloudflare token read from the credentials file into wrangler's
#      environment alone, never printed;
#   4. the checks after, against the deployed address: each channel's
#      channel.ttx and small page answer, the push upgrade is refused with
#      `Origin` (403) and without the subprotocol (426), a program without
#      the channel's secret is refused (401), and `neottx probe` is
#      accepted with the subprotocol. A failed check stops the script with
#      the rollback command;
#   5. the deployment recorded in rdm (`rdm deployed`).
# The Worker's secrets (PROGRAM_SECRET_<FOLDER>) are set apart with
# `wrangler secret put` and are not touched here.
#
#   deploy/cloudflare/deploy.sh
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
crate=$repo/crates/teletext-cloudflare
worker=neoteletext                        # wrangler.toml's name
address=https://cf.neoteletext.com        # wrangler.toml's route and ORIGIN
target=cf.neoteletext.com                 # the rdm deployment target
token_file=$HOME/devel/credentials/cloudflare-neoteletext.env      # NEOTELETEXT_CF_TOKEN
account_file=$HOME/devel/credentials/cloudflare-credentials.env    # CLOUDFLARE_ACCOUNTID
neottx=${NEOTTX:-$repo/tmp/neottx/bin/neottx}  # the engine's neottx at the server's pinned commit
subprotocol=neottx.10                     # the push WebSocket's (DOC-1917)
stamp=$(date +%Y-%m-%d-%H%M%S)
record=$repo/tmp/cloudflare-deploy-$stamp.txt

say() { printf '\n==> %s\n' "$*"; }
die() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }

# The scratch files of the build and of wrangler stay on disk in the
# repository's ignored tmp/, never in /tmp.
mkdir -p "$repo/tmp"
export TMPDIR=$repo/tmp
export PATH=$repo/tools/bin:$PATH
export XDG_CACHE_HOME=$repo/tools/cache
export WRANGLER_SEND_METRICS=false
export WRANGLER_LOG_PATH=$repo/tmp/wrangler-logs

[[ -z $(git -C "$repo" status --porcelain -- crates Cargo.toml Cargo.lock) ]] ||
    die "the crates have changes not committed: a deployment is of a commit"
commit=$(git -C "$repo" rev-parse HEAD)

say "tools"
build_version=$(sed -n 's/^readonly WORKER_BUILD_VERSION="\(.*\)"$/\1/p' "$repo/scripts/tools.sh")
[[ -n $build_version ]] || die "scripts/tools.sh pins no worker-build version"
have=$(worker-build --version 2>/dev/null || true)
[[ $have == "$build_version" ]] || die "worker-build $build_version is needed in tools/bin (scripts/tools.sh); found '${have:-none}'"
wrangler_version=$(node -e 'console.log(require(process.argv[1]).devDependencies.wrangler)' "$crate/package.json")
(cd "$crate" && npm ci --no-audit --no-fund >/dev/null 2>&1) || die "npm ci failed in $crate"
have=$(cd "$crate" && npx --no -- wrangler --version)
[[ $have == "$wrangler_version" ]] || die "wrangler $wrangler_version is pinned; npm ci gave $have"
[[ -x $neottx ]] || die "neottx is needed for the checks after, at $neottx (NEOTTX): cargo install the engine's neoteletext-cli at the pinned rev"
echo "worker-build $build_version, wrangler $wrangler_version"

# The token reaches wrangler's environment and nothing else's; set -x is
# never on, and nothing below prints it.
wrangler() {
    (
        set -a
        # shellcheck source=/dev/null
        . "$token_file"
        # shellcheck source=/dev/null
        . "$account_file"
        set +a
        export CLOUDFLARE_API_TOKEN=$NEOTELETEXT_CF_TOKEN CLOUDFLARE_ACCOUNT_ID=$CLOUDFLARE_ACCOUNTID
        unset NEOTELETEXT_CF_TOKEN NEOTELETEXT_PROGRAM_SECRET_DEMO
        cd "$crate" && npx --no -- wrangler "$@"
    )
}

# The version serving now: what a rollback returns to.
current_version() {
    wrangler deployments list --name "$worker" --json 2>/dev/null |
        node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const all=JSON.parse(s);const last=all[all.length-1];console.log(last?.versions?.[0]?.version_id??"")})'
}

say "the version to roll back to"
previous=$(current_version)
[[ -n $previous ]] || die "cannot read $worker's deployments"
{
    echo "deployment of $commit to $target, $stamp"
    echo "serving before: $previous"
    echo "roll back with: (cd crates/teletext-cloudflare && npx wrangler rollback $previous --name $worker)"
} | tee "$record"

say "deploying $commit"
wrangler deploy --message "neoteletext-app-server $commit" > "$repo/tmp/cloudflare-deploy-$stamp.log" 2>&1 ||
    { tail -20 "$repo/tmp/cloudflare-deploy-$stamp.log" >&2; die "wrangler deploy failed; nothing changed if it failed before the upload"; }
deployed=$(current_version)
[[ -n $deployed && $deployed != "$previous" ]] || die "no new version is serving (still $previous)"
echo "serving now: $deployed" | tee -a "$record"

failed() { printf 'CHECK FAILED: %s\nroll back with: (cd crates/teletext-cloudflare && npx wrangler rollback %s --name %s)\n' "$*" "$previous" "$worker" >&2; exit 1; }
code() { curl -s -o /dev/null -w '%{http_code}' --max-time 30 "$@"; }
upgrade=(-H 'Upgrade: websocket' -H 'Connection: Upgrade' -H 'Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==' -H 'Sec-WebSocket-Version: 13' --http1.1)

say "checks against $address"
folders=$(node -e '
const text = require("fs").readFileSync(process.argv[1], "utf8");
const line = text.split("\n").find((l) => l.startsWith("CHANNELS = "));
for (const entry of JSON.parse(line.slice("CHANNELS = ".length).replace(/^'"'"'|'"'"'$/g, ""))) console.log(entry.folder);
' "$crate/wrangler.toml")
[[ -n $folders ]] || failed "wrangler.toml lists no channel"
for folder in $folders; do
    [[ $(code "$address/$folder/channel.ttx") == 200 ]] || failed "$folder/channel.ttx"
    [[ $(code "$address/$folder/") == 200 ]] || failed "$folder's small page"
    [[ $(code "${upgrade[@]}" -H 'Origin: https://elsewhere.example' -H "Sec-WebSocket-Protocol: $subprotocol" "$address/$folder/push") == 403 ]] ||
        failed "$folder/push with Origin is not 403"
    [[ $(code "${upgrade[@]}" "$address/$folder/push") == 426 ]] || failed "$folder/push without the subprotocol is not 426"
    [[ $(code "${upgrade[@]}" "$address/$folder/program") == 401 ]] || failed "$folder/program without the secret is not 401"
    "$neottx" probe "$address/$folder/" > "$repo/tmp/cloudflare-probe-$folder-$stamp.txt" 2>&1 || true
    grep -q "handshake *accepted: $subprotocol" "$repo/tmp/cloudflare-probe-$folder-$stamp.txt" ||
        failed "neottx probe of $folder was not accepted (tmp/cloudflare-probe-$folder-$stamp.txt)"
    echo "$folder: channel.ttx, small page, 403, 426, 401 and the probe's handshake as they should be"
done

say "recorded"
# rdm knows the repository by its main checkout, which a worktree shares.
main_checkout=$(dirname "$(git -C "$repo" rev-parse --path-format=absolute --git-common-dir)")
rdm deployed -commit "$commit" -note "Worker version $deployed; rolls back to $previous" "$target" "$main_checkout"
echo "$record"
