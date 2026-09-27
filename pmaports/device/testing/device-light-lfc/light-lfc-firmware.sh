#!/bin/sh
# Light L16: firmware msm-firmware-loader does not find on its own. It only scans
# image/ and firmware/ on the dedicated partitions; the L16 keeps the GPU zap shader
# and the Bluetooth patches in the (plain ext4) system partition's /etc/firmware, and
# ath10k wants the Wi-Fi board data under its own name. Link them into the loader's
# firmware directory, which the kernel already searches.
BASEDIR=/run/msm-firmware-loader
TARGET=$BASEDIR/target
[ -d "$TARGET" ] || exit 0

SYSTEM=$BASEDIR/mnt/system
if ! [ -d "$SYSTEM/etc/firmware" ]; then
	for part in /sys/block/sd*/sd*; do
		[ "$(grep PARTNAME= "$part/uevent" 2>/dev/null | cut -d= -f2)" = system ] || continue
		mkdir -p "$SYSTEM"
		mount -o ro,nodev,noexec,nosuid "/dev/$(basename "$part")" "$SYSTEM"
		break
	done
fi

link() {
	[ -e "$1" ] || return 0
	mkdir -p "$(dirname "$2")"
	[ -e "$2" ] || ln -s "$1" "$2"
}

# Adreno 530 zap shader, signed for this device
for f in "$SYSTEM"/etc/firmware/a530_zap.*; do
	link "$f" "$TARGET/$(basename "$f")"
done

# QCA6174 Bluetooth (ROME 3.2) under the names hci_qca asks for
link "$SYSTEM/etc/firmware/rampatch_tlv_3.2.tlv" "$TARGET/qca/rampatch_00440302.bin"
link "$SYSTEM/etc/firmware/nvm_tlv_3.2.bin" "$TARGET/qca/nvm_00440302.bin"

# QCA6174 Wi-Fi board data: stock loads bdwlan30.bin (board_id 0 in the OTP); ath10k
# falls back to board.bin since the card has no subsystem IDs
link "$BASEDIR/mnt/modem/image/bdwlan30.bin" "$TARGET/ath10k/QCA6174/hw3.0/board.bin"
