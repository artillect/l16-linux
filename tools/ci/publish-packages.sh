#!/bin/sh
# Publish the packages pmbootstrap built to the gh-pages branch, as the apk repository
# https://artillect.github.io/l16-linux/systemd/<channel> (0.1.x, on OpenRC, keep <channel>/
# beside it as it is). The newest builds are added to what's
# already published, the newest KEEP versions of each package are kept (so a bad update
# can be undone with "apk add <pkg>=<version>"), and the index is rebuilt and signed.
# The branch is rewritten as a single commit each time, so old builds don't pile up in
# git history.
# needs: GITHUB_TOKEN, GITHUB_REPOSITORY; the signing key set up by setup.sh
set -eu
CHANNEL=${CHANNEL:-v26.06}
ARCH=aarch64
KEEP=${KEEP:-3}
W=$HOME/.local/var/pmbootstrap
P=$(mktemp -d)
R=$P/systemd/$CHANNEL/$ARCH

git clone -q --depth 1 -b gh-pages \
	"https://x-access-token:$GITHUB_TOKEN@github.com/$GITHUB_REPOSITORY.git" "$P"
mkdir -p "$R"
# pmbootstrap's builds, and those of postmarketOS's systemd repository's packages (phosh)
cp "$W/packages/$CHANNEL/$ARCH"/*.apk "$R/"
cp "$W/packages/systemd-$CHANNEL/$ARCH"/*.apk "$R/" 2>/dev/null || :

# keep the newest $KEEP versions of each package
for name in $(ls "$R" | sed -n -E 's/-[0-9][^-]*-r[0-9]+\.apk$//p' | sort -u); do
	ls "$R" | grep -E "^$name-[0-9][^-]*-r[0-9]+\.apk$" | sort -V | head -n -"$KEEP" |
		while read -r old; do
			echo "dropping $old"
			rm "${R:?}/${old:?}"
		done
done

# index and sign the whole folder in pmbootstrap's native chroot, as pmbootstrap does
S=$W/chroot_native/home/pmos/l16pub
sudo rm -rf "${S:?}"
sudo mkdir -p "$S"
sudo cp "$R"/*.apk "$S/"
sudo chown -R 12345:12345 "$S"
pmbootstrap -q chroot --user --add abuild -- sh -c \
	"cd /home/pmos/l16pub && apk -q index --no-warnings --output APKINDEX.tar.gz --description l16-linux --rewrite-arch $ARCH *.apk && abuild-sign APKINDEX.tar.gz"
cp "$S/APKINDEX.tar.gz" "$R/"
ls -l "$R"

# what each update changed (from this checkout's history), linked from the index page
python3 tools/ci/changelog.py "$P/changes.html"
grep -q 'l16-linux/systemd/' "$P/index.html" ||
	sed -i "s#^<p>Repository: .*<br>\$#<p>Repository: <code>https://artillect.github.io/l16-linux/systemd/$CHANNEL</code> (0.1.x installs, on OpenRC: <code>https://artillect.github.io/l16-linux/$CHANNEL</code>)<br>#" "$P/index.html"
grep -q 'href="changes.html"' "$P/index.html" ||
	sed -i 's#^Signing key: \(.*\)</p>$#Signing key: \1<br>\n<a href="changes.html">What changed in each update</a></p>#' "$P/index.html"

cd "$P"
git checkout -q --orphan publish
git add -A
git -c user.name="github-actions[bot]" -c user.email="41898282+github-actions[bot]@users.noreply.github.com" \
	commit -q -m "Packages from $GITHUB_REPOSITORY@${GITHUB_SHA:-local}"
git push -q -f origin publish:gh-pages
