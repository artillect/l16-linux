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
