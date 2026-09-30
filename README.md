# postmarketOS on the Light L16

Mainline Linux (msm8996-mainline 6.19) and postmarketOS v26.06 with Phosh on the
Light L16 camera (codename `lfc`, APQ8096), dual-booted with the stock LightOS
(Android 6).

> [!WARNING]
> **This is an in-development port. Use it at your own risk.** Installing it rewrites
> partitions on your camera, and a mistake can leave it unable to boot. It also runs
> new, lightly tested drivers for things like charging and power, which could in
> principle damage the hardware. [Back up the stock partitions](https://github.com/artillect/l16-linux/wiki/Unlocking-and-backups)
> before you start. There is no warranty of any kind.
>
> Most of this port was written by an AI (Claude Opus 5.5, by Anthropic): the kernel
> drivers, device tree, packaging and docs. A human tested it on real hardware along
> the way. Expect rough edges. postmarketOS doesn't accept AI-generated contributions,
> so this lives here rather than upstream.

## Documentation

Everything is in the **[wiki](https://github.com/artillect/l16-linux/wiki)**:

- [Flash mode](https://github.com/artillect/l16-linux/wiki/Flash-mode), including the Windows driver
- [Unlocking and backups](https://github.com/artillect/l16-linux/wiki/Unlocking-and-backups)
- [Installation](https://github.com/artillect/l16-linux/wiki/Installation): prebuilt image or pmbootstrap
- [Using Linux](https://github.com/artillect/l16-linux/wiki/Using-Linux): switching to Android, updates
- [Development](https://github.com/artillect/l16-linux/wiki/Development)

Prebuilt images are under [Releases](https://github.com/artillect/l16-linux/releases), and
installs get updates from the [package repository](https://artillect.github.io/l16-linux/).

## Status

| Works | Not yet |
|---|---|
| Display, touchscreen, GPU (Adreno 530) | Video recording |
| Cameras: live preview and full 16-module photos (see below) | 3.5 mm microphone jack |
| Touch strip (volume outside the camera app), haptics | GPS |
| Speaker, front and rear microphones | USB OTG, DisplayPort (ANX7688) |
| Battery and charging, charging light | Proximity sensor |
| Accelerometer, gyroscope, magnetometer, light sensor (sensor DSP) | |
| Wi-Fi, Bluetooth, USB networking | |
| Suspend, screen rotation (including the lock screen) | |

## Camera

The `light-ccb` kernel driver talks to Light's camera ASICs the way the stock camera
does. The preview comes from one module at a time: 28 mm, then 70 mm, with the zoom
cropped in between up to 150 mm. It works in any libcamera app (Snapshot, Megapixels).
Photos capture every module for the zoom and are saved as LRI files, like stock. They
can be rendered with [chiaro](pmaports/main/chiaro) (packaged here) or Light's Lumen.

[l16-camera](l16-camera) is a camera app laid out after OpenLight:
- auto, ISO priority, shutter priority and manual modes, with EV, all on stock's mode
  wheel;
- flash, touch or centre metering, tap focus and continuous focus;
- timer, burst, and zoom on the touch strip;
- white balance presets taken from each camera's own factory calibration.

It isn't packaged yet: build it from its folder with `cargo build --release`, and put
`l16-shoot` and `l16-lri-assemble` from [tools](tools) on the PATH (it runs them).

The package repository carries a patched libcamera. Its software ISP takes manual white
balance, and `libcamerasrc` no longer drops controls set while streaming.

## Community

- XDA: [Light L16 Firmware](https://xdaforums.com/t/light-l16-firmware.4403267/)
- Discord: https://discord.gg/e3c2wEVDU4

## License

Kernel changes are GPL-2.0-only, docs CC BY-SA 4.0, everything else MIT; see
[LICENSE](LICENSE).
