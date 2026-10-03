# postmarketOS on the Light L16

Mainline Linux (msm8996-mainline 6.19) and postmarketOS v26.06 with Phosh on the
Light L16 camera (codename `lfc`, APQ8096), dual-booted with the stock LightOS
(Android 6).

![The L16 running postmarketOS, on the grass](docs/images/l16-outdoors.jpg)

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
- [Dual boot](https://github.com/artillect/l16-linux/wiki/Dual-boot) (keep Android) or [Installation](https://github.com/artillect/l16-linux/wiki/Installation) (replace Android): prebuilt image or pmbootstrap
- [Using Linux](https://github.com/artillect/l16-linux/wiki/Using-Linux): switching to Android, updates
- [Development](https://github.com/artillect/l16-linux/wiki/Development)

Prebuilt images are under [Releases](https://github.com/artillect/l16-linux/releases), and
installs get updates from the [package repository](https://artillect.github.io/l16-linux/).

## Status

| Works | Not yet |
|---|---|
| Display, touchscreen, GPU (Adreno 530) | Video recording |
| Cameras: live preview and full 16-module photos (see below) | 3.5 mm microphone jack |
| The five proximity sensors around the lenses (lens-blocked warning) | |
| Touch strip (volume outside the camera app), haptics | USB OTG, DisplayPort (ANX7688) |
| Speaker, front and rear microphones | Proximity sensor beside the screen |
| Battery and charging, charging light | |
| Accelerometer, gyroscope, magnetometer, light sensor (sensor DSP) | |
| Wi-Fi, Bluetooth, USB networking | |
| GPS (the modem's receiver, with XTRA assistance, for apps through geoclue) | |
| Suspend with deep sleep, screen rotation (including the lock screen) | |
| Rebooting to Android from a quick setting; forced restarts stay in Linux | |

## Camera

The `light-ccb` kernel driver talks to Light's camera ASICs the way the stock camera
does. The preview comes from one module at a time: 28 mm, then 70 mm, with the zoom
cropped in between up to 150 mm. Photos capture every module for the zoom and are saved
as LRI files, like stock. PipeWire camera apps (Snapshot) don't see the cameras: Viewfinder
is the camera app.

Two apps, from the package repository (`apk add l16-camera l16-gallery`):

**Viewfinder** ([l16-camera](l16-camera)), a camera app laid out after OpenLight:
- auto, ISO priority, shutter priority and manual modes, with EV, all on stock's mode
  wheel;
- flash; whole-frame, centre or touch metering; tap focus, and AF-D (stock's refocus once
  the camera has moved and settled, or zoomed) with stock's focus marks;
- timer, burst, grid, histogram, and zoom on the touch strip;
- white balance presets taken from each camera's own factory calibration;
- stock's assists: tripod mode and stacked shots (the moon) from the gyro, a hand-shake
  warning, the lens-blocked warning, overheating, battery and storage status;
- geotagging (a setting): the camera's own GPS, while the preview runs;
- portrait, as stock: the controls and text turn with the camera, the display stays
  landscape, and portrait photos come out upright;
- stock's in-pocket check: lenses covered in the dark, a countdown, then the camera sleeps.

It keeps the screen on while it is in front, and its preview stops while it can't be seen
(screen off, another app in front).

| | |
|---|---|
| ![Viewfinder with the EV wheel open](docs/images/viewfinder-ev-wheel.jpg) | ![Viewfinder's settings](docs/images/viewfinder-settings.png) |

**Lightbox** ([l16-gallery](l16-gallery)) shows the photos by day. It opens a quick look
straight from the LRI, and renders the full photo with Light's own renderer on request
([l16-render](l16-render): Light's library, taken from the stock partitions). The JPEG
goes next to the LRI. A geotagged photo's info gives the nearest town, looked up on the
camera from a table of GeoNames' places. Its menu copies the LRI or the render, or shows
either in its folder, and several photos can be chosen and deleted at once.
[glycin-lri](glycin-lri) also gives LRIs thumbnails in the file manager and opens them in
Loupe. Photos can also be rendered on a PC with Light's Lumen,
or with [chiaro](pmaports/main/chiaro) (packaged here).

| | |
|---|---|
| ![Lightbox's photos by day](docs/images/lightbox-grid.jpg) | ![A photo in Lightbox](docs/images/lightbox-preview.jpg) |

The package repository carries a patched libcamera. Its software ISP takes manual white
balance and gives the preview stock's tone (digital gain and stock's gamma), and
`libcamerasrc` no longer drops controls set while streaming, passes frames to the
display without copying them, and survives the preview being stopped and started.

## Community

- XDA: [Light L16 Firmware](https://xdaforums.com/t/light-l16-firmware.4403267/)
- [Light L16 community Discord](https://discord.gg/e3c2wEVDU4): questions and help in the
  [postmarketOS thread](https://discord.com/channels/1152992591256760401/1556041895342375003)
  (join the server first)

## Thanks

Much of what's known about the L16, and so much of the wiki, was gathered by the community
in the XDA thread and the L16 community Discord. These projects helped a lot along the way:

- [openlight-camera](https://github.com/ookami125/openlight-camera) by ookami125: Viewfinder
  is laid out after it
- [lri-rs](https://github.com/gennyble/lri-rs) by gennyble: reading the LRI format
- [chiaro](https://github.com/shinf1x/chiaro) by shinf1x: an open replacement for Lumen,
  packaged here
- [Light-L16-Archive](https://github.com/helloavo/Light-L16-Archive) by helloavo: Light's
  firmware and software, kept available

## License

Kernel changes are GPL-2.0-only, docs CC BY-SA 4.0, everything else MIT; see
[LICENSE](LICENSE). Lightbox's place names come from [GeoNames](https://www.geonames.org/)
(CC BY 4.0).
