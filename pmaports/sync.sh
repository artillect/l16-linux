#!/bin/bash
# Copy the L16 packages into pmbootstrap's pmaports checkout. The kernel patches and
# config are kept once, in kernel/, and copied into the linux package here.
# usage: pmaports/sync.sh   (then: pmbootstrap checksum <pkg> / pmbootstrap build <pkg>)
# PMB_CONFIG=FILE: the pmbootstrap config (-c) whose pmaports to sync into
set -e
REPO=$(cd "$(dirname "$0")/.." && pwd)
PMAPORTS=$(pmbootstrap ${PMB_CONFIG:+-c "$PMB_CONFIG"} config aports 2>/dev/null | tail -1)
PMAPORTS=${PMAPORTS:-$HOME/.local/var/pmbootstrap/cache_git/pmaports}
DST=$PMAPORTS/device/testing

for pkg in device-light-lfc linux-light-lfc; do
	rm -rf "${DST:?}/$pkg"
	mkdir -p "$DST/$pkg"
	# keep the checksums pmbootstrap wrote last time
	cp -r "$REPO/pmaports/device/testing/$pkg/." "$DST/$pkg/"
done
cp "$REPO"/kernel/patches/*.patch "$REPO/kernel/config-light-lfc.aarch64" \
	"$DST/linux-light-lfc/"

# other packages (not device-specific) go to main/
for pkg in chiaro l16-camera glycin-lri l16-gallery l16-render l16-phosh-plugins l16-gnss; do
	rm -rf "${PMAPORTS:?}/main/$pkg"
	mkdir -p "$PMAPORTS/main/$pkg"
	cp -r "$REPO/pmaports/main/$pkg/." "$PMAPORTS/main/$pkg/"
	find "$PMAPORTS/main/$pkg" -type f -exec sed -i 's/\r$//' {} +
done

# our own programs build from this repository: pack(PKG DIR [FILE...]) packs DIR (and the
# files, into tools/) as main/PKG/PKG-src.tar.gz, reproducibly (fixed times, owners and
# modes; LF line ends), so the APKBUILD's checksum holds until they change (then:
# pmbootstrap checksum PKG, and copy it back)
pack() {
	local pkg=$1 dir=$2 S
	shift 2
	S=$(mktemp -d)
	mkdir -p "$S/$pkg/tools"
	cp -r "$REPO/$dir/." "$S/$pkg/"
	rm -rf "$S/$pkg/target"
	for f in "$@"; do cp "$REPO/$f" "$S/$pkg/tools/"; done
	# CRs off text files only: on a binary (places.tsv.gz) it corrupts the file
	find "$S" -type f -exec grep -Iq . {} \; -exec sed -i 's/\r$//' {} \;
	find "$S" -type d -exec chmod 755 {} +
	find "$S" -type f -exec chmod 644 {} +
	chmod 755 "$S/$pkg/tools" "$S/$pkg"/tools/* 2>/dev/null || true
	tar --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner -C "$S" -cf - "$pkg" |
		gzip -n > "$PMAPORTS/main/$pkg/$pkg-src.tar.gz"
	rm -rf "$S"
}
# pack_tree(PKG PATH...): the repository's paths as they are, for a program that uses files
# of its neighbours (PKG's own folder is PATH one)
pack_tree() {
	local pkg=$1 S
	shift
	S=$(mktemp -d)
	for f in "$@"; do
		mkdir -p "$S/$(dirname "$f")"
		cp -r "$REPO/$f" "$S/$f"
	done
	rm -rf "$S/$pkg/target"
	# Python's caches: git-ignored, so not in CI's checkout (a checksum made here failed there)
	find "$S" -name __pycache__ -type d -prune -exec rm -rf {} +
	# (built programs, in build/, as they are)
	# (CRs off text files only: on a binary, places.tsv.gz, it corrupted the file)
	find "$S" -type f ! -path "*/build/*" -exec grep -Iq . {} \; -exec sed -i 's/\r$//' {} \;
	find "$S" -type d -exec chmod 755 {} +
	find "$S" -type f -exec chmod 644 {} +
	tar --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner -C "$S" -cf - . |
		gzip -n > "$PMAPORTS/main/$pkg/$pkg-src.tar.gz"
	rm -rf "$S"
}
pack l16-camera l16-camera tools/l16-shoot tools/l16-lri-assemble
pack glycin-lri glycin-lri
pack_tree l16-gallery l16-gallery l16-camera/src/icons.rs glycin-lri/src/lri.rs
pack_tree l16-phosh-plugins l16-phosh
pack_tree l16-gnss l16-gnss
# l16-render: with its NDK build (l16-render/build.sh), which isn't in the repository
if [ -e "$REPO/l16-render/build/l16-render" ]; then
	pack_tree l16-render l16-render/build.sh l16-render/l16-render.sh l16-render/l16-render-orient \
		l16-render/render.cpp \
		l16-render/libcp-stub.cpp l16-render/build/l16-render
else
	echo "no l16-render/build/l16-render (l16-render/build.sh): l16-render can't be packaged" >&2
fi

# our patched copies of pmaports' own packages go back to temp/
for pkg in libcamera iio-sensor-proxy; do
	rm -rf "${PMAPORTS:?}/temp/$pkg"
	mkdir -p "$PMAPORTS/temp/$pkg"
	cp -r "$REPO/pmaports/temp/$pkg/." "$PMAPORTS/temp/$pkg/"
	find "$PMAPORTS/temp/$pkg" -type f -exec sed -i 's/\r$//' {} +
done
# (our OpenRC phosh, until 0.2.0)
rm -rf "${PMAPORTS:?}/temp/phosh"
# and of postmarketOS's systemd builds to extra-repos/systemd/
for pkg in phosh; do
	rm -rf "${PMAPORTS:?}/extra-repos/systemd/$pkg"
	mkdir -p "$PMAPORTS/extra-repos/systemd/$pkg"
	cp -r "$REPO/pmaports/extra-repos/systemd/$pkg/." "$PMAPORTS/extra-repos/systemd/$pkg/"
	find "$PMAPORTS/extra-repos/systemd/$pkg" -type f -exec sed -i 's/\r$//' {} +
done

# strip CR in case a file was edited on Windows
find "$DST/device-light-lfc" "$DST/linux-light-lfc" -type f -exec sed -i 's/\r$//' {} +
echo "synced to $DST"
