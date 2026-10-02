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
| The five proximity sensors around the lenses (lens-blocked warning) | GPS |
| Touch strip (volume outside the camera app), haptics | USB OTG, DisplayPort (ANX7688) |
| Speaker, front and rear microphones | Proximity sensor beside the screen |
| Battery and charging, charging light | Deep sleep: standby drains about 4-5% an hour |
| Accelerometer, gyroscope, magnetometer, light sensor (sensor DSP) | |
| Wi-Fi, Bluetooth, USB networking | |
| Suspend, screen rotation (including the lock screen) | |
| Rebooting to Android from a quick setting; forced restarts stay in Linux | |

## Camera

The `light-ccb` kernel driver talks to Light's camera ASICs the way the stock camera
does. The preview comes from one module at a time: 28 mm, then 70 mm, with the zoom
cropped in between up to 150 mm. It works in any libcamera app (Snapshot, Megapixels).
Photos capture every module for the zoom and are saved as LRI files, like stock.

Two apps, from the package repository (`apk add l16-camera l16-gallery`):

**Viewfinder** ([l16-camera](l16-camera)), a camera app laid out after OpenLight:
- auto, ISO priority, shutter priority and manual modes, with EV, all on stock's mode
  wheel;
- flash; whole-frame, centre or touch metering; tap focus, and AF-D (stock's refocus once
  the camera has moved and settled, or zoomed) with stock's focus marks;
- timer, burst, grid, histogram, and zoom on the touch strip;
- white balance presets taken from each camera's own factory calibration;
- stock's assists: tripod mode and stacked shots (the moon) from the gyro, a hand-shake
  warning, the lens-blocked warning, overheating, battery and storage status.

The preview stops while it can't be seen (screen off, another app in front).

**Lightbox** ([l16-gallery](l16-gallery)) shows the photos by day. It opens a quick look
straight from the LRI, and renders the full photo with Light's own renderer on request
([l16-render](l16-render): Light's library, taken from the stock partitions). The JPEG
goes next to the LRI. [glycin-lri](glycin-lri) also gives LRIs thumbnails in the file
manager and opens them in Loupe. Photos can also be rendered on a PC with Light's Lumen,
or with [chiaro](pmaports/main/chiaro) (packaged here).

The package repository carries a patched libcamera. Its software ISP takes manual white
balance and gives the preview stock's tone (digital gain and stock's gamma), and
`libcamerasrc` no longer drops controls set while streaming, passes frames to the
display without copying them, and survives the preview being stopped and started.

## Community

- XDA: [Light L16 Firmware](https://xdaforums.com/t/light-l16-firmware.4403267/)
- Discord: https://discord.gg/e3c2wEVDU4

## License

Kernel changes are GPL-2.0-only, docs CC BY-SA 4.0, everything else MIT; see
[LICENSE](LICENSE).
