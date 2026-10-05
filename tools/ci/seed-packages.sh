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
# postmarketOS's systemd repository's packages we build (phosh) go to their own
DSTS=$W/packages/systemd-$CHANNEL/$ARCH
PUB=systemd/$CHANNEL/$ARCH
P=$(mktemp -d)

git clone -q --depth 1 -b gh-pages \
	"https://x-access-token:$GITHUB_TOKEN@github.com/$GITHUB_REPOSITORY.git" "$P"
if ! [ -f "$P/$PUB/APKINDEX.tar.gz" ]; then
	echo "nothing published yet: building everything"
	exit 0
fi

# owned by the chroots' build user (uid 12345), as pmbootstrap leaves its repository
sudo install -d -o 12345 -g 12345 "$W/packages" "$W/packages/$CHANNEL" "$DST" \
	"$W/packages/systemd-$CHANNEL" "$DSTS"
for f in "$P/$PUB"/*.apk; do
	case "${f##*/}" in
	phosh-[0-9]* | phosh-*-[0-9]* | libphosh*) sudo cp "$f" "$DSTS/" ;;
	*) sudo cp "$f" "$DST/" ;;
	esac
done
sudo chown -R 12345:12345 "$W/packages"
# the published index covers both folders: one of each, from what's in it
pmbootstrap -q index
echo "seeded $(ls "$DST" "$DSTS" | grep -c '\.apk$') published packages"
