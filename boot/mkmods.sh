#!/bin/bash
# Build and package the kernel modules for the current build (after build.sh).
# usage: mkmods.sh <tag>   ->  mainline/modules-<tag>.tar.gz (install on pmOS under /)
set -e
W=/mnt/c/Users/oreo4/Desktop/light-l16-modding
O=~/l16/out-mainline
cd ~/l16/mainline
export ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu-

make O=$O -j32 modules > /tmp/modules.log 2>&1 || { grep -E "error" /tmp/modules.log | head -20; exit 1; }
rm -rf /tmp/l16mods
make O=$O INSTALL_MOD_PATH=/tmp/l16mods INSTALL_MOD_STRIP=1 modules_install > /tmp/mi.log 2>&1
cd /tmp/l16mods
tar -czf $W/mainline/modules-$1.tar.gz lib
find lib -name '*.ko*' | wc -l
