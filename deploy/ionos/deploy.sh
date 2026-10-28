#!/bin/bash
#
# Deploy the NeoTeletext app server into the neoteletext container on
# crowdwaveaudio (create_container.sh, then provision.sh, made it):
#   1. the server, a static build (musl) made on the Linux build host, so it
#      runs on the container's Ubuntu 24.04 whatever that host runs;
#   2. the TypeScript channels and the neoteletext.com home page, copied from
#      their working trees beside this repository, the channels' libraries
#      installed from their vendor/;
#   3. the channels file, the service, and the container's Caddy, which
#      serves the home page on neoteletext.com and www and proxies
#      app.neoteletext.com to the server;
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
channels=(neoteletext-server-channel-hackaday neoteletext-server-channel-demosite neoteletext-server-channel-uicomponentsdemo)
musl=x86_64-unknown-linux-musl

build=1
[ "${1:-}" = "--no-build" ] && build=0
say() { printf '\n==> %s\n' "$*"; }
die() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }

for c in "${channels[@]}" neoteletext-www; do [ -d "$devel/$c" ] || die "$devel/$c not found"; done

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
    rsync -a --delete --exclude node_modules --exclude tmp --exclude .git "$devel/$c/" "$target:$app/current/$c/"
done
rsync -a --delete "$devel/neoteletext-www/public/" "$target:$app/current/www/"
# shellcheck disable=SC2029
ssh "$target" "set -e; cd $app/current
    for c in ${channels[*]}; do (cd \$c && npm ci --omit=dev --ignore-scripts --no-audit --no-fund --silent); done"

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

say "the channels file, the service and the container's Caddy"
ssh "$target" "sudo tee $app/channels.json >/dev/null" <<EOF
{"channels":[
  {"folder":"hackaday","name":"Hackaday","command":["node","--disable-warning=ExperimentalWarning","src/main.ts"],"directory":"current/neoteletext-server-channel-hackaday"},
  {"folder":"demosite","name":"NeoTeletext demo site","command":["node","src/main.ts","--pictures","$app/pictures"],"directory":"current/neoteletext-server-channel-demosite"},
  {"folder":"uicomponentsdemo","name":"NeoTeletext UI components demo","command":["node","src/main.ts","--pictures","$app/pictures"],"directory":"current/neoteletext-server-channel-uicomponentsdemo"}
]}
EOF
ssh "$target" "sudo tee /etc/systemd/system/neoteletext.service >/dev/null" <<EOF
[Unit]
Description=NeoTeletext app server
After=network-online.target

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
ssh "$target" "set -e; sudo systemctl daemon-reload; sudo systemctl enable --now neoteletext >/dev/null 2>&1; sudo systemctl restart neoteletext
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
ssh "$target" 'sleep 3; systemctl is-active neoteletext caddy; curl -s -o /dev/null -w "server %{http_code}\n" http://127.0.0.1:8090/; curl -s -o /dev/null -w "readyz %{http_code}\n" http://127.0.0.1:9464/readyz
    for h in www.neottx.com neottx.com app.neottx.com app.neoteletext.com neoteletext.com; do curl -s -o /dev/null -w "$h via Caddy %{http_code}\n" -H "Host: $h" http://10.70.0.18:8080/; done'
