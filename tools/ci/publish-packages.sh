#!/bin/sh
# Publish the packages pmbootstrap built (signed, indexed) to the gh-pages branch, as the
# apk repository https://artillect.github.io/l16-linux/<channel>. The branch is rewritten
# as a single commit each time, so old package builds don't pile up in git history.
# needs: GITHUB_TOKEN, GITHUB_REPOSITORY
set -eu
CHANNEL=${CHANNEL:-v26.06}
ARCH=aarch64
SRC=$HOME/.local/var/pmbootstrap/packages/$CHANNEL/$ARCH
P=$(mktemp -d)

git clone -q --depth 1 -b gh-pages \
	"https://x-access-token:$GITHUB_TOKEN@github.com/$GITHUB_REPOSITORY.git" "$P"
rm -rf "$P/$CHANNEL/$ARCH"
mkdir -p "$P/$CHANNEL/$ARCH"
cp "$SRC"/*.apk "$SRC/APKINDEX.tar.gz" "$P/$CHANNEL/$ARCH/"
ls -l "$P/$CHANNEL/$ARCH"

cd "$P"
git checkout -q --orphan publish
git add -A
git -c user.name="github-actions[bot]" -c user.email="41898282+github-actions[bot]@users.noreply.github.com" \
	commit -q -m "Packages from $GITHUB_REPOSITORY@${GITHUB_SHA:-local}"
git push -q -f origin publish:gh-pages
