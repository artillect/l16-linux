#!/bin/bash
# Build the mainline (msm8996-mainline 6.19) kernel + L16 DTB.
# Outputs to mainline/: Image.gz, apq8096-light-l16.dtb, dtbs-l16-first.bin (ours + all stock DTBs)
set -e
W=/mnt/c/Users/oreo4/Desktop/light-l16-modding
cd ~/l16/mainline
export ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu-
O=~/l16/out-mainline

cp -r $W/mainline/overlay/. .
python3 $W/mainline/patch_ml.py .
cp $W/mainline/apq8096-light-l16.dts arch/arm64/boot/dts/qcom/
grep -q apq8096-light-l16 arch/arm64/boot/dts/qcom/Makefile ||
	echo 'dtb-$(CONFIG_ARCH_QCOM)	+= apq8096-light-l16.dtb' >> arch/arm64/boot/dts/qcom/Makefile

if [ ! -f $O/.config ]; then
	make O=$O defconfig
	scripts/kconfig/merge_config.sh -m -O $O $O/.config $W/mainline/l16.config
	make O=$O olddefconfig
fi

make O=$O -j32 Image.gz qcom/apq8096-light-l16.dtb > /tmp/ml.log 2>&1 || { grep -E "error" /tmp/ml.log | head -20; exit 1; }
cp $O/arch/arm64/boot/Image.gz $W/mainline/
cp $O/arch/arm64/boot/dts/qcom/apq8096-light-l16.dtb $W/mainline/
cat $W/mainline/apq8096-light-l16.dtb $W/kernel-build/stock-dtbs.bin > $W/mainline/dtbs-l16-first.bin
strings $O/arch/arm64/boot/Image | grep -m1 "Linux version"
