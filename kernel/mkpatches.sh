#!/bin/bash
# Export the L16 changes in a kernel tree as a numbered patch series against the
# msm8996-mainline base tag, one topic per patch. To get such a tree: check out
# v6.19.5-msm8996 and `git apply kernel/patches/*.patch`.
# usage: mkpatches.sh [kernel tree]   ->  kernel/patches/*.patch
set -e
T=${1:-$HOME/l16/mainline}
OUT=$(cd "$(dirname "$0")" && pwd)/patches
BASE=v6.19.5-msm8996

cd "$T"
# new files must be known to git to show up in the diff
git add -N arch/arm64/boot/dts/qcom/apq8096-light-l16.dts \
	drivers/gpu/drm/panel/panel-innolux-nt35695-l16.c \
	drivers/input/misc/dw7800-haptics.c drivers/input/misc/l16-touchstrip.c \
	drivers/soc/qcom/l16_canary.c include/linux/l16_canary.h \
	drivers/usb/misc/anx7688 drivers/video/backlight/lm3630_bl.c \
	drivers/media/i2c/light-ccb.c

rm -rf "$OUT"
mkdir -p "$OUT"
n=0
patch() {
	local name=$1 subject=$2
	shift 2
	n=$((n + 1))
	local f=$OUT/$(printf %04d $n)-$name.patch
	{
		echo "From: Riley <oreo4455@gmail.com>"
		echo "Subject: [PATCH] $subject"
		echo
		echo "---"
		git diff "$BASE" -- "$@"
	} > "$f"
	echo "$(basename "$f"): $(grep -c '^+[^+]' "$f") added lines"
}

patch arm64-dts-qcom-add-light-l16 "arm64: dts: qcom: add Light L16" \
	arch/arm64/boot/dts/qcom/apq8096-light-l16.dts arch/arm64/boot/dts/qcom/Makefile
patch drm-panel-add-innolux-nt35695-l16 "drm/panel: add Innolux NT35695 panel of the Light L16" \
	drivers/gpu/drm/panel
patch backlight-add-lm3630-l16 "backlight: add LM3630 as programmed by the Light L16" \
	drivers/video/backlight
patch input-add-l16-touchstrip-and-dw7800 "Input: add Light L16 touch strip and DW7800 haptics" \
	drivers/input/misc/Kconfig drivers/input/misc/Makefile \
	drivers/input/misc/l16-touchstrip.c drivers/input/misc/dw7800-haptics.c
patch usb-misc-add-anx7688 "usb: misc: add ANX7688 Type-C controller (port of the L16 stock driver)" \
	drivers/usb/misc
patch soc-qcom-add-l16-canary "soc: qcom: add L16 bring-up canary (panic reset to stock, boot stage resets)" \
	drivers/soc/qcom/Kconfig drivers/soc/qcom/Makefile drivers/soc/qcom/l16_canary.c \
	include/linux/l16_canary.h arch/arm64/kernel/setup.c init/main.c drivers/base/dd.c
patch power-supply-smbchg-l16 "power: supply: qcom-smbchg: input current cap and JEITA settings" \
	drivers/power/supply/qcom-smbchg.c drivers/power/supply/qcom-smbchg.h
patch power-supply-qcom-fg-current-sign "power: supply: qcom_fg: report discharging current as negative" \
	drivers/power/supply/qcom_fg.c
patch usb-gadget-u-ether-keep-endpoints "usb: gadget: u_ether: keep endpoints running while the host is connected" \
	drivers/usb/gadget/function/u_ether.c
patch drm-msm-mdp5-first-pairable-mixer "drm/msm/mdp5: use the first pair-able layer mixer" \
	drivers/gpu/drm/msm/disp/mdp5/mdp5_mixer.c
patch drm-msm-mdp5-stale-hwpipe "drm/msm/mdp5: drop hwpipes not registered in the global state" \
	drivers/gpu/drm/msm/disp/mdp5/mdp5_plane.c
patch clk-qcom-gcc-msm8996-pcie0-retention "clk: qcom: gcc-msm8996: keep PCIe0 GDSC in retention" \
	drivers/clk/qcom/gcc-msm8996.c
patch input-pm8941-pwrkey-abort-suspend "Input: pm8941-pwrkey: a press aborts suspend" \
	drivers/input/misc/pm8941-pwrkey.c
patch iio-qcom-smgr-metadata-array "iio: qcom_smgr: decode the report metadata as a per-item array" \
	drivers/iio/common/qcom_smgr/qmi
patch iio-qcom-smgr-l16-light "iio: qcom_smgr: L16: stream light instead of the dead proximity sensor" \
	drivers/iio/common/qcom_smgr/qcom_smgr.c
patch asoc-wcd9335-dec-volume-unmute "ASoC: codecs: wcd9335: decimator volume controls, unmute after settling" \
	sound/soc/codecs/wcd9335.c
patch power-reset-reboot-mode-default "power: reset: reboot-mode: settable mode for a reboot without a command" \
	drivers/power/reset/reboot-mode.c
patch media-i2c-add-light-ccb "media: i2c: add the Light L16 camera ASICs as a CSI-2 source" \
	drivers/media/i2c/Kconfig drivers/media/i2c/Makefile drivers/media/i2c/light-ccb.c
patch slimbus-qcom-ngd-base-before-add "slimbus: qcom-ngd-ctrl: set the NGD base before adding its device" \
	drivers/slimbus/qcom-ngd-ctrl.c
patch tty-serial-msm-fourth-port "tty: serial: msm: a fourth port, for the third ASIC's debug UART" \
	drivers/tty/serial/msm_serial.c
patch media-qcom-camss-l16-capture "media: qcom: camss: L16 captures (virtual channels, taller raw frames, shared links)" \
	drivers/media/platform/qcom/camss/camss.h drivers/media/platform/qcom/camss/camss.c \
	drivers/media/platform/qcom/camss/camss-csid-4-7.c drivers/media/platform/qcom/camss/camss-ispif.c \
	drivers/media/platform/qcom/camss/camss-csid.c drivers/media/platform/qcom/camss/camss-csiphy.c \
	drivers/media/platform/qcom/camss/camss-csiphy-3ph-1-0.c drivers/media/platform/qcom/camss/camss-vfe-gen1.c \
	drivers/media/platform/qcom/camss/camss-vfe.c drivers/media/platform/qcom/camss/camss-video.c \
	drivers/media/platform/qcom/camss/camss-video.h

# everything changed must be in exactly one patch
all=$(git diff --name-only "$BASE" | sort)
covered=$(cat "$OUT"/*.patch | grep '^+++ b/' | sed 's|^+++ b/||' | sort -u)
missing=$(comm -23 <(echo "$all") <(echo "$covered"))
[ -z "$missing" ] || { echo "NOT IN ANY PATCH:"; echo "$missing"; exit 1; }
echo "all $(echo "$all" | wc -l) changed files covered"
