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

# generic user with a well-known password the notes tell people to change; no ssh
pmbootstrap -y install --single-partition --no-sshd --password 147147

sudo cp "$W/chroot_rootfs_light-lfc/boot/boot.img" "$O/light-lfc-boot.img"
xz -T0 -6 -c "$W/chroot_native/home/pmos/rootfs/light-lfc.img" > "$O/light-lfc-rootfs.img.xz"
sudo chown "$(id -u):$(id -g)" "$O"/*
(cd "$O" && sha256sum light-lfc-boot.img light-lfc-rootfs.img.xz > SHA256SUMS && cat SHA256SUMS)

flag=
[ "$PRERELEASE" = true ] && flag=--prerelease
gh release create "$TAG" "$O/light-lfc-rootfs.img.xz" "$O/light-lfc-boot.img" "$O/SHA256SUMS" \
	--target "${GITHUB_SHA:-main}" --title "$TAG" --notes-file "$NOTES" $flag
