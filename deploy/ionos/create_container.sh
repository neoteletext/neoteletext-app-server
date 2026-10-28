#!/bin/bash
#
# Create the neoteletext systemd-nspawn container on crowdwaveaudio, for
# the NeoTeletext app server, built as the lockinatime container was
# (~/devel/lockinatime.com/ionos/deploy/create_container.sh), with its own
# name and addresses.
#
# Unlike the crowdwave containers this one has its OWN network namespace
# (VirtualEthernet=yes): nothing it binds is reachable from the internet
# or from the other containers unless the host routes it. Its address is
# fixed so caddyrouter can proxy to it:
#
#   host side   ve-neoteletext  10.70.0.17/28  (masquerades the container out)
#   container   host0           10.70.0.18/28  gateway 10.70.0.17
#
# lockinatime holds 10.70.0.0/28; this is the next block, 10.70.0.16/28.
#
# SSH is only reachable through the host (ProxyJump crowdwaveaudio), so
# no ufw hole is opened for it.
#
# Run as root on the host:
#   sudo AUTHORIZED_KEYS_FILE=/home/ubuntu/.ssh/authorized_keys ./create_container.sh
#
# The phases run in order; START_AT=<phase> resumes a run that stopped
# part way (phase names are printed as "==> name:"). A fresh run refuses
# to overwrite an existing container.
set -euo pipefail

NAME=neoteletext
HOST_ADDR=10.70.0.17
CONTAINER_ADDR=10.70.0.18
PREFIX=28
# IONOS resolvers, what the host's own link gets from DHCP (resolvectl dns ens6).
DNS_SERVERS="212.227.123.16 212.227.123.17"
ADMIN_IP=115.70.80.224
UBUNTU_SUITE=noble
TIMEZONE=Australia/Melbourne
USERNAME=ubuntu
MIRROR=http://archive.ubuntu.com/ubuntu/

DIR=/var/lib/machines/$NAME
NSPAWN=/etc/systemd/nspawn/$NAME.nspawn
VE=ve-$NAME # nspawn names the host side of the veth ve-<name>, max 15 chars

[ "$(id -u)" -eq 0 ] || { echo "run as root" >&2; exit 1; }
: "${AUTHORIZED_KEYS_FILE:?set AUTHORIZED_KEYS_FILE to a file of public keys for $USERNAME}"
[ -r "$AUTHORIZED_KEYS_FILE" ] || { echo "cannot read $AUTHORIZED_KEYS_FILE" >&2; exit 1; }
systemctl is-active --quiet systemd-networkd || { echo "systemd-networkd must manage the host's links" >&2; exit 1; }

START_AT=${START_AT:-}
if [ -z "$START_AT" ]; then
    [ -e "$DIR" ] && { echo "$DIR exists (START_AT=<phase> to resume)" >&2; exit 1; }
    [ -e "$NSPAWN" ] && { echo "$NSPAWN exists" >&2; exit 1; }
fi
skipping=${START_AT:+yes}
# phase NAME: true once the run has reached NAME (or from the start when
# START_AT is unset), so each block below is `if phase x; then ... fi`.
phase() {
    if [ -n "$skipping" ]; then
        [ "$1" = "$START_AT" ] && skipping=
    fi
    if [ -z "$skipping" ]; then
        echo "==> $1:"
        return 0
    fi
    return 1
}

if phase debootstrap; then
    apt-get -y -qq install debootstrap systemd-container >/dev/null
    mkdir -p "$DIR"
    debootstrap --variant=minbase --arch amd64 \
        --include=systemd,systemd-container,systemd-resolved,dbus,sudo,apt,gnupg,ca-certificates,curl,iproute2,iputils-ping,openssh-server,chrony,tzdata,vim,less,htop,rsync,python3 \
        "$UBUNTU_SUITE" "$DIR" "$MIRROR"
fi

if phase sources; then
    # debootstrap writes a sources.list with only "$UBUNTU_SUITE main": no
    # updates, no security. kingswood ran unpatched for months because of
    # exactly this. Fix it before the first apt run inside the container.
    cat > "$DIR/etc/apt/sources.list" <<EOF
deb $MIRROR $UBUNTU_SUITE main universe
deb $MIRROR $UBUNTU_SUITE-updates main universe
deb http://security.ubuntu.com/ubuntu $UBUNTU_SUITE-security main universe
EOF
fi

if phase identity; then
    echo "$NAME" > "$DIR/etc/hostname"
    printf '127.0.0.1 localhost\n127.0.1.1 %s\n' "$NAME" > "$DIR/etc/hosts"
    chroot "$DIR" ln -sf "/usr/share/zoneinfo/$TIMEZONE" /etc/localtime
fi

if phase container-network; then
    mkdir -p "$DIR/etc/systemd/network"
    # Named to sort before the stock 80-container-host0.network, which would
    # otherwise match host0 first and run DHCP instead.
    cat > "$DIR/etc/systemd/network/10-host0.network" <<EOF
[Match]
Name=host0

