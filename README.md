# postmarketOS on the Light L16

Mainline Linux (msm8996-mainline 6.19) and postmarketOS v26.06 with Phosh on the
Light L16 camera (codename `lfc`, APQ8096), dual-booted with the stock LightOS
(Android 6).

> [!WARNING]
> **This is an in-development port. Use it at your own risk.** Installing it rewrites
> partitions on your camera, and a mistake can leave it unable to boot. It also runs
> new, lightly tested drivers for things like charging and power, which could in
> principle damage the hardware. Back up the stock partitions before you start (see
> [docs/wiki-light-l16.txt](docs/wiki-light-l16.txt)). There is no warranty of any
> kind.
>
> Most of this port was written by an AI (Claude, by Anthropic): the kernel drivers,
> device tree, packaging and docs. A human tested it on real hardware along the way.
> Expect rough edges. postmarketOS doesn't accept AI-generated contributions, so this
> lives here rather than upstream.

## Status

| Works | Not yet |
|---|---|
| Display, touchscreen, GPU (Adreno 530) | The cameras |
| Touch strip, haptics | 3.5 mm microphone jack |
| Speaker, front and rear microphones | GPS |
| Battery and charging | USB OTG, DisplayPort (ANX7688) |
| Accelerometer, gyroscope, magnetometer, light sensor (sensor DSP) | Proximity sensor |
| Wi-Fi, Bluetooth, USB networking | |
| Suspend, screen rotation (including the lock screen) | |

## How it boots

The unlocked bootloader (FIH's LK) boots `boot` normally and `recovery` when asked for
the recovery reboot reason. Android keeps `boot`; Linux lives in `recovery`, with its
root filesystem on a 64 GiB `linux` partition taken from `userdata`.

- A restart from Linux comes back to Linux. **Reboot to Android** in the app grid
  starts the stock LightOS.
- From Android, `adb reboot recovery` starts Linux.
- LK only boots images followed by an Android Verified Boot 1.0 signature (any key).
  The device package signs every boot image it makes, and kernel updates write
  `recovery` themselves.

No firmware is included here. It is loaded at boot from the stock `modem`, `persist`,
`dsp`, `bluetooth` and `system` partitions, so keep those intact.

## Installing

You need:

- An unlocked bootloader: `fastboot oem devlock off` (no code needed; data is kept).
  See [docs/wiki-light-l16.txt](docs/wiki-light-l16.txt), which also covers backing up
  the stock partitions first.
- A `linux` partition. **Creating it is not documented yet.**
- [pmbootstrap](https://wiki.postmarketos.org/wiki/Pmbootstrap) on a Linux PC (or WSL,
  see below).

Build and install:

```sh
git clone https://github.com/artillect/l16-linux
pmbootstrap init            # channel v26.06; any device for now
l16-linux/pmaports/sync.sh  # copies the L16 packages into pmbootstrap's pmaports
pmbootstrap init            # device light-lfc, UI phosh, init system OpenRC
pmbootstrap install --single-partition
```

Only OpenRC is supported so far, so answer `openrc` when `pmbootstrap init` asks.
`--single-partition` makes the image a bare filesystem, so `linux` itself becomes the
root partition.

Flash, with the camera in fastboot (`adb reboot bootloader` from Android):

```sh
pmbootstrap flasher flash_rootfs    # to "linux"
pmbootstrap flasher flash_kernel    # signed boot image to "recovery"
fastboot reboot
```

Then start Linux from Android with `adb reboot recovery`. The first boot grows the root
filesystem to the whole partition, and the clock is set once Wi-Fi is connected.

Phosh brings phone apps the camera has no use for; to remove them:

```sh
sudo apk del calls chatty
```

### From Windows (WSL)

pmbootstrap works in WSL 2. To reach the camera over USB, share it with
[usbipd-win](https://github.com/dorssel/usbipd-win) while it is in fastboot:

```sh
usbipd list                          # the camera in fastboot shows up as 18d1:d00d
usbipd bind --busid <busid>          # once, as administrator
usbipd attach --wsl --busid <busid>  # each time
```

If `attach` fails to load `vhci_hcd`, run `sudo modprobe vhci_hcd` in WSL first.

## Layout

| Path | What |
|---|---|
| `pmaports/` | `device-light-lfc` and `linux-light-lfc` packages; `sync.sh` copies them into pmbootstrap's pmaports |
| `kernel/patches/` | The L16 changes against msm8996-mainline `v6.19.5-msm8996`, one topic per patch (device tree, panel, touch strip, haptics, audio, charger, ...) |
| `kernel/config-light-lfc.aarch64` | Kernel config (arm64 defconfig + `l16.config`) |
| `kernel/mkpatches.sh` | Regenerates `patches/` from a kernel tree |
| `tools/` | On-device test and debug helpers |
| `docs/` | Wiki draft, stock audio captures (mixer and codec register dumps) |

## Kernel

- Tree: https://gitlab.com/msm8996-mainline/linux, tag `v6.19.5-msm8996`
  (commit `1aed438cb5f4`)
- To work on it: check out that tag, `git apply kernel/patches/*.patch`, change things,
  then `kernel/mkpatches.sh <tree>` and rebuild `linux-light-lfc`.

## Community

- XDA: [Light L16 Firmware](https://xdaforums.com/t/light-l16-firmware.4403267/)
- Discord: https://discord.gg/e3c2wEVDU4

## License

Kernel changes are GPL-2.0-only, docs CC BY-SA 4.0, everything else MIT; see
[LICENSE](LICENSE).
