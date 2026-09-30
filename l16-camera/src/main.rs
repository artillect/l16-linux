// l16-camera: a camera app for the Light L16 on Linux, laid out after OpenLight (the L16's
// community camera app): exposure readout on the left, the preview, and on the right the
// shutter between the two exposure dials, the last photo above and the toolbar below.
//
// Preview: libcamera (libcamerasrc) on the light-ccb driver, which previews one module at a
// time (A1 28 mm, B4 70 mm, C5 150 mm); zoom in between is a crop. Exposure and focus go to
// the driver's controls directly; the ASICs meter and focus themselves. Photos: the
// preview stops and l16-capture takes an LRI with the modules for the zoom.

mod ccb;
mod input;
mod transfer;
mod zoomview;

use gst::prelude::*;
use gtk::prelude::*;
use gtk::{cairo, gdk, glib};
use std::cell::RefCell;
use std::f64::consts::PI;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use zoomview::ZoomView;

// OpenLight's value lists (res/values/arrays.xml)
const ISO: &[i32] = &[
    100, 125, 160, 200, 250, 320, 400, 500, 640, 800, 1000, 1250, 1600, 2400, 3200,
];
const SHUTTER: &[&str] = &[
    "1/8000", "1/6400", "1/5000", "1/4000", "1/3200", "1/2500", "1/2000", "1/1600", "1/1250",
    "1/1000", "1/800", "1/640", "1/500", "1/400", "1/320", "1/240", "1/200", "1/160", "1/150",
    "1/120", "1/100", "1/80", "1/60", "1/50", "1/40", "1/30", "1/24", "1/20", "1/15", "1/12",
    "1/10", "1/8", "1/6", "1/5", "1/4", "0.3", "0.4", "0.5", "0.6", "0.8", "1", "1.25", "1.66",
    "2", "2.5", "3.2", "4", "5", "6", "8", "10", "12", "15",
];
const TIMERS: &[u32] = &[0, 3, 5, 10, 20];
// OpenLight's burst modes (burst_3, burst_6)
const BURSTS: &[u8] = &[1, 3, 6];
// zoom stops: OpenLight's primes, with the L16's real 70 mm B modules
const PRIMES: &[f64] = &[28.0, 35.0, 70.0, 150.0];
const ZOOM_MIN: f64 = 28.0;
const ZOOM_MAX: f64 = 150.0;
const MODULE_MM: [f64; 3] = [28.0, 70.0, 150.0];
const ACCENT: (f64, f64, f64) = (0.0, 0.694, 0.929); // #00B1ED
const STRIP_LEN: f64 = 768.0;

