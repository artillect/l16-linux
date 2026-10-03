#!/bin/sh
# l16-render IN.lri OUT.jpg|OUT.dng [LONG-SIDE]: render an LRI with Light's own renderer (the
# stock gallery's libcp), from the camera's stock partitions (light-lfc-android-libs gathers
# it at boot). LONG-SIDE: the output's longer side in pixels (default: full size).
L=/var/lib/l16/android
if [ ! -x $L/linker64 ] || [ ! -e $L/libcp.so ] || [ ! -e $L/libc++_shared.so ]; then
	echo "l16-render: Light's renderer isn't set up from the stock partitions" \
		"(see: rc-service light-lfc-android-libs start)" >&2
	exit 69
fi
# a photo taken in portrait: the renderer turns it, and is told its size the other way round
case "$(/usr/libexec/l16-render-orient "$1" 2>/dev/null)" in
1 | 2) export L16_TURNED=1 ;;
esac
export LD_LIBRARY_PATH=$L ANDROID_ROOT=$L ANDROID_DATA=$L
exec /usr/libexec/l16-render "$@"
