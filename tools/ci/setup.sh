#!/bin/sh
# Set up pmbootstrap non-interactively for building the L16 packages (CI): pmbootstrap
# itself, pmaports on the release channel, a config for light-lfc, and the package
# signing key from $APK_SIGNING_KEY.
set -eu
: "${APK_SIGNING_KEY:?the package signing key (secret APK_SIGNING_KEY) is not set}"
PMB_VERSION=${PMB_VERSION:-3.11.1}
CHANNEL=${CHANNEL:-v26.06}
KEY_NAME=l16-linux@artillect-6ab9d2e0.rsa
REPO=$(cd "$(dirname "$0")/../.." && pwd)
W=$HOME/.local/var/pmbootstrap

git clone -q --depth 1 -b "$PMB_VERSION" \
	https://gitlab.postmarketos.org/postmarketOS/pmbootstrap.git "$HOME/pmbootstrap"
mkdir -p "$HOME/.local/bin"
ln -sf "$HOME/pmbootstrap/pmbootstrap.py" "$HOME/.local/bin/pmbootstrap"

# the work folder, as "pmbootstrap init" would leave it
mkdir -p "$W/cache_git" "$HOME/.config"
git clone -q --depth 1 -b "$CHANNEL" \
	https://gitlab.postmarketos.org/postmarketOS/pmaports.git "$W/cache_git/pmaports"
# pmbootstrap reads channels.cfg from origin/main
git -C "$W/cache_git/pmaports" fetch -q --depth 1 origin main:refs/remotes/origin/main
python3 -c "import sys; sys.path.insert(0, '$HOME/pmbootstrap'); import pmb.config; print(pmb.config.work_version)" \
	> "$W/version"
cat > "$HOME/.config/pmbootstrap_v3.cfg" <<EOF
[pmbootstrap]
device = light-lfc
extra_packages = l16-camera,l16-gallery
hostname = light-lfc
is_default_channel = False
service_manager = openrc
ssh_keys = False
timezone = UTC
ui = phosh
user = user

[providers]

[mirrors]
EOF

# sign with the release key (the chroots' build user is uid 12345), and trust it
pub="$REPO/pmaports/device/testing/device-light-lfc/$KEY_NAME.pub"
sudo install -d -o 12345 -g 12345 "$W/config_abuild"
printf '%s\n' "$APK_SIGNING_KEY" | sudo install -o 12345 -g 12345 -m 600 /dev/stdin "$W/config_abuild/$KEY_NAME"
sudo install -o 12345 -g 12345 -m 644 "$pub" "$W/config_abuild/"
echo "PACKAGER_PRIVKEY=\"/home/pmos/.abuild/$KEY_NAME\"" |
	sudo install -o 12345 -g 12345 -m 644 /dev/stdin "$W/config_abuild/abuild.conf"
sudo install -d "$W/config_apk_keys"
sudo install -m 644 "$pub" "$W/config_apk_keys/"

# the L16 packages into pmaports
PATH=$HOME/.local/bin:$PATH bash "$REPO/pmaports/sync.sh"
