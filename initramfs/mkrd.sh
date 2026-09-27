#!/bin/bash
# Build the USB-shell initramfs: Alpine minirootfs + extra Alpine packages (apk/*.apk:
# busybox-extras for udhcpd, dropbear for ssh) + our /init, owned by root.
set -e
W=/mnt/c/Users/oreo4/Desktop/light-l16-modding/mainline
R=/tmp/l16rd
rm -rf $R; mkdir $R
tar -xzf $W/alpine-minirootfs-3.24.2-aarch64.tar.gz -C $R 2>/dev/null || true
for a in $W/apk/*.apk; do
	tar -xzf $a -C $R --exclude='.PKGINFO' --exclude='.SIGN.*' --exclude='.*-install' \
		--exclude='.trigger' 2>/dev/null || true
done
# UFS drivers as modules (load by hand: insmod qcom_ice, phy-qcom-qmp-ufs, ufs-qcom)
O=~/l16/out-mainline
mkdir -p $R/lib/modules/l16
cp $O/drivers/soc/qcom/qcom_ice.ko $O/drivers/phy/qualcomm/phy-qcom-qmp-ufs.ko \
	$O/drivers/ufs/host/ufs-qcom.ko $R/lib/modules/l16/
aarch64-linux-gnu-gcc -static -Os -o $R/sbin/holdreboot $W/initramfs/holdreboot.c
aarch64-linux-gnu-strip $R/sbin/holdreboot
sed 's/\r$//' $W/initramfs/init > $R/init
chmod 755 $R/init
# ssh login with public keys only: the user's key and a project key for scripted access
mkdir -p $R/root/.ssh $R/etc/dropbear
cat /mnt/c/Users/oreo4/.ssh/id_ed25519.pub $W/l16_ssh_key.pub | sed 's/\r$//' > $R/root/.ssh/authorized_keys
chmod 700 $R/root/.ssh; chmod 600 $R/root/.ssh/authorized_keys
cd $R
find . | cpio -o -H newc -R 0:0 2>/dev/null | gzip -9 > $W/usbshell-rd.cpio.gz
ls -la $W/usbshell-rd.cpio.gz; ls $R/usr/sbin/dropbear $R/bin/busybox-extras
