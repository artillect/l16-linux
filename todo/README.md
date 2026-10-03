# To do

What's left, in one place. Notes for the bigger items are in this folder. Tick items off
(or delete them) in the same commit that does them.

## 0.1.0: the first public release (proposed; trim or extend)

The bar: someone with an L16 can install it from the release notes, take photos and look at
them, keep Android, and not lose the camera to a bad state.

- [ ] Install path: a release image from CI, release notes (releases/0.1.0.md) and a tested
      dual-boot install/upgrade from scratch, following only the notes
- [x] CI green with every package (2026-10-01: ~54 min, the kernel ~42 of it)
- [x] Lens-blocked warning: the five proximity sensors ([proximity-sensors.md](proximity-sensors.md));
      later maybe the in-pocket check
- [x] Device status as stock's: battery and captures left, low storage banners, the battery-low
      screen at 10%, no photo under 1 GB free
- [x] GPU freeze when changing ISO in continuous mode: fine in use since r64 (A530 preemption off)
- [ ] Camera module (ASIC) stalls: none seen lately; keep the UART logs running while testing
- [x] Gallery: quit didn't happen once (SIGTERM); not seen again
- [ ] README/wiki: what works, what doesn't, how to get back to Android, the photo pipeline

## Camera app (Viewfinder)

- [x] AF-D as stock's: motion-then-settle (gyro) and zoom, with the marks
- [ ] AF-D: faces as a trigger (stock refocuses on face count/size/position changes)
- [ ] Shutter sound choice (stock has several)
- [x] Portrait UI rotation (as stock: icons turn in place, text re-laid out, the LRI's
      orientation set)
- [ ] In-pocket check: a longer warning before it acts (a countdown), and suspend instead of
      closing the app (it closed on a leg in a dark room, as designed, with little warning)
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

- [ ] The SLPI stops answering sensor requests after a while (2026-10-02, ~8 h into a boot
      with suspends, the modem and GPS tests): enabling any of its sensors times out, and
      the accelerometer, gyro and light sensor freeze on their last values. Restarting the
      SLPI (remoteproc stop/start) brings them all back. Cause unknown; not reproduced on
      demand. Kernel r85 tells the SLPI when the CPUs suspend (stock's sleepstate), which it
      didn't know before; watch whether it comes back
- [x] Photos' capture time was 1970 (the ASICs' uptime): fixed in r86 (SET_TIME as stock's)
- [x] Lightbox dates photos by the LRI's capture time (it showed the file time)
- [x] Deep sleep (XO shutdown in suspend, kernel r94): Wi-Fi's PCIe controller and PHY
      powered down for suspend, the RPM clocks' unused handoff votes withdrawn, the modem's
      GPLL0 branch without a parent (as stock)
- [ ] A camera stream left on (a crash mid-stream, 2026-10-02) keeps light-ccb's stream count
      up: the preview never starts again and the ASICs stay powered until a reboot. The
      driver's stream on/off logging is staged for the next kernel; WirePlumber's camera
      monitors, which held the camera too, are off (device-light-lfc r28)
- [ ] Measure the idle drain on battery overnight now that the crystal shuts off (was 4-5%/h)
- [ ] UFS at boot: "dme-get: attr-id 0xa00b failed", "hw clk gating enabled failed": the v2
      controller has no UniPro clock gating attributes (stock enables only the UTP gating,
      which works); harmless. Patch 0027 skips them on v2, uncommitted, for the next kernel bump
- [ ] Kernel tracing (CONFIG_FTRACE) is on for debugging suspend; drop it if it costs anything

- [ ] Power-key long press: the power menu once froze on its first frame during a 10 s hold;
      not seen on a short hold since
- [ ] Hack cleanup: comments about the persistent transfer streams

## Later

- [x] Geotagging in Viewfinder (GPS through geoclue: l16-gnss), the place in Lightbox
- [x] XTRA: l16-gnss injects it (stock's way, from the server whose file this modem dates)

- [ ] CLI and Python library for scripting the camera (astro, timelapse)
- [ ] The ToF laser (ST VL53L0X on ASIC1's own I2C): stock never ranges with it; its CCB
      command (process_tof_cmd in ASIC1.bin) is unknown
- [ ] UX and optimisation passes over everything
- [ ] A grip that cools it: the SoC's heat (CPU ~80 C under the preview) spreads into the camera
      modules beside it
