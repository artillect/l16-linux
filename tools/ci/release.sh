#!/bin/sh
# Build the prebuilt image from the packages just built and publish it as a GitHub
# release, with notes from releases/<tag>.md.
# usage: release.sh <tag> [prerelease: true|false]    needs: GH_TOKEN
set -eu
TAG=$1
PRERELEASE=${2:-true}
REPO=$(cd "$(dirname "$0")/../.." && pwd)
W=$HOME/.local/var/pmbootstrap
NOTES=$REPO/releases/$TAG.md
O=$(mktemp -d)
[ -f "$NOTES" ] || { echo "no release notes: releases/$TAG.md"; exit 1; }

# The apps: postmarketOS's recommended ones (postmarketos-ui-phosh, -base-ui-gnome,
# -base-ui-gnome-mobile, -base-ui, -base) without a phone's (calls, chatty, gnome-contacts),
# a camera app that can't see the cameras (snapshot: no PipeWire cameras), fprintd (no
# fingerprint reader) and rygel (a media server). pmbootstrap installs all recommends or
# none, so they're listed: a renamed one fails the build rather than going missing.
APPS="sudo-rs
font-droid font-droid-nonlatin font-twemoji lang
phosh-mobile-settings phosh-tour
mobile-config-firefox postmarketos-tweaks-setting-definitions ttyescape
decibels firefox-esr flatpak g4music gnome-calculator gnome-calendar gnome-clocks
gnome-console gnome-maps gnome-software gnome-software-plugin-apk gnome-text-editor
gnome-user-share gnome-weather gst-libav gst-plugins-bad gst-plugins-good
gst-plugins-rs-dav1d gvfs-full loupe nautilus papers showtime tuned-ppd"

# generic user with a well-known password the notes tell people to change; no ssh
pmbootstrap -y install --single-partition --no-sshd --password 147147 \
	--no-recommends --add "$(echo $APPS | tr ' ' ',')"

IMG=$W/chroot_native/home/pmos/rootfs/light-lfc.img
sudo cp "$W/chroot_rootfs_light-lfc/boot/boot.img" "$O/light-lfc-boot.img"
xz -T0 -6 -c "$IMG" > "$O/light-lfc-rootfs.img.xz"

# the image's manifest, for tools/fresh-check: from the image itself (pmbootstrap writes some
# files only there, as fstab), which is sparse, so unpacked to a raw copy first
sudo apt-get install -y -q android-sdk-libsparse-utils
M=$(mktemp -d)
simg2img "$IMG" "$O/raw.img"
sudo mount -o loop,ro "$O/raw.img" "$M"
sudo APK="$W/apk.static" sh "$REPO/tools/fresh-manifest" "$M" > "$O/light-lfc-manifest.txt"
sudo umount "$M"
rm "$O/raw.img"

sudo chown "$(id -u):$(id -g)" "$O"/*
# (the image files only: the wiki's `sha256sum -c` would fail on a manifest not downloaded)
(cd "$O" && sha256sum light-lfc-boot.img light-lfc-rootfs.img.xz > SHA256SUMS && cat SHA256SUMS)

flag=
[ "$PRERELEASE" = true ] && flag=--prerelease
gh release create "$TAG" "$O/light-lfc-rootfs.img.xz" "$O/light-lfc-boot.img" \
	"$O/light-lfc-manifest.txt" "$O/SHA256SUMS" \
	--target "${GITHUB_SHA:-main}" --title "$TAG" --notes-file "$NOTES" $flag
