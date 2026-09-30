#!/bin/sh
# Build l16-render with the Android NDK: an Android program run through the stock linker64
# and libraries copied from the camera's system partition to /var/lib/l16/android. It links
# against a stub of Light's libcp.so (the real one's symbol table trips the NDK's linker)
# and the stock gallery's libc++_shared.so, which libcp needs and which must be that exact
# old build: pass its folder as $1 (it is Light's, so it isn't in this repository).
set -e
export MSYS_NO_PATHCONV=1	# Git Bash: keep the Linux paths below as they are
NDK=${NDK:-$ANDROID_NDK_HOME}
LIBS=${1:?usage: build.sh DIR-WITH-GALLERY-libc++_shared.so}
CXX="$NDK/toolchains/llvm/prebuilt/$(ls "$NDK/toolchains/llvm/prebuilt" | head -1)/bin/aarch64-linux-android23-clang++"
[ -e "$CXX" ] || CXX="$CXX.cmd"
cd "$(dirname "$0")"
mkdir -p build
"$CXX" -std=c++17 -O2 -shared -fPIC -nostdlib++ -w libcp-stub.cpp "$LIBS/libc++_shared.so" \
	-Wl,-soname,libcp.so -o build/libcp.so
"$CXX" -std=c++17 -O2 -Wall -nostdlib++ render.cpp build/libcp.so "$LIBS/libc++_shared.so" \
	-Wl,--dynamic-linker=/var/lib/l16/android/linker64 -Wl,-rpath,/var/lib/l16/android \
	-o build/l16-render
echo "built: build/l16-render"
