# Linux / postmarketOS on the Light L16

Mainline Linux and postmarketOS (Phosh) on the Light L16 camera (APQ8096, FIH LK
bootloader), dual-booted with the stock LightOS (Android 6): Linux lives in the
`recovery` partition and a 64 GiB `linux` partition carved from `userdata`, Android
keeps `boot` and everything else.

## Layout

| Path | What |
|---|---|
| `kernel/` | Board DTS, kernel config fragment, `patch_ml.py` (+ helpers) applying our changes to the kernel tree, `overlay/` with the new drivers |
| `boot/` | Kernel/module build scripts and the boot image packer (`repack.py`, `mkdtbs.sh`) |
| `initramfs/` | The current custom initramfs (USB networking, UFS bring-up, switch_root) |
| `rootfs/` | postmarketOS configuration: `setup-rootfs.sh` and the files it installs |
| `tools/` | On-device test and debug helpers |
| `docs/` | Wiki draft, stock captures (audio mixer and codec register dumps) |

Stock-derived files (firmware, stock DTBs, boot signature, base boot image) are not in
git; see `.gitignore`.

## Kernel base

- Tree: https://gitlab.com/msm8996-mainline/linux
- Branch `msm8996-stable-6.19.y`, commit `1aed438cb5f4` (tag `v6.19.5-msm8996`)
