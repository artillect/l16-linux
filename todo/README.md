# To do

What's left, in one place. Notes for the bigger items are in this folder. Tick items off
(or delete them) in the same commit that does them.

## 0.1.0: the first public release (proposed; trim or extend)

The bar: someone with an L16 can install it from the release notes, take photos and look at
them, keep Android, and not lose the camera to a bad state.

- [x] Install path: a release image from CI, release notes (releases/0.1.0.md) and a tested
      dual-boot install/upgrade from scratch, following only the notes (0.1.0, 2026-10-04;
      a user's install from the notes went smoothly)
- [x] CI green with every package (2026-10-01: ~54 min, the kernel ~42 of it)
- [x] Lens-blocked warning: the five proximity sensors ([proximity-sensors.md](proximity-sensors.md));
      later maybe the in-pocket check
- [x] Device status as stock's: battery and captures left, low storage banners, the battery-low
      screen at 10%, no photo under 1 GB free
- [x] GPU freeze when changing ISO in continuous mode: fine in use since r64 (A530 preemption off)
- [ ] Camera module (ASIC) stalls: none seen lately; keep the UART logs running while testing
- [x] Gallery: quit didn't happen once (SIGTERM); not seen again
- [x] README/wiki: what works, what doesn't, how to get back to Android, the photo pipeline
- [x] Linux only (no Android): tested both ways with 0.1.0-rc1 (2026-10-03): installed to
      `userdata` and `boot`, first-boot resize, rebooting, the camera, and back to Android
      (stock `boot`, `userdata` erased); written up in the wiki

## 0.2.0: systemd (needs a reinstall)

postmarketOS v26.06 runs Phosh on systemd by default (postmarketos-ui-phosh: pmb:default-systemd);
we chose OpenRC. GNOME's Settings switches that start services need systemd: SSH and Remote
Desktop (gnome-control-center calls StartUnit) and File Sharing (gsd-sharing starts
`%s.service` units), so they do nothing here. An installed system can't switch init, so 0.2.0
is a reinstall: everything else that only changes at install time stays (single root
partition, ext4, no encryption by default, the 64 GiB dual-boot partition).

0.2.0 on is systemd only. Its packages are published to `systemd/v26.06`; `v26.06` stays as
0.1.x's (OpenRC) repository, so those installs never pull a systemd package (our systemd Phosh
would outrank theirs). Phosh comes from postmarketOS's systemd repository there, so ours is
forked from that one (extra-repos/systemd/phosh) with our keypad patch.

- [x] systemd units for our services (device-light-lfc-systemd, l16-gnss-systemd), enabled by
      presets; OpenRC's parts dropped (initd, temp/phosh, the chrony hook: timesyncd)
- [x] Kernel for postmarketOS's systemd services: nftables (the firewall), zram, PSI (oomd)
- [ ] Build with systemd: tools/ci done (setup.sh, publish/seed to systemd/v26.06); the wiki's
      "Building it yourself" (it says only OpenRC)
- [ ] Re-test what touches init or power: rebooting to Android and back, Linux only, suspend
      and deep sleep (logind, not elogind), suspend-on-blank and the sleep inhibitor, a GPS
      fix, the camera. Done (dual boot, 2026-10-05): first-boot resize, all services start,
      bootmode's dual-boot setup
- [ ] Settings' SSH switch works (2026-10-05); File Sharing works over USB, the firewall blocks
      it over Wi-Fi as upstream (left so: photos over USB below). Drop the "doesn't work"
      notes (release notes, wiki Using Linux)
- [ ] 0.1.x's repository: frozen at what's published; say so in the 0.2.0 release notes
- [ ] The upgrade path from 0.1.x: back up photos and settings, reinstall, restore; tested
      once end to end, written up in the release notes and the wiki

## Camera app (Viewfinder)

