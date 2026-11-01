#!/bin/bash
#
# Deploy the NeoTeletext app server into the neoteletext container on
# crowdwaveaudio (create_container.sh, then provision.sh, made it):
#   1. the server, a static build (musl) made on the Linux build host, so it
#      runs on the container's Ubuntu 24.04 whatever that host runs;
#   2. the TypeScript channels and the neoteletext.com home page, copied from
#      their working trees beside this repository, the channels' libraries
#      installed from their vendor/; the old Hackaday channel, Python, with
#      a pinned uv and the engine's neottx for its QR codes; and the Hackaday
#      dataset both Hackaday channels read, and nothing else (DEC-1568);
#   3. the channels file, the service and its socket (a restart refuses no
#      connection, REQ-1041), and the container's Caddy, which serves the
#      home page on neoteletext.com and www and proxies app.neoteletext.com
#      to the server;
#   4. caddyrouter's site block for the three names, checked as a candidate
#      before the live Caddyfile changes.
# DNS is not touched here: the three names are A records, DNS only, to the
# host (74.208.133.103), as lockinatime.com's are.
#
#   deploy/ionos/deploy.sh [--no-build]
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
devel=$(cd "$repo/.." && pwd)
target=neoteletext      # ~/.ssh/config alias: 10.70.0.18 via crowdwaveaudio
edge=caddyrouter        # ~/.ssh/config alias for the TLS edge
builder=${TELETEXT_LINUX_HOST:-foundintelquiet4060}
app=/opt/neoteletext
channels=(neoteletext-server-channel-home-www.neoteletext.com neoteletext-server-channel-hackaday neoteletext-server-channel-demosite neoteletext-server-channel-uicomponentsdemo)
# The old Hackaday channel, Python, beside the Python kit.
python_channels=neoteletext-server-channels
# The development machine's uv, so uv.lock is read by the version that wrote it.
uv_version=0.13.6
# The Hackaday dataset both Hackaday channels read, and only it (DEC-1568):
# the development machine's folder, copied to the container in its own layout.
dataset_source=$devel/neoteletext-hackaday
dataset=$app/data/neoteletext-hackaday
# The engine the server is pinned to builds the neottx the old channel runs.
engine_rev=$(sed -n 's/^neoteletext-core = .*rev = "\([0-9a-f]*\)".*/\1/p' "$repo/Cargo.toml")
musl=x86_64-unknown-linux-musl
stamp=$(date +%Y-%m-%d-%H%M%S)

build=1
[ "${1:-}" = "--no-build" ] && build=0
say() { printf '\n==> %s\n' "$*"; }
die() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }

for c in "${channels[@]}" neoteletext-www "$python_channels"; do [ -d "$devel/$c" ] || die "$devel/$c not found"; done
[ -f "$dataset_source/analysis.sqlite" ] && [ -f "$dataset_source/hackaday.sqlite" ] || die "no Hackaday dataset in $dataset_source"
[ -n "$engine_rev" ] || die "no engine rev in $repo/Cargo.toml"

