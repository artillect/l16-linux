#!/bin/sh
# Seed pmbootstrap's local repository with the packages already published (gh-pages), so
# "pmbootstrap build" only builds those whose pkgver-pkgrel changed: it compares each
# APKBUILD's version with the local repository's index and skips a package already there.
# needs: GITHUB_TOKEN, GITHUB_REPOSITORY; run after setup.sh
set -eu
CHANNEL=${CHANNEL:-v26.06}
ARCH=aarch64
W=$HOME/.local/var/pmbootstrap
DST=$W/packages/$CHANNEL/$ARCH
P=$(mktemp -d)

git clone -q --depth 1 -b gh-pages \
	"https://x-access-token:$GITHUB_TOKEN@github.com/$GITHUB_REPOSITORY.git" "$P"
if ! [ -f "$P/$CHANNEL/$ARCH/APKINDEX.tar.gz" ]; then
	echo "nothing published yet: building everything"
	exit 0
fi

# owned by the chroots' build user (uid 12345), as pmbootstrap leaves its repository
sudo install -d -o 12345 -g 12345 "$W/packages" "$W/packages/$CHANNEL" "$DST"
sudo cp "$P/$CHANNEL/$ARCH"/*.apk "$P/$CHANNEL/$ARCH/APKINDEX.tar.gz" "$DST/"
sudo chown -R 12345:12345 "$W/packages"
echo "seeded $(ls "$DST" | grep -c '\.apk$') published packages"
