#!/bin/bash
#
# Provision the neoteletext container for the NeoTeletext app server: the
# lockinatime container's base (unattended security updates, report-only
# needrestart, Caddy from its apt repository with its admin API on a unix
# socket), then what the server's channels need: ffmpeg with libopus (the
# component library encodes every sound as Opus with it) and Node, pinned,
# for the TypeScript channels. Idempotent: every step checks before it acts.
#
# Run inside the container as root:
#   ssh neoteletext sudo bash -s < provision.sh
set -euo pipefail

# Node's release for the TypeScript channels: 22.18 or later runs .ts files
# directly; this is the release the channels are developed on.
NODE_VERSION=v24.21.0
NODE_DIST=https://nodejs.org/dist/$NODE_VERSION

[ "$(id -u)" -eq 0 ] || { echo "run as root" >&2; exit 1; }
export DEBIAN_FRONTEND=noninteractive NEEDRESTART_MODE=l

echo "==> unattended security updates (report-only needrestart, as on the host)"
apt-get -y -qq install unattended-upgrades needrestart >/dev/null
install -d /etc/needrestart/conf.d
cat > /etc/needrestart/conf.d/no-autorestart.conf <<'EOF'
# Report only; never restart services behind our back (same as the host).
$nrconf{restart} = 'l';
EOF

echo "==> ffmpeg with libopus, and the tools the deploy uses"
apt-get -y -qq install ffmpeg sqlite3 xz-utils >/dev/null
# Read whole first: grep -q would stop at its match and pipefail would
# report the SIGPIPE ffmpeg then dies of.
encoders=$(ffmpeg -hide_banner -encoders 2>/dev/null)
grep -q libopus <<<"$encoders" || { echo "ffmpeg has no libopus" >&2; exit 1; }

echo "==> Node $NODE_VERSION, checked against nodejs.org's SHA-256"
if ! [ -x /usr/local/bin/node ] || [ "$(/usr/local/bin/node --version)" != "$NODE_VERSION" ]; then
    work=$(mktemp -d)
    tarball=node-$NODE_VERSION-linux-x64.tar.xz
    curl -fsSL -o "$work/$tarball" "$NODE_DIST/$tarball"
    curl -fsSL -o "$work/SHASUMS256.txt" "$NODE_DIST/SHASUMS256.txt"
    (cd "$work" && grep " $tarball\$" SHASUMS256.txt | sha256sum -c --status) || { echo "$tarball does not match its SHA-256" >&2; exit 1; }
    tar -C /usr/local --strip-components=1 -xJf "$work/$tarball" --exclude CHANGELOG.md --exclude LICENSE --exclude README.md
    rm -r "$work"
fi

echo "==> Caddy from its apt repository"
if [ ! -f /usr/share/keyrings/caddy-stable-archive-keyring.gpg ]; then
    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' > /etc/apt/sources.list.d/caddy-stable.list
    apt-get -qq update
fi
apt-get -y -qq install caddy >/dev/null
# Admin API on a unix socket, never a TCP port (crowdwave lost its site
# once to two Caddys fighting over :2019).
install -d /etc/systemd/system/caddy.service.d
cat > /etc/systemd/system/caddy.service.d/runtimedir.conf <<'EOF'
[Service]
RuntimeDirectory=caddy
RuntimeDirectoryMode=0750
EOF
systemctl daemon-reload

echo "==> versions"
ffmpeg -hide_banner -version | head -1
/usr/local/bin/node --version
caddy version