say "backups on the container, stamped $stamp"
# Data the channels download is left out: nothing below overwrites it.
# shellcheck disable=SC2029
ssh "$target" "set -e; cd ~
    if [ -f $app/bin/teletext-server ]; then cp -a $app/bin/teletext-server teletext-server.bak-$stamp; fi
    if [ -f $app/channels.json ]; then sudo cp -a $app/channels.json channels.json.bak-$stamp; fi
    if [ -d $app/current ]; then sudo tar -C $app/current -czf current.bak-$stamp.tgz --exclude=node_modules --exclude=.venv \
        --exclude='*/hackaday/cache' --exclude='*/hackaday/sounds' --exclude='*/hackaday/people' .; fi
    ls -d ~/*.bak-$stamp*"

if [ "$build" = 1 ]; then
    say "the server, built static on $builder"
    rsync -a --delete --exclude target --exclude tools --exclude .git --exclude fuzz/corpus --exclude fuzz/artifacts \
        --exclude node_modules --exclude .wrangler "$repo/" "$builder:teletext-server-check/"
    # The engine comes from its public repository, which the builder fetches by itself.
    # shellcheck disable=SC2029 # the target is expanded here on purpose
    ssh "$builder" ". ~/.cargo/env; cd teletext-server-check; rustup target add $musl >/dev/null 2>&1
        cargo build --quiet --release --locked --target $musl -p teletext-native"
fi

say "the server binary into $app/bin"
# shellcheck disable=SC2029
ssh "$builder" "cat teletext-server-check/target/$musl/release/teletext-server" |
    ssh "$target" "sudo install -d $app/bin && sudo install -m 755 /dev/stdin $app/bin/teletext-server.new && sudo mv $app/bin/teletext-server.new $app/bin/teletext-server"

say "the channels and the home page into $app/current, the channels' user's"
ssh "$target" "sudo install -d -o ubuntu -g ubuntu $app/current"
for c in "${channels[@]}"; do
    rsync -a --delete --exclude node_modules --exclude tmp --exclude .git --exclude .idea "$devel/$c/" "$target:$app/current/$c/"
done
rsync -a --delete "$devel/neoteletext-www/public/" "$target:$app/current/www/"
# shellcheck disable=SC2029
ssh "$target" "set -e; cd $app/current
    for c in ${channels[*]}; do (cd \$c && npm ci --omit=dev --ignore-scripts --no-audit --no-fund --silent); done"

say "the Hackaday dataset into $dataset"
# Compared by content: the container's copy began as hard links to files
# with their own times. SQLite's -wal and -shm belong to the reader.
# shellcheck disable=SC2029
ssh "$target" "sudo install -d -o ubuntu -g ubuntu $app/data $dataset"
rsync -a --checksum --exclude '*.sqlite-wal' --exclude '*.sqlite-shm' --exclude '*.bak-*' "$dataset_source/" "$target:$dataset/"

say "the old Hackaday channel, uv, and the engine's tools into $app/tools"
# The channel's former downloads (cache) and its local sounds and people are
# the container's own: excluded, so rsync neither sends nor deletes them.
rsync -a --delete --exclude .git --exclude tmp --exclude .venv --exclude __pycache__ --exclude .mypy_cache \
    --exclude .ruff_cache --exclude .pytest_cache --exclude /hackaday/cache --exclude /hackaday/sounds --exclude /hackaday/people \
    "$devel/$python_channels/" "$target:$app/current/$python_channels/"
uv_archive=uv-x86_64-unknown-linux-gnu.tar.gz
uv_release=https://github.com/astral-sh/uv/releases/download/$uv_version
# shellcheck disable=SC2029
ssh "$target" "set -e
    if [ \"\$(uv --version 2>/dev/null)\" != 'uv $uv_version' ]; then
        d=\$(mktemp -d)
        (cd \$d && curl -fsSL -o $uv_archive $uv_release/$uv_archive && curl -fsSL -o $uv_archive.sha256 $uv_release/$uv_archive.sha256 \
            && sha256sum -c $uv_archive.sha256 >/dev/null && tar xzf $uv_archive \
            && sudo install -m 755 uv-x86_64-unknown-linux-gnu/uv uv-x86_64-unknown-linux-gnu/uvx /usr/local/bin/)
        rm -rf \$d
    fi
    # The environment is made afresh, from the lock: a kept one keeps the
    # Python it was made with, a release candidate on the first deploy.
    cd $app/current/$python_channels && uv python install --quiet \$(cat .python-version) && rm -rf .venv && uv sync --quiet --frozen --no-dev"
# A glibc build: the musl one needs a musl C compiler, which the builder
# lacks, for the TLS that neottx's qr and keyboard commands bring.
# shellcheck disable=SC2029
ssh "$builder" ". ~/.cargo/env; set -e; cd ~; [ -d neottx-build ] || git clone --quiet https://github.com/neoteletext/neoteletext-engine neottx-build
    cd neottx-build; git fetch --quiet origin; git checkout --quiet $engine_rev; cargo build --quiet --release --locked -p neoteletext-cli"
# shellcheck disable=SC2029
glibc_needed=$(ssh "$builder" "objdump -T neottx-build/target/release/neottx | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -1")
glibc_there=$(ssh "$target" "ldd --version | head -1 | awk '{print \$NF}'")
[ "$(printf '%s\n%s\n' "$glibc_needed" "$glibc_there" | sort -V | tail -1)" = "$glibc_there" ] ||
    die "neottx needs glibc $glibc_needed; the container has $glibc_there"
# shellcheck disable=SC2029
ssh "$target" "sudo install -d -o ubuntu -g ubuntu $app/tools"
# shellcheck disable=SC2029
ssh "$builder" "cat neottx-build/target/release/neottx" | ssh "$target" "install -m 755 /dev/stdin $app/tools/neottx"

say "the data, once: the Hackaday archive and the pictures from ~/incoming"
# The copy into ~/incoming ends by writing ~/incoming/.complete; nothing is
# moved from a copy still being written.
ssh "$target" 'set -e
    if [ -f ~/incoming/.complete ] && [ -f ~/incoming/neoteletext-transfer/hackaday-analysis/analysis.sqlite ]; then
        mkdir -p ~/hackaday-analysis ~/whatsthatsound/data
        mv ~/incoming/neoteletext-transfer/hackaday-analysis/analysis.sqlite ~/hackaday-analysis/
        [ -d ~/incoming/hackaday-analysis/audio ] && mv ~/incoming/hackaday-analysis/audio ~/hackaday-analysis/
        [ -d ~/incoming/whatsthatsound/data/clips ] && mv ~/incoming/whatsthatsound/data/clips ~/whatsthatsound/data/
    fi
    if [ -f ~/incoming/.complete ] && [ -d ~/incoming/neoteletext-server-channels/tmp/pictures ]; then
        sudo rsync -a --delete ~/incoming/neoteletext-server-channels/tmp/pictures/ /opt/neoteletext/pictures/
    fi
    du -sh ~/hackaday-analysis/analysis.sqlite ~/hackaday-analysis/audio ~/whatsthatsound/data/clips /opt/neoteletext/pictures 2>/dev/null || true'

say "the channels file, the service, its socket and the container's Caddy"
ssh "$target" "sudo tee $app/channels.json >/dev/null" <<EOF
{"channels":[
  {"folder":"home","name":"NeoTeletext","command":["node","src/main.ts"],"directory":"current/neoteletext-server-channel-home-www.neoteletext.com"},
  {"folder":"hackaday","name":"Hackaday","command":["node","--disable-warning=ExperimentalWarning","src/main.ts","--dataset","$dataset"],"directory":"current/neoteletext-server-channel-hackaday"},
  {"folder":"oldhackaday","name":"Hackaday (old)","command":["env","NEOTTX=$app/tools/neottx","uv","run","--frozen","--no-dev","python","-m","hackaday","--dataset","$dataset"],"directory":"current/$python_channels/hackaday"},
  {"folder":"demosite","name":"NeoTeletext demo site","command":["node","src/main.ts","--pictures","$app/pictures"],"directory":"current/neoteletext-server-channel-demosite"},
  {"folder":"uicomponentsdemo","name":"NeoTeletext UI components demo","command":["node","src/main.ts","--pictures","$app/pictures"],"directory":"current/neoteletext-server-channel-uicomponentsdemo"}
]}
EOF
ssh "$target" "sudo tee /etc/systemd/system/neoteletext.socket >/dev/null" < "$here/neoteletext.socket"
ssh "$target" "sudo tee /etc/systemd/system/neoteletext.service >/dev/null" <<EOF
[Unit]
Description=NeoTeletext app server
After=network-online.target neoteletext.socket
# systemd holds the listening socket across a restart (REQ-1041).
Requires=neoteletext.socket

[Service]
# ubuntu, whose home holds the Hackaday channel's archive at its default paths.
User=ubuntu
Environment=PATH=/usr/local/bin:/usr/bin:/bin
# What survives a restart (teletext-server requirement 5.2), in /var/lib/neoteletext.
StateDirectory=neoteletext
ExecStart=$app/bin/teletext-server --listen 127.0.0.1:8090 --origin https://app.neottx.com --channels $app/channels.json --admin-listen 127.0.0.1:9464
Restart=on-failure
# The server stops within 10 s of SIGTERM (requirement 5.1).
TimeoutStopSec=15
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
EOF
ssh "$target" "sudo tee /etc/caddy/Caddyfile >/dev/null" < "$here/Caddyfile"
ssh "$target" "set -e; sudo systemctl daemon-reload
    # Once, the first time: a server that bound the address itself must go
    # before the socket can. After that the socket stays open and only the
    # service restarts, so no connection is refused.
    systemctl is-active --quiet neoteletext.socket || sudo systemctl stop neoteletext
    sudo systemctl enable --now neoteletext.socket neoteletext >/dev/null 2>&1; sudo systemctl restart neoteletext
    # As the caddy user: run as root, validate creates the log file root's, and
    # the service, running as caddy, then cannot open it.
    sudo runuser -u caddy -- caddy validate --config /etc/caddy/Caddyfile >/dev/null 2>&1 || { echo 'the container Caddyfile does not validate' >&2; exit 1; }
    sudo systemctl enable caddy >/dev/null 2>&1; sudo systemctl restart caddy"

say "caddyrouter: the neoteletext site block"
# The block between its BEGIN and END lines is replaced whole (or the first
# deploy's unmarked neoteletext.com block, to its closing brace, or
# appended), in a candidate that must validate before the live Caddyfile
# changes. A marked block ends at its END line alone: ending it at the
# closing brace left the old END behind, one more on every deploy, and
# those strays are dropped.
# shellcheck disable=SC2016 # expanded on caddyrouter
ssh "$edge" 'set -e; block=$(mktemp); cat > "$block"; candidate=$(mktemp)
    awk -v block="$block" '"'"'
        /^# BEGIN neoteletext / { skip = "end" }
        /^# neoteletext.com: its own nspawn container/ { skip = "brace" }
        (skip == "end" && /^# END neoteletext/) || (skip == "brace" && /^}$/) { skip = ""; if (!done) { while ((getline line < block) > 0) print line; done = 1 }; next }
        !skip && /^# END neoteletext/ { next }
        !skip { print }
        END { if (!done) while ((getline line < block) > 0) print line }
    '"'"' /etc/caddy/Caddyfile > "$candidate"; chmod 644 "$candidate"
    if cmp -s "$candidate" /etc/caddy/Caddyfile; then echo "unchanged"; rm -f "$block" "$candidate"; exit 0; fi
    sudo runuser -u caddy -- caddy validate --adapter caddyfile --config "$candidate" >/dev/null 2>&1 || { echo "the candidate Caddyfile does not validate" >&2; rm -f "$block" "$candidate"; exit 1; }
    sudo cp -a /etc/caddy/Caddyfile /etc/caddy/Caddyfile.bak-$(date +%Y-%m-%d-%H%M%S)
    sudo cp "$candidate" /etc/caddy/Caddyfile; rm -f "$block" "$candidate"
    sudo systemctl reload caddy && echo "replaced and reloaded"' < "$here/caddyrouter.caddy"

say "inside the container"
ssh "$target" 'sleep 3; systemctl is-active neoteletext.socket neoteletext caddy; curl -s -o /dev/null -w "server %{http_code}\n" http://127.0.0.1:8090/; curl -s -o /dev/null -w "readyz %{http_code}\n" http://127.0.0.1:9464/readyz
    for h in www.neottx.com neottx.com app.neottx.com app.neoteletext.com neoteletext.com; do curl -s -o /dev/null -w "$h via Caddy %{http_code}\n" -H "Host: $h" http://10.70.0.18:8080/; done
    for c in home hackaday oldhackaday demosite uicomponentsdemo; do curl -s -o /dev/null -w "$c channel.ttx %{http_code}\n" http://127.0.0.1:8090/$c/channel.ttx; done'
