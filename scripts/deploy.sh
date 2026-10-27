#!/usr/bin/env bash
# Deploys the server and its channels to a Linux host as a systemd user
# service:
#   scripts/deploy.sh [HOST] [ORIGIN]
# HOST is an ssh name (foundintelquiet4060 by default); ORIGIN is the
# address viewers reach it at (http://192.168.0.166:8090 by default),
# which each channel's push WebSocket is named on. The source and the
# engine's history go to the host, it builds there; the channels'
# repository (decision 12; TELETEXT_CHANNELS, ~/devel/neoteletext-server-channels
# by default) goes beside it, the pictures (decision 6: never in git) into
# its tmp/pictures, where its channels.json has the showcase's programs
# read them, and the service is restarted with that channels.json.
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
channels="${TELETEXT_CHANNELS:-$HOME/devel/neoteletext-server-channels}"
[[ -f "$channels/channels.json" ]] || { echo "$channels has no channels.json; TELETEXT_CHANNELS names the channels' repository" >&2; exit 1; }
[[ -f "$channels/tmp/pictures/captions.tsv" ]] || (cd "$channels" && python3 -I scripts/pictures.py)

# Once, on the first deploy since decision 12: the Hackaday channel's
# archive, which took a day to download, moves from the server's folder to
# the channels' before either copy below can remove it.
# shellcheck disable=SC2016 # expanded on the host
ssh "$host" 'set -e; old=teletext-server/channels/hackaday; new=neoteletext-server-channels/hackaday
    if [ -d "$old/cache" ] && [ ! -d "$new/cache" ]; then
        mkdir -p "$new"
        for folder in cache sounds people; do [ ! -d "$old/$folder" ] || mv "$old/$folder" "$new/$folder"; done
    fi'

# The channels' own caches and environments stay on the host: a cache is
# the host's knowledge (pictures take 14 s each to make), and an
# environment is built there for its system.
rsync -a --delete --exclude target --exclude tools --exclude .git --exclude tmp ./ "$host:teletext-server/"
rsync -a --delete --exclude .git --exclude tmp --exclude 'hackaday/cache' --exclude 'hackaday/sounds' --exclude 'hackaday/people' \
    --exclude .venv --exclude node_modules --exclude __pycache__ --exclude .mypy_cache --exclude .ruff_cache --exclude .pytest_cache \
    "$channels/" "$host:neoteletext-server-channels/"
rsync -a --delete "${engine#file://}/.git/" "$host:teletext-server-engine.git/"
# tmp is left out of the source's copy above, which therefore keeps it;
# rsync makes only the last folder of a destination.
ssh "$host" mkdir -p teletext-server/tmp neoteletext-server-channels/tmp
rsync -a --delete "$channels/tmp/pictures/" "$host:neoteletext-server-channels/tmp/pictures/"

# shellcheck disable=SC2029 # expanded here on purpose
ssh "$host" "set -e; . ~/.cargo/env; cd teletext-server; rustup toolchain install >/dev/null
    export CARGO_NET_GIT_FETCH_WITH_CLI=true GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=url.file://\$HOME/teletext-server-engine.git.insteadOf GIT_CONFIG_VALUE_0=$engine
    cargo build --quiet --release --locked -p teletext-native
    # The engine's tools at the pinned revision, for the channels that run them.
    [ -d ~/teletext-server-engine-tools/.git ] || git clone --quiet ~/teletext-server-engine.git ~/teletext-server-engine-tools
    git -C ~/teletext-server-engine-tools fetch --quiet origin
    git -C ~/teletext-server-engine-tools checkout --quiet --detach $engine_rev
    (cd ~/teletext-server-engine-tools && rustup toolchain install >/dev/null && cargo build --quiet --release --locked -p neoteletext-cli)
    (cd ~/neoteletext-server-channels && ~/.local/bin/uv sync --quiet --frozen --no-dev)
    # The TypeScript channels' libraries, released into the channels' vendor/ (decision 13).
    (cd ~/neoteletext-server-channels && npm ci --omit=dev --ignore-scripts --no-audit --no-fund --silent)
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
ExecStart=%h/teletext-server/target/release/teletext-server --listen $listen --origin $origin --channels %h/neoteletext-server-channels/channels.json
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
