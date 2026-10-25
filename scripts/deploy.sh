#!/usr/bin/env bash
# Deploys the server to a Linux host as a systemd user service:
#   scripts/deploy.sh [HOST] [ORIGIN]
# HOST is an ssh name (foundintelquiet4060 by default); ORIGIN is the
# address viewers reach it at (http://192.168.0.166:8090 by default),
# which each channel's push WebSocket is named on. The source and the
# engine's history go to the host, it builds there, the pictures
# (decision 6: never in git) are copied into its tmp/pictures, where
# channels.json has the showcase's programs read them, and the service is
# restarted.
set -euo pipefail
cd "$(dirname "$0")/.."

host="${1:-foundintelquiet4060}"
origin="${2:-http://192.168.0.166:8090}"
listen="0.0.0.0:${origin##*:}"
# As many connections as the host allows (decision 7): its hard limit.
open_files="524288"

engine="$(sed -n 's/^neoteletext-[a-z0-9-]* = { git = "\(file:[^"]*\)".*/\1/p' Cargo.toml | sort -u)"
# The engine's revision, whose tools (neottx, the picture converter) the
# Hackaday channel runs.
engine_rev="$(sed -n 's/^neoteletext-core = .* rev = "\([0-9a-f]*\)".*/\1/p' Cargo.toml)"
[[ -n "$engine_rev" ]] || { echo "Cargo.toml must pin the engine's revision" >&2; exit 1; }
[[ -n "$engine" && "$engine" != *$'\n'* ]] || { echo "Cargo.toml must name one engine repository" >&2; exit 1; }
[[ -f tmp/pictures/captions.tsv ]] || python3 -I scripts/pictures.py

# The channels' own caches and environments stay on the host: a cache is
# the host's knowledge (pictures take 14 s each to make), and an
# environment is built there for its system.
rsync -a --delete --exclude target --exclude tools --exclude .git --exclude tmp \
    --exclude 'channels/*/cache' --exclude .venv --exclude node_modules --exclude __pycache__ ./ "$host:teletext-server/"
rsync -a --delete "${engine#file://}/.git/" "$host:teletext-server-engine.git/"
# tmp is left out of the source's copy above, which therefore keeps it;
# rsync makes only the last folder of a destination.
ssh "$host" mkdir -p teletext-server/tmp
rsync -a --delete tmp/pictures/ "$host:teletext-server/tmp/pictures/"

# shellcheck disable=SC2029 # expanded here on purpose
ssh "$host" "set -e; . ~/.cargo/env; cd teletext-server; rustup toolchain install >/dev/null
    export CARGO_NET_GIT_FETCH_WITH_CLI=true GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=url.file://\$HOME/teletext-server-engine.git.insteadOf GIT_CONFIG_VALUE_0=$engine
    cargo build --quiet --release --locked -p teletext-native
    # The engine's tools at the pinned revision, for the channels that run them.
    [ -d ~/teletext-server-engine-tools/.git ] || git clone --quiet ~/teletext-server-engine.git ~/teletext-server-engine-tools
    git -C ~/teletext-server-engine-tools fetch --quiet origin
    git -C ~/teletext-server-engine-tools checkout --quiet --detach $engine_rev
    (cd ~/teletext-server-engine-tools && rustup toolchain install >/dev/null && cargo build --quiet --release --locked -p neoteletext-cli)
    ~/.local/bin/uv sync --quiet --frozen --no-dev
    mkdir -p ~/.config/systemd/user
    cat > ~/.config/systemd/user/teletext-server.service <<UNIT
[Unit]
Description=NeoTeletext server
After=network-online.target

[Service]
Environment=PATH=%h/.local/bin:/usr/local/bin:/usr/bin:/bin
Environment=UV=%h/.local/bin/uv
Environment=NEOTTX=%h/teletext-server-engine-tools/target/release/neottx
Environment=HACKADAY_CONVERTER=%h/teletext-server-engine-tools/scripts/image-to-teletext.py
ExecStart=%h/teletext-server/target/release/teletext-server --listen $listen --origin $origin --channels %h/teletext-server/channels.json
Restart=on-failure
LimitNOFILE=$open_files

[Install]
WantedBy=default.target
UNIT
    systemctl --user daemon-reload
    systemctl --user enable --now teletext-server >/dev/null 2>&1 || true
    systemctl --user restart teletext-server
    sleep 2
    systemctl --user --no-pager status teletext-server | head -5
    journalctl --user -u teletext-server --no-pager -n 3 -o cat"
echo "deployed: $origin/showcase/ and $origin/hackaday/"
