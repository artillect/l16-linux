# Lens-blocked warning: the proximity sensors

Stock warns when a finger covers a lens. Research from 2026-10-01 (stock app smali, stock
DT, the sensor HAL), not yet tried on Linux.

## Hardware (stock DT: device-dumps/boot/lfc-p2-pvt.dts)

Five TXC **PA224** IR proximity sensors, all at I2C address **0x1e**, on the SoC's own I2C
(not the camera ASICs'), supply `pm8994_l29` (3.0 V), DT `ps_threshold_low/high` 25/40.

| Stock channel | Bus | Mux channel | IRQ |
|---|---|---|---|
| ch0 | BLSP1 QUP6 `i2c@757a000`, behind a TI **TCA9545A** mux @0x70 | 1 | mux irq GPIO 50 |
| ch1 | same mux | 3 | " |
| ch2 | BLSP1 QUP5 `i2c@7579000` (the TMP112 bus), direct | - | GPIO 34 |
| ch3 | mux | 0 | " |
| ch4 | mux | 2 | " |

Mux reset is GPIO 131. Our mainline DT (`apq8096-light-l16.dts`) already notes ch2 as left
out.

Stock kernel driver: `CONFIG_INPUT_PA224` (`pa224_*` in kallsyms of
device-dumps/boot/vmlinux.elf), sysfs `/sys/devices/virtual/input/txc_ps_chN/pa224_sysfs/ps`.

## Stock's logic

- **HAL** (`/system/vendor/lib64/sensors.ssc.so`, class `CamProximity`): round-robin, one
  emitter on at a time (IR crosstalk): enable chN, wait 150 ms, read `ps` (atoi), disable,
  next. A full cycle is ~750 ms. Reported as Android sensor "Light Proximity sensors"
  (`android.sensor.cam.proximity`), values[0] raw count, values[1] channel+1.
- **App** (`utils/LensObstructionDetector`): raw **>= 100** = blocked. Every 5 events
  (one cycle) it compares the blocked set with the last; on a change it calls
  onObstruction(list) / onNoObstruction.
- **UI**: haptic `Immersion.LENS_BLOCKED`; `proximity_sensor_notification_layout`, an L16
  back outline (`proximity_sensor_l16.png`) with a red block per blocked sensor
  (1 left-top, 2 left-mid, 3 left-bottom, 4 top-centre, 5 bottom-centre), text "lens
  blocked".
- **In pocket**: 2+ sensors blocked and the ambient light (SLPI's ALS, TYPE_LIGHT) under 2 lux,
  both for 30 s: "Entering pocket power save due to inactivity.", the camera closes.
- Settings `lens_blocked_detector_setting`, `inpocket_detection_setting`: both on by default.
- The LRI has a `ProximitySensors` block (5 bools, LightHeader field 20), but stock never
  fills it.

## To do on Linux

1. DT: the TCA9545A (`nxp,pca9545`, mainline `i2c-mux-pca954x`) on QUP6 with its reset GPIO,
   four PA224s behind it, the fifth on QUP5; their supply.
2. A PA224 driver: none in mainline. Either a small IIO proximity driver, or userspace
   polling through i2c-dev. Its registers: disassemble `pa224_init_client` /
   `pa224_enable_ps` / the read in vmlinux.elf, or i2cdump the parts on stock.
3. The camera app: poll one channel at a time as stock does, >= 100 blocked, the warning
   (stock's layout and haptic), optionally the pocket check.

## Unknowns, and the cheapest answers

- Register map and init values: the vmlinux.elf disassembly (above).
- Which chN is which lens, and typical open/covered counts (the >= 100 threshold): one session
  on stock: `adb shell su -c 'cat /sys/devices/virtual/input/txc_ps_ch*/pa224_sysfs/ps'`
  while covering one lens at a time (after enabling each channel as the HAL does).

## PA224 registers (from stock's vmlinux.elf, 2026-10-01)

SMBus byte access at 0x1e, 8-bit registers (meanings inferred from TXC's PA22x naming; the
values are exact from the code):

| Reg | Meaning | Stock |
|---|---|---|
| 0x00 | CFG0, bit 1 = PS on | 0x02 on, 0x00 off |
| 0x01 | LED current / persistence | 0x48 |
| 0x02 | interrupt set / flags | 0x00 (0x08 while calibrating) |
| 0x03 | PS period | 0x08 |
| 0x08 / 0x0A | PS low / high threshold | 25 / 40 (0 / 0xFF while calibrating) |
| 0x0E | **PS data, one byte 0-255** (offset already taken off) | read |
| 0x10 | PS offset (crosstalk) | calibrated, below |
| 0x11 / 0x12 | ? | 0x82 / 0x0C |
| 0x7F | chip ID | must read 0x11 |

- Power-up: L29 on, 130 ms. Init: 0x01=0x48, 0x03=0x08, 0x11=0x82, 0x12=0x0C, 0x10=0, 0x02=0.
- Crosstalk (stock does it at every boot, one sensor at a time): 0x0A=0xFF, 0x08=0, 0x02=0x08,
  0x10=0, PS on; 4 reads of 0x0E 50 ms apart; xt = mean of the middle two + 4; if xt > 99
  use 0, else 0x10=xt; 0x0A=40, 0x08=25, PS off. (No saved factory values: the bspdata slot
  at 0x7C000 holds none.)
- Reading: PS on, 150 ms, read 0x0E, PS off; >= 100 blocked.
- No reset line, no interrupt used (purely polled).
- Mux (TCA9545A @0x70): write 1<<chan to select, 0 to deselect; reset GPIO 131 active low.
  Mux channels 0-3 = stock ch3, ch0, ch4, ch1; ch2 on blsp1 I2C5 directly.
