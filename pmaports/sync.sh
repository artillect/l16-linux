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

# strip CR in case a file was edited on Windows
find "$DST/device-light-lfc" "$DST/linux-light-lfc" -type f -exec sed -i 's/\r$//' {} +
echo "synced to $DST"
