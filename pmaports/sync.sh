#!/bin/bash
# Copy the L16 packages into pmbootstrap's pmaports checkout. The kernel patches and
# config are kept once, in kernel/, and copied into the linux package here.
# usage: pmaports/sync.sh   (then: pmbootstrap checksum <pkg> / pmbootstrap build <pkg>)
set -e
REPO=$(cd "$(dirname "$0")/.." && pwd)
PMAPORTS=$(pmbootstrap config aports 2>/dev/null | tail -1)
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
for pkg in chiaro l16-camera glycin-lri; do
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
	find "$S" -type f -exec sed -i 's/\r$//' {} +
	find "$S" -type d -exec chmod 755 {} +
	find "$S" -type f -exec chmod 644 {} +
	chmod 755 "$S/$pkg/tools" "$S/$pkg"/tools/* 2>/dev/null || true
	tar --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner -C "$S" -cf - "$pkg" |
		gzip -n > "$PMAPORTS/main/$pkg/$pkg-src.tar.gz"
	rm -rf "$S"
}
pack l16-camera l16-camera tools/l16-shoot tools/l16-lri-assemble
pack glycin-lri glycin-lri

# our patched copies of pmaports' own packages go back to temp/
for pkg in libcamera; do
	rm -rf "${PMAPORTS:?}/temp/$pkg"
	mkdir -p "$PMAPORTS/temp/$pkg"
	cp -r "$REPO/pmaports/temp/$pkg/." "$PMAPORTS/temp/$pkg/"
	find "$PMAPORTS/temp/$pkg" -type f -exec sed -i 's/\r$//' {} +
done

# strip CR in case a file was edited on Windows
find "$DST/device-light-lfc" "$DST/linux-light-lfc" -type f -exec sed -i 's/\r$//' {} +
echo "synced to $DST"
