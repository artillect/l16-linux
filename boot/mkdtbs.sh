#!/bin/bash
# dtbs-l16-only.bin: our DTB + all 115 stock DTBs with qcom,msm-id changed so LK can't pick them.
set -e
W=/mnt/c/Users/oreo4/Desktop/light-l16-modding
T=/tmp/l16dtbs; rm -rf $T; mkdir $T
cp $W/mainline/apq8096-light-l16.dtb $T/out.bin
for i in $(seq 0 114); do
	f=$W/device-dumps/boot/dtb$(printf %02d $i).dtb
	cp $f $T/d.dtb
	fdtput -t x $T/d.dtb / qcom,msm-id 0xdead 0x0
	cat $T/d.dtb >> $T/out.bin
done
cp $T/out.bin $W/mainline/dtbs-l16-only.bin
fdtget -t x $T/d.dtb / qcom,msm-id
