#!/bin/sh
# Light L16: UFS clock gating stalls USB DMA on this board (shared bus clock), which
# freezes USB networking; keep the UFS host clocked.
U=/sys/bus/platform/devices/624000.ufshc
if [ -e "$U/clkgate_enable" ]; then
	echo 0 > "$U/clkgate_enable"
	echo on > "$U/power/control"
fi
