#!/bin/sh
# boot-deploy runs deviceinfo_mkinitfs_postprocess with the initramfs in its work
# directory, right after it has made boot.img there and before it copies it to /boot
# and flashes it: sign boot.img for the L16's bootloader.
img="$(dirname "$1")/boot.img"
[ -f "$img" ] || exit 0
exec /usr/libexec/light-lfc-bootsig "$img"
