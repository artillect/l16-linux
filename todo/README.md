# To do

What's left, in one place. Notes for the bigger items are in this folder. Tick items off
(or delete them) in the same commit that does them.

## 0.1.0: the first public release (proposed; trim or extend)

The bar: someone with an L16 can install it from the release notes, take photos and look at
them, keep Android, and not lose the camera to a bad state.

- [ ] Install path: a release image from CI, release notes (releases/0.1.0.md) and a tested
      dual-boot install/upgrade from scratch, following only the notes
- [ ] CI green with every package (the last run lost its runner after 71 min: watch memory)
- [ ] Lens-blocked warning: the five proximity sensors ([proximity-sensors.md](proximity-sensors.md))
- [ ] Low battery and low storage: warn before a shot that can't be saved
- [ ] GPU freeze when changing ISO in continuous mode: retest (A530 preemption is off since r64)
- [ ] Camera module (ASIC) stalls: none seen lately; keep the UART logs running while testing
- [ ] Gallery: quit didn't happen once (SIGTERM); not reproduced
- [ ] README/wiki: what works, what doesn't, how to get back to Android, the photo pipeline

## Camera app (Viewfinder)

- [ ] AF-D: faces as a trigger (stock refocuses on face count/size/position changes)
- [ ] Shutter sound choice (stock has several)
- [ ] Portrait UI rotation
- [ ] Preview: denoising (stock's ABF) and local tone mapping (LTM): our dim previews are noisy
- [ ] Preview digital gain: stock's ISP reached ~6.9-7.4x in a dim scene, more than the 4.13x
      boost we apply; pairing its log to frames was unreliable (see memory: stock preview tone)
- [ ] Optional experiment: a preview exposure longer than the firmware's 42 ms (0x0067's
      shutter max); stock never does it
- [ ] Video

## Gallery (Lightbox)

- [ ] Menu: copy to clipboard, show in folder, ...
- [ ] Select several photos (delete, share)
- [ ] Process while charging with the screen off (stock's "dream" processing, opt-in)
- [ ] Thumbnails loaded as they scroll into view (all load at once today)
- [ ] Renderer speed (l16-render: ~15-25 s a photo)

## System

- [ ] Power-key long press: the power menu once froze on its first frame during a 10 s hold;
      not seen on a short hold since
- [ ] Hack cleanup: comments about the persistent transfer streams

## Later

- [ ] CLI and Python library for scripting the camera (astro, timelapse)
- [ ] The ToF laser (ST VL53L0X on ASIC1's own I2C): stock never ranges with it; its CCB
      command (process_tof_cmd in ASIC1.bin) is unknown
- [ ] UX and optimisation passes over everything