[Network]
Address=$CONTAINER_ADDR/$PREFIX
Gateway=$HOST_ADDR
DNS=${DNS_SERVERS// / DNS=}
EOF
    # Both are installed by debootstrap; enabling them is what makes the
    # static address and the resolver come up at boot.
    chroot "$DIR" systemctl enable systemd-networkd systemd-resolved >/dev/null 2>&1
    ln -sf /run/systemd/resolve/stub-resolv.conf "$DIR/etc/resolv.conf"
fi

if phase host-network; then
    # Named to sort before the stock 80-container-ve.network, which matches
    # every ve-* link and would hand this one a random /28 and a DHCP server.
    cat > "/etc/systemd/network/10-$VE.network" <<EOF
# Host end of the neoteletext container's veth. Fixed address so
# caddyrouter can reach the container at $CONTAINER_ADDR; IPMasquerade lets
# the container reach the internet (apt, and the channels' own fetching).
[Match]
Name=$VE
Kind=veth

[Network]
Address=$HOST_ADDR/$PREFIX
IPMasquerade=ipv4
LinkLocalAddressing=no
IPv6AcceptRA=no
EOF
    networkctl reload
    # ufw's forward policy is DROP; masquerading needs the container's
    # packets to be forwarded from the veth to ens6.
    ufw status | grep -q "$VE" || ufw route allow in on "$VE" out on ens6 comment "$NAME container outbound" >/dev/null
fi

if phase nspawn-file; then
    mkdir -p /etc/systemd/nspawn
    cat > "$NSPAWN" <<EOF
# neoteletext: own network namespace, own uid range. See
# ~/devel/neoteletext-app-server/deploy/ionos/create_container.sh.
[Exec]
PrivateUsers=yes
NotifyReady=yes

[Network]
VirtualEthernet=yes

[Files]
TemporaryFileSystem=/tmp
EOF
fi

if phase sshd; then
    # Written before the uid shift below, so the files belong to the
    # container's root rather than to "nobody".
    cat > "$DIR/etc/ssh/sshd_config.d/10-$NAME.conf" <<EOF
Port 22
ListenAddress $CONTAINER_ADDR
PermitRootLogin no
PasswordAuthentication no
KbdInteractiveAuthentication no
PubkeyAuthentication yes
MaxAuthTries 3
AllowUsers $USERNAME
EOF
    # Ubuntu's ssh.socket ignores sshd_config's Port/ListenAddress; the
    # service form reads them.
    chroot "$DIR" systemctl disable ssh.socket >/dev/null 2>&1 || true
    chroot "$DIR" systemctl enable ssh >/dev/null 2>&1
fi

if phase user; then
    # The first nspawn with --private-users=pick shifts the whole tree
    # into the container's uid range. Everything the host writes into
    # the tree after this point is owned by "nobody" inside, so the key
    # file is staged world-readable and installed from within.
    install -m 644 "$AUTHORIZED_KEYS_FILE" "$DIR/root/authorized_keys.staged"
    systemd-nspawn -q -D "$DIR" --private-users=pick --private-users-ownership=chown /bin/bash -c "
        id $USERNAME >/dev/null 2>&1 || adduser --disabled-password --gecos '' $USERNAME
        usermod -aG sudo $USERNAME
        install -d -m 755 /etc/sudoers.d
        echo '%sudo ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/ALL_NOPASSWD
        chmod 440 /etc/sudoers.d/ALL_NOPASSWD
        install -d -m 700 -o $USERNAME -g $USERNAME /home/$USERNAME/.ssh
        install -m 600 -o $USERNAME -g $USERNAME /root/authorized_keys.staged /home/$USERNAME/.ssh/authorized_keys
        rm -f /root/authorized_keys.staged
    "
fi

if phase start; then
    machinectl enable "$NAME" >/dev/null
    machinectl start "$NAME"
    for _ in $(seq 1 30); do
        systemctl is-active --quiet "systemd-nspawn@$NAME" && break
        sleep 1
    done
    systemctl is-active --quiet "systemd-nspawn@$NAME" || { echo "container did not start; journalctl -u systemd-nspawn@$NAME" >&2; exit 1; }
    # Give networkd inside a moment to bring host0 up before apt needs it.
    for _ in $(seq 1 15); do
        ping -c1 -W1 "$CONTAINER_ADDR" >/dev/null 2>&1 && break
        sleep 1
    done
fi

if phase update; then
    systemd-run -q --wait --machine="$NAME" --pipe /bin/bash -c "
        export DEBIAN_FRONTEND=noninteractive
        apt-get -qq update
        apt-get -y -qq upgrade
        apt-get -y -qq install fail2ban
        cat > /etc/fail2ban/jail.local <<JAIL
[DEFAULT]
ignoreip = 127.0.0.1/8 ::1 $HOST_ADDR $ADMIN_IP
bantime  = 3600
findtime = 600
maxretry = 5
backend  = systemd

[sshd]
enabled = true
JAIL
        systemctl enable --now fail2ban
    " || echo "WARNING: the first apt run inside the container failed; check its network" >&2
fi

cat <<EOF

$NAME is up. Check:
    machinectl list
    networkctl status $VE
    sudo machinectl shell $NAME /bin/bash -c 'ip -br addr; resolvectl query archive.ubuntu.com | head -2'
    ssh -J crowdwaveaudio $USERNAME@$CONTAINER_ADDR

~/.ssh/config on the laptop:
    Host $NAME
      Hostname $CONTAINER_ADDR
      User $USERNAME
      ProxyJump crowdwaveaudio
      IdentityFile ~/devel/certs/templates_www_ohio.pem
EOF