const CSS: &str = "
window.camera { background: #000; }
.hud-value { color: #fff; font-size: 16px; font-weight: 600; }
.hud-unit { color: rgba(255,255,255,0.7); font-size: 13px; }
.toolbar { background: rgba(0,0,0,0.4); }
.toolbar button, button.flat-white { background: none; border: none; box-shadow: none;
    color: #fff; font-size: 16px; font-weight: 600; min-width: 68px; min-height: 56px; }
.toolbar button.on { color: #00B1ED; }
.status { color: #fff; font-size: 17px; font-weight: 600; background: rgba(0,0,0,0.55);
    border-radius: 8px; padding: 4px 14px; }
.countdown { color: #fff; font-size: 96px; font-weight: 700; }
.thumb { border: 2px solid rgba(255,255,255,0.8); border-radius: 4px; }
.blackout { background: #000; }
.burst-screen { background: #000; }
.burst-count { color: #fff; font-size: 48px; }
.burst-saving { color: #fff; font-size: 24px; }
.burst-badge { color: #fff; font-size: 13px; font-weight: 600; border: 1px solid #fff;
    border-radius: 3px; padding: 0 4px; }
";

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Auto,
    Manual,
}

// a photo's way from the shutter to the LRI (threads report on App::stage_tx)
enum Stage {
    Captured(Result<PathBuf, String>), // the ASICs hold it (records in DIR)
    Transferred(Result<PathBuf, String>), // in DIR/asic*.raw
    Saved(Result<PathBuf, String>),       // the LRI
}

#[derive(Clone, Copy, PartialEq)]
enum Dial {
    Iso,
    Shutter,
}

struct State {
    mode: Mode,
    iso: f64,     // position, see iso_at
    shutter: f64, // position, see secs_at
    zoom: f64,
    module: usize,
    timer: usize,
    grid: bool,
    busy: bool,
    counting: bool,
    saving: u32,
    seq: u32,
    burst_count: u8,  // the burst screen's number (0: not showing)
    burst_captured: bool,
    burst: usize,
    flash: bool,
    dragged: bool,
    wheel: Option<Dial>,
    wheel_start: f64,
    zoom_start: f64,
    zoom_wheel_until: Option<Instant>,
    focus_until: Option<Instant>,
    focus_at: Option<(f64, f64)>,
    live_iso: i32,
    live_secs: f64,
    strip_down: bool,
    strip_x0: i32,
    strip_x: i32,
    strip_t0: Instant,
    settle: Option<glib::SourceId>,
    switching: Option<mpsc::Receiver<usize>>,
}

struct App {
    st: RefCell<State>,
    ccb: Option<ccb::Ccb>,
    focusing: Arc<AtomicBool>,
    stage_tx: mpsc::Sender<Stage>,
    transfers: RefCell<Option<transfer::Transfers>>,
    transfer_turn: Arc<Mutex<()>>,
    stage_rx: mpsc::Receiver<Stage>,
    pipeline: gst::Pipeline,
    paintable: gdk::Paintable,
    _bus: gst::bus::BusWatchGuard,
    view: ZoomView,
    marks: gtk::DrawingArea,
    wheels: gtk::DrawingArea,
    hud: Vec<gtk::Label>,
    top: gtk::DrawingArea,
    bottom: gtk::DrawingArea,
    shutter: gtk::DrawingArea,
    thumb: gtk::Image,
    thumb_spin: gtk::DrawingArea,
    blackout: gtk::Box,
    burst_screen: gtk::Box,
    burst_label: gtk::Label,
    burst_saving: gtk::Box,
    burst_dots: gtk::DrawingArea,
    burst_badge: gtk::Label,
    mode_label: gtk::Label,
    toolbar: gtk::Revealer,
    mode_btn: gtk::Button,
    timer_btn: gtk::Button,
    grid_btn: gtk::Button,
    burst_btn: gtk::Button,
    flash_btn: gtk::Button,
    status: gtk::Label,
    countdown: gtk::Label,
}

const ISO_MAX: f64 = 3200.0;
const ISO_MIN: f64 = 100.0;
const SECS_MAX: f64 = 15.0;
const SECS_MIN: f64 = 1.0 / 8000.0;

fn iso_at(pos: f64) -> i32 {
    (ISO_MAX * (ISO_MIN / ISO_MAX).powf(pos.clamp(0.0, 1.0))).round() as i32
}

fn secs_at(pos: f64) -> f64 {
    SECS_MAX * (SECS_MIN / SECS_MAX).powf(pos.clamp(0.0, 1.0))
}

fn iso_pos(iso: f64) -> f64 {
    (iso / ISO_MAX).ln() / (ISO_MIN / ISO_MAX).ln()
}

fn secs_pos(t: f64) -> f64 {
    (t / SECS_MAX).ln() / (SECS_MIN / SECS_MAX).ln()
}

fn shutter_secs(s: &str) -> f64 {
    match s.strip_prefix("1/") {
        Some(d) => 1.0 / d.parse::<f64>().unwrap_or(1.0),
        None => s.parse().unwrap_or(1.0),
    }
}

fn fmt_secs(t: f64) -> String {
    if t >= 0.3 {
        let s = format!("{t:.2}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        format!("1/{:.0}", 1.0 / t)
    }
}

fn module_for(zoom: f64) -> usize {
    if zoom < 70.0 {
        0
    } else if zoom < 150.0 {
        1
    } else {
        2
    }
}

// text at (x, y), vertically centred; align 0 = left, 0.5 = centre, 1 = right
fn text(cr: &cairo::Context, s: &str, x: f64, y: f64, size: f64, align: f64) {
    cr.set_font_size(size);
    if let Ok(e) = cr.text_extents(s) {
        cr.move_to(x - e.width() * align - e.x_bearing(), y - e.height() / 2.0 - e.y_bearing());
        let _ = cr.show_text(s);
    }
}

fn make_pipeline() -> (gst::Pipeline, gdk::Paintable) {
    // the frames are converted into buffers of our own: the sink shows libcamera's buffers
    // in place, and stopping the camera (for a capture) freed them under the display
    let pipeline = gst::parse::launch(
        "libcamerasrc ! video/x-raw,width=1040,height=780,format=BGRx \
         ! queue max-size-buffers=1 leaky=downstream ! videoconvert \
         ! video/x-raw,format=RGBx ! gtk4paintablesink name=sink",
    )
    .expect("preview pipeline")
    .downcast::<gst::Pipeline>()
    .expect("a pipeline");
    let sink = pipeline.by_name("sink").expect("sink");
    let paintable = sink.property::<gdk::Paintable>("paintable");
    (pipeline, paintable)
}

impl App {
    fn start_preview(self: &Rc<Self>) {
        let _ = self.pipeline.set_state(gst::State::Playing);
    }

    fn stop_preview(&self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }

    fn exposure_us(&self) -> i32 {
        let st = self.st.borrow();
        ((secs_at(st.shutter) * 1e6).round() as i32).clamp(1, 15_000_000)
    }

    // auto: the ASICs meter; manual: the chosen ISO and shutter (the preview slows down for
    // long shutters, as stock's)
    fn apply_exposure(&self) {
        let Some(c) = &self.ccb else { return };
        let (mode, iso) = {
            let st = self.st.borrow();
            (st.mode, iso_at(st.iso))
        };
        match mode {
            Mode::Auto => {
                c.set(ccb::EXPOSURE_AUTO, 0);
            }
            Mode::Manual => {
                c.set(ccb::ISO, iso);
                c.set(ccb::EXPOSURE_US, self.exposure_us());
                c.set(ccb::EXPOSURE_AUTO, 1);
            }
        }
    }

    fn refresh(&self) {
        let st = self.st.borrow();
        let (ev, iso, secs) = match st.mode {
            Mode::Auto => ("0".to_string(), st.live_iso, st.live_secs),
            Mode::Manual => ("–".to_string(), iso_at(st.iso), secs_at(st.shutter)),
        };
        self.hud[0].set_text(&ev);
        self.hud[1].set_text(&if iso > 0 { iso.to_string() } else { "–".into() });
        self.hud[2].set_text(&if secs > 0.0 { fmt_secs(secs) } else { "–".into() });
        self.hud[3].set_text(&format!("{:.0}", st.zoom));
        self.mode_label.set_text(match st.mode {
            Mode::Auto => "auto",
            Mode::Manual => "manual",
        });
        self.mode_btn.set_label(match st.mode {
            Mode::Auto => "auto",
            Mode::Manual => "manual",
        });
        let t = TIMERS[st.timer];
        self.timer_btn.set_label(&if t == 0 { "timer off".into() } else { format!("timer {t}s") });
        if t == 0 {
            self.timer_btn.remove_css_class("on");
        } else {
            self.timer_btn.add_css_class("on");
        }
        self.grid_btn.set_label(if st.grid { "grid 3×3" } else { "grid off" });
        self.flash_btn.set_label(if st.flash { "flash auto" } else { "flash off" });
        if st.flash {
            self.flash_btn.add_css_class("on");
        } else {
            self.flash_btn.remove_css_class("on");
        }
        let b = BURSTS[st.burst];
        self.burst_btn.set_label(&if b > 1 { format!("burst {b}") } else { "burst off".into() });
        self.burst_badge.set_text(&format!("×{b}"));
        self.burst_badge.set_visible(b > 1);
        if b > 1 {
            self.burst_btn.add_css_class("on");
        } else {
            self.burst_btn.remove_css_class("on");
        }
        if st.grid {
            self.grid_btn.add_css_class("on");
        } else {
            self.grid_btn.remove_css_class("on");
        }
        drop(st);
        self.top.queue_draw();
        self.bottom.queue_draw();
        self.shutter.queue_draw();
        self.marks.queue_draw();
    }

    fn show_status(self: &Rc<Self>, msg: &str, secs: u64) {
        self.status.set_text(msg);
        self.status.set_visible(true);
        if secs > 0 {
            let app = self.clone();
            let msg = msg.to_string();
            glib::timeout_add_local_once(Duration::from_secs(secs), move || {
                if app.status.text() == msg.as_str() {
                    app.status.set_visible(false);
                }
            });
        }
    }

    fn set_dial(&self, dial: Dial, pos: f64) {
        {
            let mut st = self.st.borrow_mut();
            match dial {
                Dial::Iso => st.iso = pos.clamp(0.0, 1.0),
                Dial::Shutter => st.shutter = pos.clamp(0.0, 1.0),
            }
        }
        if let Some(c) = &self.ccb {
            match dial {
                Dial::Iso => {
                    c.set(ccb::ISO, iso_at(self.st.borrow().iso));
                }
                Dial::Shutter => {
                    c.set(ccb::EXPOSURE_US, self.exposure_us());
                }
            }
        }
        self.refresh();
        self.wheels.queue_draw();
    }

    fn set_mode(&self, mode: Mode) {
        self.st.borrow_mut().mode = mode;
        self.apply_exposure();
        self.refresh();
    }

    // the driver restarts the ASICs' preview on the new module; the stream carries on
    // (it takes a moment: off the UI thread). The crop changes with the new module's
    // first frame, from switched().
    fn switch_module(&self, module: usize) {
        let (tx, rx) = mpsc::channel();
        self.st.borrow_mut().switching = Some(rx);
        thread::spawn(move || {
            if let Some(c) = ccb::Ccb::open() {
                c.set(ccb::MODULE, module as i32);
            }
            let _ = tx.send(module);
        });
    }

    fn switched(self: &Rc<Self>) {
        let done = {
            let st = self.st.borrow();
            st.switching.as_ref().and_then(|rx| rx.try_recv().ok())
        };
        let Some(module) = done else { return };
        // the old module's frames stop before the driver returns: the next frame is the
        // new module's
        let (zoom, want) = {
            let mut st = self.st.borrow_mut();
            st.switching = None;
            st.module = module;
            (st.zoom, module_for(st.zoom))
        };
        self.view.set_zoom_next_frame(zoom / MODULE_MM[module]);
        if want != module {
            self.switch_module(want);
        }
    }

    fn set_zoom(self: &Rc<Self>, zoom: f64) {
        let zoom = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
        {
            let mut st = self.st.borrow_mut();
            st.zoom = zoom;
            st.zoom_wheel_until = Some(Instant::now() + Duration::from_millis(700));
            if let Some(id) = st.settle.take() {
                id.remove();
            }
            self.view.set_zoom(zoom / MODULE_MM[st.module]);
        }
        // the module follows once the zoom settles (switching restarts the preview)
        let app = self.clone();
        let id = glib::timeout_add_local_once(Duration::from_millis(400), move || {
            let (want, have, busy) = {
                let mut st = app.st.borrow_mut();
                st.settle = None;
                (module_for(st.zoom), st.module, st.busy || st.switching.is_some())
            };
            if want != have && !busy {
                app.switch_module(want);
            }
        });
        self.st.borrow_mut().settle = Some(id);
        self.refresh();
        self.wheels.queue_draw();
        let wheels = self.wheels.clone();
        glib::timeout_add_local_once(Duration::from_millis(750), move || wheels.queue_draw());
    }

    fn step_prime(self: &Rc<Self>, up: bool) {
        let z = self.st.borrow().zoom;
        let next = if up {
            PRIMES.iter().copied().find(|&p| p > z + 0.5)
        } else {
            PRIMES.iter().rev().copied().find(|&p| p < z - 0.5)
        };
        if let Some(p) = next {
            self.set_zoom(p);
        }
    }

    // focus on @at (preview coordinates), or the centre: a 200x200 window in the module's
    // 4160x3120 pixels, through the zoom's crop (the driver runs AF in the background)
    fn focus(self: &Rc<Self>, at: Option<(f64, f64)>) {
        if self.st.borrow().busy || self.focusing.swap(true, Ordering::SeqCst) {
            return;
        }
        let (w, h) = (self.view.width() as f64, self.view.height() as f64);
        let z = self.view.zoom();
        let (px, py) = at.unwrap_or((w / 2.0, h / 2.0));
        let sx = 2080.0 + (px - w / 2.0) / w * 4160.0 / z;
        let sy = 1560.0 + (py - h / 2.0) / h * 3120.0 / z;
        let fx = ((sx - 100.0).round() as i32).clamp(0, 4160 - 200);
        let fy = ((sy - 100.0).round() as i32).clamp(0, 3120 - 200);
        {
            let mut st = self.st.borrow_mut();
            st.focus_until = Some(Instant::now() + Duration::from_millis(1500));
            st.focus_at = at;
        }
        self.marks.queue_draw();
        let focusing = self.focusing.clone();
        thread::spawn(move || {
            if let Some(c) = ccb::Ccb::open() {
                c.set(ccb::FOCUS_X, fx);
                c.set(ccb::FOCUS_Y, fy);
                c.set(ccb::AF_START, 1);
            }
            focusing.store(false, Ordering::SeqCst);
        });
        let marks = self.marks.clone();
        glib::timeout_add_local_once(Duration::from_millis(1600), move || marks.queue_draw());
    }

    fn shutter_pressed(self: &Rc<Self>) {
        let (busy, counting, t) = {
            let st = self.st.borrow();
            (st.busy, st.counting, TIMERS[st.timer])
        };
        if busy || counting || !self.room_for(BURSTS[self.st.borrow().burst]) {
            return;
        }
        if t == 0 {
            return self.capture();
        }
        self.st.borrow_mut().counting = true;
        let left = Rc::new(RefCell::new(t));
        self.countdown.set_text(&t.to_string());
        self.countdown.set_visible(true);
        let app = self.clone();
        glib::timeout_add_local(Duration::from_secs(1), move || {
            let mut n = left.borrow_mut();
            *n -= 1;
            if *n == 0 {
                app.countdown.set_visible(false);
                app.st.borrow_mut().counting = false;
                app.capture();
                return glib::ControlFlow::Break;
            }
            app.countdown.set_text(&n.to_string());
            glib::ControlFlow::Continue
        });
    }

    // As stock: the driver takes the photo while the preview runs (it pauses for the
    // exposure), with the preview's exposure and focus, and the next one can be taken right
    // away. The records come over the CSI links beside the preview, photo after photo, and
    // are joined into LRIs in the background (a burst: one per frame).
    // room in /tmp for a photo of @frames frames on its way (about 300 MB each: up to 17
    // records of 16 MB), with a margin for the LRI assembly
    fn room_for(self: &Rc<Self>, frames: u8) -> bool {
        let mut fs: libc::statvfs = unsafe { std::mem::zeroed() };
        let path = std::ffi::CString::new("/tmp").unwrap();
        if unsafe { libc::statvfs(path.as_ptr(), &mut fs) } != 0 {
            return true;
        }
        let free = fs.f_bavail as u64 * fs.f_frsize as u64;
        let room = free > (frames as u64 + 1) * 300 << 20;
        if !room {
            self.show_status("waiting for photos to save", 2);
        }
        room
    }

    fn capture(self: &Rc<Self>) {
        let (zoom, burst, seq, dark) = {
            let mut st = self.st.borrow_mut();
            st.busy = true;
            st.saving += 1;
            st.seq += 1;
            (st.zoom, BURSTS[st.burst], st.seq, st.mode == Mode::Auto && st.live_iso > 400)
        };
        self.thumb.set_paintable(Some(&self.paintable.current_image()));
        if burst > 1 {
            self.start_burst_screen(burst);
        } else {
            self.blackout.set_opacity(1.0);
            self.blackout.set_visible(true);
        }
        self.thumb.set_opacity(0.5);
        let app = self.clone();
        self.thumb_spin.add_tick_callback(move |w, _| {
            w.queue_draw();
            if app.st.borrow().saving > 0 {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
        self.refresh();

        // the modules for the zoom, as stock's 28/70/150 sets (bit n + 1 = LRI camera n)
        let cams = match module_for(zoom) {
            0 => 0..10,  // A1-A5 B1-B5
            1 => 5..16,  // B1-B5 C1-C6
            _ => 10..16, // C1-C6
        };
        let mask = cams.fold(0u32, |m, i| m | 1 << (i + 1));
        let stamp = glib::DateTime::now_local()
            .ok()
            .and_then(|d| d.format("%Y%m%d_%H%M%S").ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "photo".into());
        let dir = PathBuf::from(format!("/tmp/l16-shot-{stamp}-{seq}"));
        let tx = self.stage_tx.clone();
        let turn = self.transfer_turn.clone();
        let Some(queue) = self.transfers.borrow().as_ref().map(|t| t.queue.clone()) else {
            self.st.borrow_mut().busy = false;
            self.fade_blackout();
            self.saved();
            return self.show_status("capture failed: no transfer streams", 6);
        };
        // Quick shots: no precapture metering (about half a second; the preview is metered)
        // and one frame per module. In the dark, as stock: the precapture metering, and the
        // ASICs stack several exposures per module when they judge it needed (slower, 4x
        // the data, less noise). Bursts are always quick.
        let flags = if dark && burst == 1 {
            0
        } else {
            ccb::CAPTURE_NO_PRECAPTURE | ccb::CAPTURE_NO_STACK
        };
        thread::spawn(move || {
            let c = match ccb::Ccb::open() {
                Some(c) => c,
                None => return drop(tx.send(Stage::Captured(Err("no camera driver".into())))),
            };
            let t = Instant::now();
            let cap = match c.capture(mask, burst, flags) {
                Ok(cap) => cap,
                Err(e) => return drop(tx.send(Stage::Captured(Err(e)))),
            };
            eprintln!(
                "l16-camera: captured {} in {:.2} s: records {:?} (burst {burst}, status {})",
                dir.display(),
                t.elapsed().as_secs_f64(),
                cap.records,
                cap.status
            );
            // the records come in the order asked for: photos take turns
            let _turn = turn.lock().unwrap();
            match transfer::Photo::new(dir.clone(), cap.records) {
                Ok(p) => queue.lock().unwrap().push_back(p),
                Err(e) => return drop(tx.send(Stage::Captured(Err(e.to_string())))),
            }
            let _ = tx.send(Stage::Captured(Ok(dir.clone())));
            // ASIC by ASIC; at most four in flight per ASIC (a stream has eight buffers)
            let got = |a: usize| -> Option<u16> {
                let q = queue.lock().unwrap();
                q.iter().find(|p| p.dir == dir).map(|p| p.received(a))
            };
            let t = Instant::now();
            let mut err = None;
            'asics: for a in 0..3 {
                for k in 0..cap.records[a] {
                    let start = Instant::now();
                    while got(a).is_some_and(|n| n + 4 <= k) {
                        if start.elapsed() > Duration::from_secs(3) {
                            err = Some(format!("ASIC{} record {k} did not arrive", a + 1));
                            break 'asics;
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                    if let Err(e) = c.transfer(a as u32) {
                        err = Some(e);
                        break 'asics;
                    }
                }
            }
            // the last records arrive within a moment; give up on missing ones
            for _ in 0..50 {
                if err.is_some() || got(0).is_none() {
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }
            let mut q = queue.lock().unwrap();
            if let Some(i) = q.iter().position(|p| p.dir == dir) {
                q.remove(i);
                let _ = tx.send(Stage::Transferred(Err(err.unwrap_or("records missing".into()))));
            } else {
                eprintln!("l16-camera: transferred {} in {:.2} s", dir.display(), t.elapsed().as_secs_f64());
            }
        });
    }

    // OpenLight's burst screen: the number counts up every exposure (100 ms at least), a
    // timer as stock's, then "saving captures" until the photo is taken
    fn start_burst_screen(self: &Rc<Self>, total: u8) {
        let step = {
            let mut st = self.st.borrow_mut();
            st.burst_count = 1;
            st.burst_captured = false;
            let secs = match st.mode {
                Mode::Auto => st.live_secs,
                Mode::Manual => secs_at(st.shutter),
            };
            Duration::from_secs_f64(secs.max(0.1))
        };
        self.burst_label.set_text("1");
        self.burst_label.set_visible(true);
        self.burst_saving.set_visible(false);
        self.burst_screen.set_visible(true);
        let app = self.clone();
        glib::timeout_add_local(step, move || {
            let (n, captured) = {
                let mut st = app.st.borrow_mut();
                st.burst_count += 1;
                (st.burst_count, st.burst_captured)
            };
            if n <= total {
                app.burst_label.set_text(&n.to_string());
                return glib::ControlFlow::Continue;
            }
            if captured {
                app.end_burst_screen();
            } else {
                app.burst_label.set_visible(false);
            }
            glib::ControlFlow::Break
        });
    }

    fn burst_taken(&self) {
        let counting = {
            let mut st = self.st.borrow_mut();
            st.burst_captured = true;
            st.burst_count
        };
        // the counter still running shows its last numbers first
        if counting > 0 && !self.burst_label.is_visible() {
            self.end_burst_screen();
        }
    }

    fn end_burst_screen(&self) {
        self.st.borrow_mut().burst_count = 0;
        self.burst_screen.set_visible(false);
    }

    // the blackout fades as the preview returns (OpenLight: alpha 1 to 0)
    fn fade_blackout(&self) {
        let b = self.blackout.clone();
        let start = Instant::now();
        glib::timeout_add_local(Duration::from_millis(16), move || {
            let t = start.elapsed().as_secs_f64() / 0.25;
            if t >= 1.0 {
                b.set_visible(false);
                return glib::ControlFlow::Break;
            }
            b.set_opacity(1.0 - t);
            glib::ControlFlow::Continue
        });
    }

    fn saved(&self) {
        let left = {
            let mut st = self.st.borrow_mut();
            st.saving = st.saving.saturating_sub(1);
            st.saving
        };
        if left == 0 {
            self.thumb.set_opacity(1.0);
        }
        self.thumb_spin.queue_draw();
    }

    // three dots going round over the thumbnail while photos are saved
    fn draw_thumb_spin(&self, cr: &cairo::Context, w: f64, h: f64) {
        if self.st.borrow().saving == 0 {
            return;
        }
        let t = glib::monotonic_time() as f64 / 1e6;
        for i in 0..3 {
            let a = t * 4.0 + i as f64 * 2.0 * PI / 3.0;
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.arc(w / 2.0 + 10.0 * a.cos(), h / 2.0 + 10.0 * a.sin(), 3.0, 0.0, 2.0 * PI);
            let _ = cr.fill();
        }
    }

    fn run(cmd: &mut Command) -> Result<(), String> {
        match cmd.output() {
            Ok(o) if o.status.success() => Ok(()),
            Ok(o) => {
                let err = String::from_utf8_lossy(&o.stderr);
                let out = String::from_utf8_lossy(&o.stdout);
                let last = err.lines().chain(out.lines()).filter(|l| !l.is_empty()).last();
                Err(last.unwrap_or("failed").to_string())
            }
            Err(e) => Err(e.to_string()),
        }
    }

    fn on_stage(self: &Rc<Self>, stage: Stage) {
        match stage {
            Stage::Captured(Ok(_)) => {
                self.st.borrow_mut().busy = false;
                self.fade_blackout();
                self.burst_taken();
            }
            Stage::Transferred(r) => {
                match r {
                    Ok(dir) => {
                        let out = glib::user_special_dir(glib::UserDirectory::Pictures)
                            .unwrap_or_else(|| glib::home_dir().join("Pictures"))
                            .join("L16");
                        let _ = std::fs::create_dir_all(&out);
                        let name = dir.file_name().map(|n| n.to_string_lossy().into_owned());
                        let stamp = name.unwrap_or_default().replace("l16-shot-", "");
                        let out = out.join(format!("L16_{stamp}.lri"));
                        let tx = self.stage_tx.clone();
                        thread::spawn(move || {
                            let mut raws: Vec<PathBuf> = (1..=3)
                                .map(|a| dir.join(format!("asic{a}.raw")))
                                .filter(|p| p.exists())
                                .collect();
                            raws.sort();
                            let r = App::run(Command::new("l16-lri-assemble").arg(&out).args(&raws));
                            let _ = std::fs::remove_dir_all(&dir);
                            let _ = tx.send(Stage::Saved(r.map(|_| out)));
                        });
                    }
                    Err(e) => {
                        self.saved();
                        self.show_status(&format!("capture failed: {e}"), 6);
                    }
                }
            }
            Stage::Captured(Err(e)) => {
                self.st.borrow_mut().busy = false;
                self.fade_blackout();
                self.burst_taken();
                self.saved();
                self.show_status(&format!("capture failed: {e}"), 6);
            }
            Stage::Saved(Ok(_)) => self.saved(),
            Stage::Saved(Err(e)) => {
                self.saved();
                self.show_status(&format!("saving failed: {e}"), 6);
            }
        }
        self.refresh();
    }

    fn poll(self: &Rc<Self>) {
        // the metered exposure, which the driver mirrors into its controls
        if let Some(c) = &self.ccb {
            if self.st.borrow().mode == Mode::Auto && !self.st.borrow().busy {
                let iso = c.get(ccb::AE_ISO).unwrap_or(0);
                let us = c.get(ccb::AE_EXPOSURE_US).unwrap_or(0);
                let mut st = self.st.borrow_mut();
                st.live_iso = iso;
                st.live_secs = us as f64 / 1e6;
            }
        }
        self.refresh();
    }

    fn on_input(self: &Rc<Self>, ev: input::Ev) {
        match ev {
            input::Ev::Key(input::KEY_CAMERA_FOCUS, true) => self.focus(None),
            input::Ev::Key(input::KEY_CAMERA | input::KEY_VOLUMEUP, true) => self.shutter_pressed(),
            input::Ev::Key(..) => {}
            // the strip's position comes before its touch-down in each report
            input::Ev::StripX(x) => {
                let (down, last) = {
                    let st = self.st.borrow();
                    (st.strip_down, st.strip_x)
                };
                if !down {
                    let mut st = self.st.borrow_mut();
                    st.strip_down = true;
                    st.strip_t0 = Instant::now();
                    st.strip_x0 = x;
                    st.strip_x = x;
                } else {
                    self.st.borrow_mut().strip_x = x;
                    let z = self.st.borrow().zoom;
                    // OpenLight: a full strip length zooms 2.3x
                    self.set_zoom(z * 2.3f64.powf((x - last) as f64 / STRIP_LEN));
                }
            }
            input::Ev::StripTouch(true) => {}
            input::Ev::StripTouch(false) => {
                let (tap, x0) = {
                    let mut st = self.st.borrow_mut();
                    st.strip_down = false;
                    let tap = st.strip_t0.elapsed() < Duration::from_millis(300)
                        && (st.strip_x - st.strip_x0).abs() < 30;
                    (tap, st.strip_x0)
                };
                // taps on the ends step between the primes
                if tap && x0 < 100 {
                    self.step_prime(false);
                } else if tap && x0 > 700 {
                    self.step_prime(true);
                }
            }
        }
    }

    fn draw_marks(&self, cr: &cairo::Context, w: f64, h: f64) {
        let st = self.st.borrow();
        if st.grid {
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.4);
            cr.set_line_width(1.0);
            for i in 1..3 {
                let x = (w * i as f64 / 3.0).round() + 0.5;
                let y = (h * i as f64 / 3.0).round() + 0.5;
                cr.move_to(x, 0.0);
                cr.line_to(x, h);
                cr.move_to(0.0, y);
                cr.line_to(w, y);
            }
            let _ = cr.stroke();
        }
        if st.focus_until.is_some_and(|t| Instant::now() < t) {
            let (cx, cy) = st.focus_at.unwrap_or((w / 2.0, h / 2.0));
            let (s, c) = (40.0, 12.0);
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.set_line_width(2.0);
            for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                cr.move_to(cx + sx * s, cy + sy * (s - c));
                cr.line_to(cx + sx * s, cy + sy * s);
                cr.line_to(cx + sx * (s - c), cy + sy * s);
            }
            let _ = cr.stroke();
        }
    }

    fn draw_wheels(&self, cr: &cairo::Context, w: f64, h: f64) {
        let st = self.st.borrow();
        cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
        // the exposure wheel: the value lists as ticks on an arc beside the dials, turning
        // with the (continuous) value, which sits on the pointer
        if let Some(dial) = st.wheel {
            let (pos, value, ticks): (f64, String, Vec<(f64, String)>) = match dial {
                Dial::Iso => (
                    st.iso,
                    iso_at(st.iso).to_string(),
                    ISO.iter().map(|&i| (iso_pos(i as f64), i.to_string())).collect(),
                ),
                Dial::Shutter => (
                    st.shutter,
                    fmt_secs(secs_at(st.shutter)),
                    SHUTTER.iter().map(|s| (secs_pos(shutter_secs(s)), s.to_string())).collect(),
                ),
            };
            let r = 400.0;
            let (cx, cy) = (w - 340.0 + r, h / 2.0);
            // 0.1 rad between neighbouring list entries, as before
            let k = 0.1 * (ticks.len() - 1) as f64;
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.25);
            cr.set_line_width(2.0);
            cr.arc(cx, cy, r, PI - 0.7, PI + 0.7);
            let _ = cr.stroke();
            for (tp, l) in &ticks {
                // lower values below (the finger goes down for less light)
                let d = (tp - pos) * k;
                if d.abs() > 0.6 {
                    continue;
                }
                let a = PI - d;
                let (x, y) = (cx + r * a.cos(), cy + r * a.sin());
                cr.set_source_rgba(1.0, 1.0, 1.0, 1.0 - d.abs() / 0.7);
                cr.arc(x, y, 3.0, 0.0, 2.0 * PI);
                let _ = cr.fill();
                if d.abs() > 0.06 {
                    text(cr, l, x - 18.0, y, 20.0, 1.0);
                }
            }
            let (x, y) = (cx - r, cy);
            cr.set_source_rgb(ACCENT.0, ACCENT.1, ACCENT.2);
            cr.arc(x, y, 5.0, 0.0, 2.0 * PI);
            let _ = cr.fill();
            text(cr, &value, x - 18.0, y, 34.0, 1.0);
        }
        // the zoom wheel: an arc of dots from 28 (bottom) to 150 mm (top), primes labelled
        if st.zoom_wheel_until.is_some_and(|t| Instant::now() < t) {
            let r = 300.0;
            let (cx, cy) = (w - 340.0 + r, h / 2.0);
            let angle = |z: f64| PI - 0.55 + 1.1 * (z / ZOOM_MIN).ln() / (ZOOM_MAX / ZOOM_MIN).ln();
            let point = |z: f64| {
                let a = angle(z);
                (cx + r * a.cos(), cy + r * a.sin())
            };
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.6);
            for k in 0..=30 {
                let z = ZOOM_MIN * (ZOOM_MAX / ZOOM_MIN).powf(k as f64 / 30.0);
                let (x, y) = point(z);
                cr.arc(x, y, 2.0, 0.0, 2.0 * PI);
                let _ = cr.fill();
            }
            for &p in PRIMES {
                let (x, y) = point(p);
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.arc(x, y, 4.0, 0.0, 2.0 * PI);
                let _ = cr.fill();
                text(cr, &format!("{p:.0}"), x + 12.0, y, 14.0, 0.0);
            }
            let (x, y) = point(st.zoom);
            cr.set_source_rgb(ACCENT.0, ACCENT.1, ACCENT.2);
            cr.arc(x, y, 7.0, 0.0, 2.0 * PI);
            let _ = cr.fill();
            text(cr, &format!("{:.0} mm", st.zoom), x - 18.0, y, 30.0, 1.0);
        }
    }

    fn draw_dial(&self, cr: &cairo::Context, w: f64, h: f64, top: bool) {
        let st = self.st.borrow();
        let manual = st.mode == Mode::Manual;
        let (label, enabled, dial) = match (top, manual) {
            (true, true) => ("ISO", true, Some(Dial::Iso)),
            (false, true) => ("S", true, Some(Dial::Shutter)),
            (true, false) => ("EV", false, None),
            (false, false) => ("S", false, None),
        };
        let (cx, cy, r) = (w / 2.0, h / 2.0, w.min(h) / 2.0 - 2.0);
        if st.wheel.is_some() && st.wheel != dial {
            return;
        }
        if dial.is_some() && st.wheel == dial {
            cr.set_source_rgba(ACCENT.0, ACCENT.1, ACCENT.2, 0.35);
            cr.arc(cx, cy, r, 0.0, 2.0 * PI);
            let _ = cr.fill();
        }
        let alpha = if enabled { 1.0 } else { 0.35 };
        cr.set_source_rgba(1.0, 1.0, 1.0, alpha);
        cr.set_line_width(2.0);
        cr.arc(cx, cy, r, 0.0, 2.0 * PI);
        let _ = cr.stroke();
        cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
        text(cr, label, cx, cy, 15.0, 0.5);
    }

    fn draw_shutter(&self, cr: &cairo::Context, w: f64, h: f64) {
        let busy = self.st.borrow().busy;
        let (cx, cy, r) = (w / 2.0, h / 2.0, w.min(h) / 2.0 - 2.0);
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.set_line_width(3.0);
        cr.arc(cx, cy, r, 0.0, 2.0 * PI);
        let _ = cr.stroke();
        if busy {
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.3);
        }
        cr.arc(cx, cy, r - 7.0, 0.0, 2.0 * PI);
        let _ = cr.fill();
    }
}

fn hud_item(value: &gtk::Label, unit: &str) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
    value.add_css_class("hud-value");
    let u = gtk::Label::new(Some(unit));
    u.add_css_class("hud-unit");
    b.append(value);
    b.append(&u);
    b
}

fn build(gapp: &gtk::Application) {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(CSS);
    gtk::style_context_add_provider_for_display(
        &gdk::Display::default().expect("display"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let window = gtk::ApplicationWindow::builder().application(gapp).title("Camera").build();
    window.add_css_class("camera");

    let (pipeline, paintable) = make_pipeline();
    let bus = pipeline
        .bus()
        .expect("bus")
        .add_watch_local(|_, msg| {
            if let gst::MessageView::Error(e) = msg.view() {
                eprintln!("l16-camera: preview: {} ({:?})", e.error(), e.debug());
            }
            glib::ControlFlow::Continue
        })
        .expect("bus watch");

    // preview, with the grid and focus marks over it
    let view = ZoomView::new(&paintable);
    view.set_hexpand(true);
    view.set_vexpand(true);
    let marks = gtk::DrawingArea::new();
    marks.set_can_target(false);
    let preview = gtk::Overlay::new();
    preview.set_child(Some(&view));
    preview.add_overlay(&marks);
    let blackout = gtk::Box::new(gtk::Orientation::Vertical, 0);
    blackout.add_css_class("blackout");
    blackout.set_can_target(false);
    blackout.set_visible(false);
    preview.add_overlay(&blackout);
    let frame = gtk::AspectFrame::new(0.5, 0.5, 4.0 / 3.0, false);
    frame.set_child(Some(&preview));
    frame.set_hexpand(true);

    // left: the exposure readout
    let hud: Vec<gtk::Label> = (0..4).map(|_| gtk::Label::new(Some("–"))).collect();
    let left = gtk::Box::new(gtk::Orientation::Vertical, 16);
    left.set_size_request(84, -1);
    left.set_valign(gtk::Align::Center);
    for (l, unit) in hud.iter().zip(["ev", "iso", "s", "mm"]) {
        left.append(&hud_item(l, unit));
    }

    // right: last photo, the dials around the shutter, the toolbar opener
    let thumb = gtk::Image::new();
    thumb.set_pixel_size(44);
    thumb.add_css_class("thumb");
    let thumb_spin = gtk::DrawingArea::new();
    thumb_spin.set_can_target(false);
    let thumb_box = gtk::Overlay::new();
    thumb_box.set_child(Some(&thumb));
    thumb_box.add_overlay(&thumb_spin);
    thumb_box.set_halign(gtk::Align::Center);
    thumb_box.set_margin_top(16);
    let dial = |size: i32| {
        let d = gtk::DrawingArea::new();
        d.set_size_request(size, size);
        d.set_halign(gtk::Align::Center);
        d
    };
    let (top, shutter, bottom) = (dial(54), dial(66), dial(54));
    let mode_label = gtk::Label::new(Some("auto"));
    let opener_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    opener_box.append(&gtk::Label::new(Some("︿")));
    opener_box.append(&mode_label);
    let opener = gtk::Button::new();
    opener.set_child(Some(&opener_box));
    opener.add_css_class("flat-white");
    opener.set_halign(gtk::Align::Center);
    opener.set_margin_bottom(8);
    let right = gtk::Box::new(gtk::Orientation::Vertical, 13);
    right.set_size_request(190, -1);
    let spacer = || {
        let s = gtk::Box::new(gtk::Orientation::Vertical, 0);
        s.set_vexpand(true);
        s
    };
    right.append(&thumb_box);
    right.append(&spacer());
    right.append(&top);
    right.append(&shutter);
    right.append(&bottom);
    right.append(&spacer());
    right.append(&opener);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    row.append(&left);
    row.append(&frame);
    row.append(&right);

    // the toolbar: mode, timer, grid
    let mode_btn = gtk::Button::with_label("auto");
    let timer_btn = gtk::Button::with_label("timer off");
    let grid_btn = gtk::Button::with_label("grid off");
    let burst_btn = gtk::Button::with_label("burst off");
    let flash_btn = gtk::Button::with_label("flash off");
    let close_btn = gtk::Button::with_label("✕");
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    bar.add_css_class("toolbar");
    bar.append(&mode_btn);
    bar.append(&flash_btn);
    bar.append(&timer_btn);
    bar.append(&grid_btn);
    bar.append(&burst_btn);
    let fill = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    fill.set_hexpand(true);
    bar.append(&fill);
    bar.append(&close_btn);
    let toolbar = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideUp)
        .child(&bar)
        .valign(gtk::Align::End)
        .build();

    let wheels = gtk::DrawingArea::new();
    wheels.set_can_target(false);
    wheels.set_halign(gtk::Align::End);
    wheels.set_size_request(560, -1);
    let status = gtk::Label::new(None);
    status.add_css_class("status");
    status.set_valign(gtk::Align::Start);
    status.set_halign(gtk::Align::Center);
    status.set_margin_top(14);
    status.set_visible(false);
    status.set_can_target(false);
    let countdown = gtk::Label::new(None);
    countdown.add_css_class("countdown");
    countdown.set_visible(false);
    countdown.set_can_target(false);

    // OpenLight's BurstView: black over everything (touches too), the frame number, then
    // "saving captures" under three dots until the photo is taken
    let burst_label = gtk::Label::new(None);
    burst_label.add_css_class("burst-count");
    let burst_dots = gtk::DrawingArea::new();
    burst_dots.set_size_request(72, 72);
    burst_dots.set_halign(gtk::Align::Center);
    let saving_text = gtk::Label::new(Some("saving captures"));
    saving_text.add_css_class("burst-saving");
    let burst_saving = gtk::Box::new(gtk::Orientation::Vertical, 24);
    burst_saving.append(&burst_dots);
    burst_saving.append(&saving_text);
    burst_saving.set_visible(false);
    let burst_inner = gtk::Box::new(gtk::Orientation::Vertical, 0);
    burst_inner.set_valign(gtk::Align::Center);
    burst_inner.set_halign(gtk::Align::Center);
    burst_inner.append(&burst_label);
    burst_inner.append(&burst_saving);
    let burst_screen = gtk::Box::new(gtk::Orientation::Vertical, 0);
    burst_screen.add_css_class("burst-screen");
    burst_screen.append(&burst_inner);
    burst_inner.set_vexpand(true);
    burst_screen.set_visible(false);
    burst_screen.add_controller(gtk::GestureClick::new()); // swallows taps
    let burst_badge = gtk::Label::new(None);
    burst_badge.add_css_class("burst-badge");
    burst_badge.set_halign(gtk::Align::Center);
    burst_badge.set_visible(false);
    left.prepend(&burst_badge);

    let root = gtk::Overlay::new();
    root.set_child(Some(&row));
    root.add_overlay(&wheels);
    root.add_overlay(&status);
    root.add_overlay(&countdown);
    root.add_overlay(&toolbar);
    root.add_overlay(&burst_screen);
    window.set_child(Some(&root));

    let (stage_tx, stage_rx) = mpsc::channel();
    let app = Rc::new(App {
        st: RefCell::new(State {
            mode: Mode::Auto,
            iso: 1.0,
            shutter: secs_pos(1.0 / 60.0),
            zoom: ZOOM_MIN,
            module: 0,
            timer: 0,
            grid: false,
            busy: false,
            counting: false,
            saving: 0,
            seq: 0,
            burst_count: 0,
            burst_captured: false,
            burst: 0,
            flash: false,
            dragged: false,
            wheel: None,
            wheel_start: 0.0,
            zoom_start: ZOOM_MIN,
            zoom_wheel_until: None,
            focus_until: None,
            focus_at: None,
            live_iso: 0,
            live_secs: 0.0,
            strip_down: false,
            strip_x0: 0,
            strip_x: 0,
            strip_t0: Instant::now(),
            settle: None,
            switching: None,
        }),
        ccb: ccb::Ccb::open(),
        focusing: Arc::new(AtomicBool::new(false)),
        stage_tx,
        stage_rx,
        transfers: RefCell::new(None),
        transfer_turn: Arc::new(Mutex::new(())),
        pipeline,
        paintable,
        _bus: bus,
        view,
        marks,
        wheels,
        hud,
        top,
        bottom,
        shutter,
        thumb,
        thumb_spin,
        blackout,
        burst_screen,
        burst_label,
        burst_saving,
        burst_dots,
        burst_badge,
        mode_label,
        toolbar,
        mode_btn,
        timer_btn,
        grid_btn,
        burst_btn,
        flash_btn,
        status,
        countdown,
    });
    if app.ccb.is_none() {
        app.show_status("no light-ccb camera driver", 0);
    }

    // drawing
    let a = app.clone();
    app.marks.set_draw_func(move |_, cr, w, h| a.draw_marks(cr, w as f64, h as f64));
    let a = app.clone();
    app.wheels.set_draw_func(move |_, cr, w, h| a.draw_wheels(cr, w as f64, h as f64));

    let a = app.clone();
    app.top.set_draw_func(move |_, cr, w, h| a.draw_dial(cr, w as f64, h as f64, true));
    let a = app.clone();
    app.bottom.set_draw_func(move |_, cr, w, h| a.draw_dial(cr, w as f64, h as f64, false));
    let a = app.clone();
    app.shutter.set_draw_func(move |_, cr, w, h| a.draw_shutter(cr, w as f64, h as f64));
    let a = app.clone();
    app.thumb_spin.set_draw_func(move |_, cr, w, h| a.draw_thumb_spin(cr, w as f64, h as f64));
    app.burst_dots.set_draw_func(|_, cr, w, h| {
        // three dots going round
        let t = glib::monotonic_time() as f64 / 1e6;
        let (w, h) = (w as f64, h as f64);
        for i in 0..3 {
            let a = t * 4.0 + i as f64 * 2.0 * PI / 3.0;
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.arc(w / 2.0 + 20.0 * a.cos(), h / 2.0 + 20.0 * a.sin(), 5.0, 0.0, 2.0 * PI);
            let _ = cr.fill();
        }
    });

    // preview: tap focuses (or closes the toolbar), drag and pinch zoom
    let click = gtk::GestureClick::new();
    let a = app.clone();
    click.connect_released(move |_, _, x, y| {
        if a.st.borrow().dragged {
            return;
        }
        if a.toolbar.reveals_child() {
            a.toolbar.set_reveal_child(false);
        } else {
            a.focus(Some((x, y)));
        }
    });
    app.view.add_controller(click);
    let drag = gtk::GestureDrag::new();
    let a = app.clone();
    drag.connect_drag_begin(move |_, _, _| {
        let mut st = a.st.borrow_mut();
        st.zoom_start = st.zoom;
        st.dragged = false;
    });
    let a = app.clone();
    drag.connect_drag_update(move |_, _, dy| {
        if dy.abs() > 8.0 {
            a.st.borrow_mut().dragged = true;
            let z = a.st.borrow().zoom_start;
            a.set_zoom(z * 2.3f64.powf(-dy / 400.0));
        }
    });
    app.view.add_controller(drag);
    let pinch = gtk::GestureZoom::new();
    let a = app.clone();
    pinch.connect_begin(move |_, _| {
        let mut st = a.st.borrow_mut();
        st.zoom_start = st.zoom;
        st.dragged = true;
    });
    let a = app.clone();
    pinch.connect_scale_changed(move |_, s| {
        let z = a.st.borrow().zoom_start;
        a.set_zoom(z * s);
    });
    app.view.add_controller(pinch);

    // the dials: tag and drag (manual mode)
    for (widget, dial) in [(app.top.clone(), Dial::Iso), (app.bottom.clone(), Dial::Shutter)] {
        let drag = gtk::GestureDrag::new();
        let a = app.clone();
        drag.connect_drag_begin(move |_, _, _| {
            let mut st = a.st.borrow_mut();
            if st.mode != Mode::Manual {
                return;
            }
            st.wheel = Some(dial);
            st.wheel_start = match dial {
                Dial::Iso => st.iso,
                Dial::Shutter => st.shutter,
            };
            drop(st);
            a.refresh();
            a.wheels.queue_draw();
        });
        let a = app.clone();
        drag.connect_drag_update(move |_, _, dy| {
            let (active, start) = {
                let st = a.st.borrow();
                (st.wheel == Some(dial), st.wheel_start)
            };
            if active {
                a.set_dial(dial, start + dy * 0.001);
            }
        });
        let a = app.clone();
        drag.connect_drag_end(move |_, _, _| {
            let a = a.clone();
            glib::timeout_add_local_once(Duration::from_millis(600), move || {
                a.st.borrow_mut().wheel = None;
                a.refresh();
                a.wheels.queue_draw();
            });
        });
        widget.add_controller(drag);
    }

    let click = gtk::GestureClick::new();
    let a = app.clone();
    click.connect_released(move |_, _, _, _| a.shutter_pressed());
    app.shutter.add_controller(click);

    // toolbar
    let a = app.clone();
    opener.connect_clicked(move |_| {
        let open = a.toolbar.reveals_child();
        a.toolbar.set_reveal_child(!open);
    });
    let a = app.clone();
    close_btn.connect_clicked(move |_| a.toolbar.set_reveal_child(false));
    let a = app.clone();
    app.mode_btn.connect_clicked(move |_| {
        let mode = match a.st.borrow().mode {
            Mode::Auto => Mode::Manual,
            Mode::Manual => Mode::Auto,
        };
        a.set_mode(mode);
    });
    let a = app.clone();
    app.timer_btn.connect_clicked(move |_| {
        {
            let mut st = a.st.borrow_mut();
            st.timer = (st.timer + 1) % TIMERS.len();
        }
        a.refresh();
    });
    let a = app.clone();
    app.flash_btn.connect_clicked(move |_| {
        let on = {
            let mut st = a.st.borrow_mut();
            st.flash = !st.flash;
            st.flash
        };
        if let Some(c) = &a.ccb {
            c.set(ccb::FLASH, on as i32);
        }
        a.refresh();
    });
    let a = app.clone();
    app.burst_btn.connect_clicked(move |_| {
        {
            let mut st = a.st.borrow_mut();
            st.burst = (st.burst + 1) % BURSTS.len();
        }
        a.refresh();
    });
    let a = app.clone();
    app.grid_btn.connect_clicked(move |_| {
        {
            let mut st = a.st.borrow_mut();
            st.grid = !st.grid;
        }
        a.refresh();
    });

    // hardware: shutter button, touch strip
    let (tx, rx) = mpsc::channel();
    input::spawn(tx);
    let a = app.clone();
    glib::timeout_add_local(Duration::from_millis(15), move || {
        while let Ok(ev) = rx.try_recv() {
            a.on_input(ev);
        }
        a.switched();
        while let Ok(stage) = a.stage_rx.try_recv() {
            a.on_stage(stage);
        }
        glib::ControlFlow::Continue
    });
    let a = app.clone();
    glib::timeout_add_local(Duration::from_millis(300), move || {
        a.poll();
        glib::ControlFlow::Continue
    });

    let a = app.clone();
    window.connect_close_request(move |_| {
        if let Some(t) = a.transfers.borrow_mut().as_mut() {
            t.stop();
        }
        a.stop_preview();
        glib::Propagation::Proceed
    });

    if let Some(c) = &app.ccb {
        c.set(ccb::MODULE, 0);
        c.set(ccb::FLASH, 0);
    }
    app.start_preview();
    app.apply_exposure();
    // the photo transfer streams, beside the preview (done: Stage::Transferred)
    let (done_tx, done_rx) = mpsc::channel();
    match transfer::Transfers::start(done_tx) {
        Ok(t) => *app.transfers.borrow_mut() = Some(t),
        Err(e) => app.show_status(&format!("no photo transfers: {e}"), 0),
    }
    let tx = app.stage_tx.clone();
    thread::spawn(move || {
        while let Ok(r) = done_rx.recv() {
            if tx.send(Stage::Transferred(r)).is_err() {
                break;
            }
        }
    });
    app.refresh();
    window.fullscreen();
    window.present();
}

fn main() -> glib::ExitCode {
    gst::init().expect("gstreamer");
    let app = gtk::Application::builder().application_id("org.l16linux.Camera").build();
    app.connect_activate(build);
    app.run()
}