- [ ] Manual focus: the host can move lenses (0x0040 write: a hall code per module; 0x0041:
      a fraction of the hard-stop range) and read them back (0x0040 read). B and C set for a
      distance from the factory calibration (818/1500 mm points, infinity = infinity stop +
      200) land within 2 codes (2026-10-04). Next: capture without AF, check a matched-focus
      photo, the A modules (opposite direction, ~1000-code correction), stops probed once and
      saved, then a focus mode in the app (infinity lock for astro, a focus pull, and a
      quick swap for the touch strip between zoom and focus: a user's suggestion)

- [x] AF-D as stock's: motion-then-settle (gyro) and zoom, with the marks
- [ ] AF-D: faces as a trigger (stock refocuses on face count/size/position changes)
- [ ] Shutter sound choice (stock has several)
- [x] Portrait UI rotation (as stock: icons turn in place, text re-laid out, the LRI's
      orientation set)
- [x] In-pocket check: a countdown from 20 s, then the screen blanks (suspend on battery)
      instead of closing the app; the screen stays on while the camera is in front
- [ ] Preview: denoising (stock's ABF) and local tone mapping (LTM): our dim previews are noisy
- [ ] Preview digital gain: stock's ISP reached ~6.9-7.4x in a dim scene, more than the 4.13x
      boost we apply; pairing its log to frames was unreliable (see memory: stock preview tone)
- [ ] Optional experiment: a preview exposure longer than the firmware's 42 ms (0x0067's
      shutter max); stock never does it
- [ ] Video

## Gallery (Lightbox)

- [x] Menu: copy the LRI or the render, show either in its folder
- [x] Select several photos (a button or a long press) and delete them
- [x] Swiping between photos as on a phone (the photo follows the finger); a swipe isn't a tap
- [x] Rotate left/right: the LRI's own orientation, so every reader turns it (2026-10-04)
- [ ] Editing (crop, exposure, colour, ...) with stock's pipeline (libcp): the edits in a
      sidecar file beside the LRI (stock kept them in its database), the original untouched
- [ ] Share (no share portal on the desktop yet)
- [ ] Process while charging with the screen off (stock's "dream" processing, opt-in)
- [ ] Thumbnails loaded as they scroll into view: missing ones are made by a worker already;
      the cached ones are all read when Lightbox opens (only matters for a big library)
- [ ] Renderer speed (l16-render: ~15-25 s a photo)

## System

- [x] The SLPI stopped answering sensor requests after a while: a sensor report left running
      through a long suspend (~10 min) wedged it until a restart. Kernel r102 stops the reports
      at suspend and asks again at resume (stock never streams through a sleep); an 8 h sleep
      kept rotation, and the SoC reaches vmin again
- [x] Photos' capture time was 1970 (the ASICs' uptime): fixed in r86 (SET_TIME as stock's)
- [x] Lightbox dates photos by the LRI's capture time (it showed the file time)
- [x] Deep sleep (XO shutdown in suspend, kernel r94): Wi-Fi's PCIe controller and PHY
      powered down for suspend, the RPM clocks' unused handoff votes withdrawn, the modem's
      GPLL0 branch without a parent (as stock)
- [x] Reopening Viewfinder while it closed (about 3 s) did nothing or left the preview dead
      until a reboot: the launch went to the closing instance, or two drove the camera at
      once. Fixed in l16-camera r6 (the app's name given up at the close, a new camera waits
      for the old one's streams); WirePlumber's camera monitors, which held the camera too,
      are off (device-light-lfc r28). Kernel r95 logs light-ccb's stream on/off, to keep an
      eye on its count
- [x] Measure the idle drain on battery overnight now that the crystal shuts off (was 4-5%/h):
      ~2.2%/h over 8.5 h asleep (2026-10-04, r102); phones manage 0.5-1%/h, so more to find
- [x] UFS at boot: "hw clk gating enabled failed": the v2 controller has no UniPro clock
      gating attributes (stock enables only the UTP gating); kernel r95 skips them on v2
- [ ] Kernel tracing (CONFIG_FTRACE) is on for debugging suspend; drop it if it costs anything

- [ ] Power-key long press: the power menu once froze on its first frame during a 10 s hold;
      not seen on a short hold since
- [ ] Hack cleanup: comments about the persistent transfer streams
- [x] Display: phoc's "Atomic commit failed: Resource busy", about 2,500 a minute while
      scrolling (a frame in six dropped): an unused plane phoc keeps off in every commit is
      tracked by DRM's fake commit, which msm completed only at the end of its commit tail,
      after the flip event had reached phoc. Kernel r106 (patch 0028) completes it at the
      start: none since, and scrolling is smoother. Upstream msm has the same code
- [ ] Display: MDP5's "pp done time out, lm=0", a few per boot (3 on r106's first boot, all
      before login; up to a few dozen per boot before). Find what triggers them
- [ ] Phosh's home bar sometimes disappears (and once the status bar wouldn't dismiss, until
      the power button blanked and unblanked the screen). Intermittent: seen 2026-10-05 after
      opening Viewfinder in portrait, turning to landscape, then opening and closing the
      shade; the same steps again didn't repeat it. Possibly tied to the display errors above
- [ ] gsd-color corrupts its memory once location services are on: at start it fails to get
      a location from GeoClue ("Failed to connect to GeoClue2 service: Timeout was reached",
      for Night Light's automatic schedule, though Night Light is off) and logs
      "g_atomic_ref_count_dec: assertion 'old_value > 0' failed" twice (a double unref).
      Later its gdbus thread spins at 100% of a core in g_hash_table_contains under
      g_source_attach, holding the main context, so it stops answering D-Bus: logout hangs
      on "Color not responding" (2026-10-05: ~28 min of a core burnt since login). First
      seen the minute location services were turned on. GeoClue's own where-am-i shows the
      same assertion (KDE Discuss), so likely libgeoclue's timeout path; and why GeoClue
      times out for it at all
- [ ] Home screen sometimes entirely black with the status bar drawn (so phoc drew the top
      layer); it comes back on swiping the shade down or tapping an (unseen) icon. Seen
      2026-10-05 on phoc r107; a few times before for a moment. Next time: phosh-home and
      phoc layer/render debug logs (DebugControl LogDomains), not screenshots
- [ ] Lock screen keeps the orientation from before it blanked for a moment after waking:
      Phosh releases the accelerometer while blanked and nothing else holds it now
      (light-lfc-rotate is gone), so a fresh reading takes the sensor's start-up
- [x] App switcher thumbnail of a pinned app (Viewfinder) stayed landscape in portrait:
      phoc r114 draws it as the view shows on its output
- [ ] Photos over USB without SSH: show up on the computer when plugged in, as a camera or a
      portable device. First check what stock did (MTP or PTP, which folders, and why the gallery
      locks while plugged in). The pieces: FunctionFS is in the kernel, `umtprd` (an MTP server,
      with a systemd unit) and `usb-moded` are in the repos, and the gadget now has only
      `ncm.usb0` (USB networking), which MTP could sit beside. MTP shows every file (the .lri
      too) without locking the device; PTP importers may skip .lri. Check whether umtprd can
      present itself as a camera, and whether it announces photos taken while plugged in

## Later

- [x] Geotagging in Viewfinder (GPS through geoclue: l16-gnss), the place in Lightbox
- [x] XTRA: l16-gnss injects it (stock's way, from the server whose file this modem dates)

- [ ] CLI and Python library for scripting the camera (astro, timelapse)
- [ ] The ToF laser (ST VL53L0X on ASIC1's own I2C): stock never ranges with it; its CCB
      command (process_tof_cmd in ASIC1.bin) is unknown
- [ ] UX and optimisation passes over everything
- [ ] A grip that cools it: the SoC's heat (CPU ~80 C under the preview) spreads into the camera
      modules beside it
