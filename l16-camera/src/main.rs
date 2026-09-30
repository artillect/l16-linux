// l16-camera: a camera app for the Light L16 on Linux, laid out after OpenLight (the L16's
// community camera app): exposure readout on the left, the preview, and on the right the
// shutter between the two exposure dials, the last photo above and the toolbar below.
//
// Preview: libcamera (libcamerasrc) on the light-ccb driver, which previews one module at a
// time (A1 28 mm, B4 70 mm, C5 150 mm); zoom in between is a crop. Exposure and focus go to
// the driver's controls directly; the ASICs meter and focus themselves. Photos: the
// preview stops and l16-capture takes an LRI with the modules for the zoom.

mod ccb;
mod haptics;
mod input;
mod settings;
mod transfer;
mod wb;
mod zoomview;

use gst::prelude::*;
use gtk::prelude::*;
use gtk::{cairo, gdk, glib};
use std::cell::RefCell;
use std::collections::HashMap;
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
// the preview modules' focal lengths: A1, and B4 from 70 mm on (stock never previews on
// the 150 mm modules; it crops B4)
const MODULE_MM: [f64; 2] = [28.0, 70.0];
// the mode wheel's touch band: its labels (two either side of the chosen one)
const MODE_TOUCH_H: i32 = 320;
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
.settings { background: #000; }
.settings list { background: #000; }
.settings row, .chooser row { padding: 14px 32px; border-bottom: 1px solid rgba(255,255,255,0.15);
    background: none; }
.settings row:active, .chooser row:active { background: rgba(255,255,255,0.12); }
.set-title { color: #fff; font-size: 18px; font-weight: 600; }
.set-sub { color: rgba(255,255,255,0.6); font-size: 14px; }
.set-value { color: #00B1ED; font-size: 17px; font-weight: 600; }
.set-chevron { color: rgba(255,255,255,0.6); font-size: 22px; }
.settings switch { background: rgba(255,255,255,0.25); border: none; }
.settings switch:checked { background: #00B1ED; }
.settings switch slider { background: #fff; border: none; box-shadow: none; }
.chooser { background: rgba(0,0,0,0.6); }
.chooser-card { background: #1c1c1c; border-radius: 12px; }
.chooser-card list { background: none; }
.chooser-title { color: rgba(255,255,255,0.6); font-size: 15px; padding: 16px 32px 8px 32px; }
.chooser-check { color: #00B1ED; font-size: 18px; font-weight: 700; }
";

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Auto,
    Iso,     // ISO priority: the ASICs choose the shutter
    Shutter, // shutter priority: they choose the ISO
    Manual,
}

// stock's modes in its mode wheel's order (CameraMode; its video mode aside)
const MODES: [Mode; 4] = [Mode::Auto, Mode::Iso, Mode::Shutter, Mode::Manual];

impl Mode {
    fn index(self) -> usize {
        MODES.iter().position(|&m| m == self).unwrap_or(0)
    }

    // the mode wheel's label
    fn label(self) -> &'static str {
        ["auto", "iso priority", "shutter priority", "manual"][self.index()]
    }

    // the toolbar opener's (and the settings file's)
    fn short(self) -> &'static str {
        ["auto", "iso", "shutter", "manual"][self.index()]
    }

    // the dials above and below the shutter (stock's getTopControlWheel / getBottomControlWheel)
    fn dials(self) -> (Option<Dial>, Dial) {
        match self {
            Mode::Auto => (None, Dial::Ev),
            Mode::Iso => (Some(Dial::Iso), Dial::Ev),
            Mode::Shutter => (Some(Dial::Ev), Dial::Shutter),
            Mode::Manual => (Some(Dial::Iso), Dial::Shutter),
        }
    }

    fn fixes_iso(self) -> bool {
        matches!(self, Mode::Iso | Mode::Manual)
    }

    fn fixes_shutter(self) -> bool {
        matches!(self, Mode::Shutter | Mode::Manual)
    }
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
    Ev,
}

struct State {
    mode: Mode,
    mode_pos: f64, // the mode wheel's position, 0 (auto) to 1 (manual)
    mode_start: f64,
    mode_swiped: bool,
    iso: f64,     // position, see iso_at
    shutter: f64, // position, see secs_at
    ev: f64,      // position, see ev_at
    zoom: f64,
    module: usize,
    timer: usize,
    grid: u8, // 0 off, 1 3x3, 2 golden ratio
    histogram: bool,
    busy: bool,
    counting: bool,
    saving: u32,
    seq: u32,
    burst_count: u8,  // the burst screen's number (0: not showing)
    burst_captured: bool,
    burst: usize,
    flash: u8, // 0 off, 1 auto, 2 on
    wb: usize,  // wb::PRESETS
    dragged: bool,
    wheel: Option<Dial>,
    wheel_start: f64,
    // closes the exposure wheel after a drag; a new drag cancels it
    wheel_close: Option<glib::SourceId>,
    haptics: u8, // 0 off, 1 normal, 2 strong (stock's)
    zoom_start: f64,
    zoom_wheel_until: Option<Instant>,
    focus_until: Option<Instant>,
    focus_at: Option<(f64, f64)>,
    zoom_sent: Instant,
    // continuous focus: the metered exposure (log) and zoom at the last focus
    caf_ref: Option<(f64, f64)>,
    // the settings screen's
    metering: u8, // 0 centre-weighted, 1 touch
    caf: bool,
    stacked: bool,
    exposure_info: bool,
    inverse_wheel: bool,
    strip_zoom: bool,
    asleep: bool, // the preview stopped while the screen is off
    live_iso: i32,
    live_secs: f64,
    strip_down: bool,
    strip_x0: i32,
    strip_x: i32,
    strip_t0: Instant,
    settle: Option<glib::SourceId>,
    switching: Option<mpsc::Receiver<usize>>,
}

impl State {
    // what's kept between runs, as the settings file's lines
    fn saved(&self) -> String {
        let mode = self.mode.short();
        format!(
            "mode={mode}\niso={}\nshutter={}\nev={}\nflash={}\ntimer={}\ngrid={}\nhistogram={}\nburst={}\n\
             wb={}\nmetering={}\ncaf={}\nstacked={}\nexposure_info={}\ninverse_wheel={}\nhaptics={}\nstrip_zoom={}\n",
            self.iso,
            self.shutter,
            self.ev,
            self.flash,
            self.timer,
            self.grid,
            self.histogram as u8,
            self.burst,
            self.wb,
            self.metering,
            self.caf as u8,
            self.stacked as u8,
            self.exposure_info as u8,
            self.inverse_wheel as u8,
            self.haptics,
            self.strip_zoom as u8,
        )
    }

    fn load(&mut self, m: &std::collections::HashMap<String, String>) {
        let num = |k: &str| m.get(k).and_then(|v| v.parse::<f64>().ok());
        let flag = |k: &str, d: bool| num(k).map_or(d, |v| v != 0.0);
        if let Some(&mode) = MODES.iter().find(|md| m.get("mode").map(String::as_str) == Some(md.short())) {
            self.mode = mode;
        }
        self.ev = num("ev").unwrap_or(self.ev).clamp(0.0, 1.0);
        self.iso = num("iso").unwrap_or(self.iso).clamp(0.0, 1.0);
        self.shutter = num("shutter").unwrap_or(self.shutter).clamp(0.0, 1.0);
        self.flash = num("flash").map_or(self.flash, |v| (v as u8).min(2));
        self.timer = num("timer").map_or(self.timer, |v| (v as usize).min(TIMERS.len() - 1));
        self.grid = num("grid").map_or(self.grid, |v| (v as u8).min(2));
        self.histogram = flag("histogram", self.histogram);
        self.burst = num("burst").map_or(self.burst, |v| (v as usize).min(BURSTS.len() - 1));
        self.metering = num("metering").map_or(self.metering, |v| (v as u8).min(1));
        self.caf = flag("caf", self.caf);
        self.wb = num("wb").map_or(self.wb, |v| (v as usize).min(wb::PRESETS.len() - 1));
        self.stacked = flag("stacked", self.stacked);
        self.exposure_info = flag("exposure_info", self.exposure_info);
        self.inverse_wheel = flag("inverse_wheel", self.inverse_wheel);
        self.haptics = num("haptics").map_or(self.haptics, |v| (v as u8).min(2));
        self.strip_zoom = flag("strip_zoom", self.strip_zoom);
    }
}

// a row of the settings screen: a switch, or a value that opens a list to choose from
enum SettingKind {
    Switch(fn(&State) -> bool, fn(&mut State, bool)),
    Choice(&'static [&'static str], fn(&State) -> usize, fn(&mut State, usize)),
}

struct SettingRow {
    title: &'static str,
    sub: &'static str,
    kind: SettingKind,
}

const SETTINGS: &[SettingRow] = &[
    SettingRow {
        title: "Metering",
        sub: "Where auto exposure meters: the centre, or the spot you tap",
        kind: SettingKind::Choice(
            &["Centre-weighted", "Touch"],
            |s| s.metering as usize,
            |s, v| s.metering = v as u8,
        ),
    },
    SettingRow {
        title: "Continuous focus",
        sub: "Refocus when the scene changes (AF-D), outside manual mode",
        kind: SettingKind::Switch(|s| s.caf, |s, v| s.caf = v),
    },
    SettingRow {
        title: "Stacked capture",
        sub: "In low light, several exposures per module for less noise",
        kind: SettingKind::Switch(|s| s.stacked, |s, v| s.stacked = v),
    },
    SettingRow {
        title: "Exposure info",
        sub: "EV, ISO, shutter and focal length beside the preview",
        kind: SettingKind::Switch(|s| s.exposure_info, |s, v| s.exposure_info = v),
    },
    SettingRow {
        title: "Haptics",
        sub: "Vibration as the dials and the zoom turn",
        kind: SettingKind::Choice(
            &["Off", "Normal", "Strong"],
            |s| s.haptics as usize,
            |s, v| s.haptics = v as u8,
        ),
    },
    SettingRow {
        title: "Inverse wheel scroll",
        sub: "Turn the exposure wheels the other way",
        kind: SettingKind::Switch(|s| s.inverse_wheel, |s, v| s.inverse_wheel = v),
    },
    SettingRow {
        title: "Touch strip",
        sub: "Zoom with the touch strip",
        kind: SettingKind::Switch(|s| s.strip_zoom, |s, v| s.strip_zoom = v),
    },
];

struct App {
    st: RefCell<State>,
    ccb: Option<ccb::Ccb>,
    focusing: Arc<AtomicBool>,
    stage_tx: mpsc::Sender<Stage>,
    ctl_tx: mpsc::Sender<(u32, i32)>,
    // the driver's metered ISO and exposure (us), read by a thread of their own
    metered: Arc<[std::sync::atomic::AtomicI32; 2]>,
    transfers: RefCell<Option<transfer::Transfers>>,
    transfer_turn: Arc<Mutex<()>>,
    stage_rx: mpsc::Receiver<Stage>,
    input_rx: mpsc::Receiver<input::Ev>,
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
    right: gtk::Box,
    mode_wheel: gtk::DrawingArea,
    mode_touch: gtk::Box,
    timer_btn: gtk::Button,
    grid_btn: gtk::Button,
    hist_btn: gtk::Button,
    hist: RefCell<Vec<u32>>,
    burst_btn: gtk::Button,
    flash_btn: gtk::Button,
    wb_btn: gtk::Button,
    cal: wb::Calibration,
    motor: haptics::Haptics,
    // a photo's view preferences for the LRI (white balance, exposure), by its directory
    photo_args: RefCell<HashMap<PathBuf, Vec<String>>>,
    hud_box: gtk::Box,
    settings_page: gtk::Overlay,
    last_saved: RefCell<String>,
    // logind's sleep inhibitor while photos are on their way (dropped: released)
    sleep_inhibitor: RefCell<Option<std::os::fd::OwnedFd>>,
    status: gtk::Label,
    countdown: gtk::Label,
}

const ISO_MAX: f64 = 3200.0;
const ISO_ANALOG_MAX: f64 = 775.0; // stock's analog ceiling: 7.75x
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

// EV compensation in thirds: +3 EV at the top of the wheel (position 0), -3 at the bottom
fn ev_at(pos: f64) -> i32 {
    (9.0 - 18.0 * pos.clamp(0.0, 1.0)).round() as i32
}

fn ev_pos(ev: i32) -> f64 {
    (9 - ev) as f64 / 18.0
}

fn fmt_ev(ev: i32) -> String {
    if ev == 0 {
        return "0".into();
    }
    let sign = if ev > 0 { "+" } else { "-" };
    let (whole, third) = (ev.abs() / 3, ["", "⅓", "⅔"][(ev.abs() % 3) as usize]);
    if whole == 0 {
        format!("{sign}{third}")
    } else {
        format!("{sign}{whole}{third}")
    }
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

// the list value nearest @pos on a dial (its tick marks)
fn dial_tick(dial: Dial, pos: f64) -> usize {
    let ticks: Vec<f64> = match dial {
        Dial::Iso => ISO.iter().map(|&i| iso_pos(i as f64)).collect(),
        Dial::Shutter => SHUTTER.iter().map(|s| secs_pos(shutter_secs(s))).collect(),
        Dial::Ev => (-9..=9).map(ev_pos).collect(),
    };
    let mut best = 0;
    for (i, t) in ticks.iter().enumerate() {
        if (t - pos).abs() < (ticks[best] - pos).abs() {
            best = i;
        }
    }
    best
}

// the zoom wheel's dot nearest @zoom (31 dots, 28 to 150 mm, even in log)
fn zoom_dot(zoom: f64) -> i64 {
    (30.0 * (zoom / ZOOM_MIN).ln() / (ZOOM_MAX / ZOOM_MIN).ln()).round() as i64
}

fn preview_module(zoom: f64) -> usize {
    usize::from(zoom >= 70.0)
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
        "libcamerasrc name=src ! video/x-raw,width=1040,height=780,format=BGRx \
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

    // auto: the ASICs meter; the priority modes: they meter the other half; manual: the chosen
    // ISO and shutter (the preview slows down for long shutters, as stock's)
    fn apply_exposure(&self) {
        let (mode, iso, ev) = {
            let st = self.st.borrow();
            (st.mode, iso_at(st.iso), ev_at(st.ev))
        };
        let _ = self.ctl_tx.send((ccb::EV, ev));
        if mode.fixes_iso() {
            let _ = self.ctl_tx.send((ccb::ISO, iso));
        }
        if mode.fixes_shutter() {
            let _ = self.ctl_tx.send((ccb::EXPOSURE_US, self.exposure_us()));
        }
        let priority = match mode {
            Mode::Iso => 1,
            Mode::Shutter => 2,
            _ => 0,
        };
        let _ = self.ctl_tx.send((ccb::PRIORITY, priority));
        let _ = self.ctl_tx.send((ccb::EXPOSURE_AUTO, (mode == Mode::Manual) as i32));
    }

    fn refresh(&self) {
        let st = self.st.borrow();
        let ev = if st.mode == Mode::Manual { "–".to_string() } else { fmt_ev(ev_at(st.ev)) };
        let iso = if st.mode.fixes_iso() { iso_at(st.iso) } else { st.live_iso };
        let secs = if st.mode.fixes_shutter() { secs_at(st.shutter) } else { st.live_secs };
        // stock keeps the sensors' analog gain at most 7.75 (ISO 775) and has its ISP apply
        // the rest as digital gain, up to 4.13x (ISO 3200): the preview does the same here
        self.view.set_gain((iso as f64 / ISO_ANALOG_MAX).clamp(1.0, ISO_MAX / ISO_ANALOG_MAX));
        self.hud[0].set_text(&ev);
        self.hud[1].set_text(&if iso > 0 { iso.to_string() } else { "–".into() });
        self.hud[2].set_text(&if secs > 0.0 { fmt_secs(secs) } else { "–".into() });
        self.hud[3].set_text(&format!("{:.0}", st.zoom));
        self.mode_label.set_text(st.mode.short());
        let t = TIMERS[st.timer];
        self.timer_btn.set_label(&if t == 0 { "timer off".into() } else { format!("timer {t}s") });
        if t == 0 {
            self.timer_btn.remove_css_class("on");
        } else {
            self.timer_btn.add_css_class("on");
        }
        self.grid_btn.set_label(["grid off", "grid 3×3", "grid golden"][st.grid as usize]);
        self.hist_btn.set_label(if st.histogram { "histogram on" } else { "histogram off" });
        if st.histogram {
            self.hist_btn.add_css_class("on");
        } else {
            self.hist_btn.remove_css_class("on");
        }
        self.flash_btn.set_label(["flash off", "flash auto", "flash on"][st.flash as usize]);
        self.wb_btn.set_label(&format!("wb {}", wb::PRESETS[st.wb]));
        if st.wb > 0 {
            self.wb_btn.add_css_class("on");
        } else {
            self.wb_btn.remove_css_class("on");
        }
        self.hud_box.set_opacity(if st.exposure_info { 1.0 } else { 0.0 });
        if st.flash > 0 {
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
        if st.grid > 0 {
            self.grid_btn.add_css_class("on");
        } else {
            self.grid_btn.remove_css_class("on");
        }
        let saved = st.saved();
        drop(st);
        if *self.last_saved.borrow() != saved {
            settings::save(&saved);
            *self.last_saved.borrow_mut() = saved;
        }
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
        let before = {
            let st = self.st.borrow();
            match dial {
                Dial::Iso => st.iso,
                Dial::Shutter => st.shutter,
                Dial::Ev => st.ev,
            }
        };
        if dial_tick(dial, before) != dial_tick(dial, pos.clamp(0.0, 1.0)) {
            self.buzz(6);
        }
        {
            let mut st = self.st.borrow_mut();
            match dial {
                Dial::Iso => st.iso = pos.clamp(0.0, 1.0),
                Dial::Shutter => st.shutter = pos.clamp(0.0, 1.0),
                Dial::Ev => st.ev = pos.clamp(0.0, 1.0),
            }
        }
        // through the control thread: a focus run holds the driver for seconds
        let ctl = match dial {
            Dial::Iso => (ccb::ISO, iso_at(self.st.borrow().iso)),
            Dial::Shutter => (ccb::EXPOSURE_US, self.exposure_us()),
            Dial::Ev => (ccb::EV, ev_at(self.st.borrow().ev)),
        };
        let _ = self.ctl_tx.send(ctl);
        self.refresh();
        self.wheels.queue_draw();
    }

    // white balance: libcamera's AWB (auto), or a preset's gains for the preview module
    fn apply_wb(&self) {
        let Some(src) = self.pipeline.by_name("src") else { return };
        let (preset, module) = {
            let st = self.st.borrow();
            (st.wb, st.module)
        };
        let gains = self.cal.gains(preset, module);
        eprintln!("l16-camera: white balance {} (module {module}): {gains:?}", wb::PRESETS[preset]);
        match gains {
            None => src.set_property("awb-enable", true),
            Some((r, b)) => {
                src.set_property("colour-gains", gst::Array::new([r, b]));
                src.set_property("awb-enable", false);
            }
        }
    }

    fn set_mode(&self, mode: Mode) {
        self.st.borrow_mut().mode = mode;
        self.apply_exposure();
        self.refresh();
    }

    // stock's toolbar: opening it swaps the right-hand column (last photo, dials, shutter)
    // for the mode wheel (only while no photo is being taken)
    fn show_toolbar(&self, open: bool) {
        if open && self.st.borrow().busy {
            return;
        }
        self.toolbar.set_reveal_child(open);
        self.right.set_opacity(if open { 0.0 } else { 1.0 });
        self.right.set_can_target(!open);
        self.mode_wheel.set_visible(open);
        self.mode_touch.set_visible(open);
        let pos = self.st.borrow().mode.index() as f64 / (MODES.len() - 1) as f64;
        self.st.borrow_mut().mode_pos = pos;
        self.mode_wheel.queue_draw();
    }

    // the mode wheel's positions: 0 for auto to 1 for manual, a mode per 1/3
    // @apply: send the mode to the camera as the wheel passes it, as stock does (the driver
    // sends only what changed: one or two messages a mode); false only re-snaps the wheel
    fn set_mode_pos(&self, pos: f64, apply: bool) {
        let pos = pos.clamp(0.0, 1.0);
        let max = (MODES.len() - 1) as f64;
        let mode = MODES[(pos * max).round() as usize];
        self.st.borrow_mut().mode_pos = pos;
        // the mode changes as the wheel passes half way to it, as stock's
        if apply {
            self.set_mode(mode);
        } else if mode != self.st.borrow().mode {
            self.st.borrow_mut().mode = mode;
            self.refresh();
        }
        self.mode_wheel.queue_draw();
    }

    // stock's ModeWheel (landscape) in this screen's units (its pixels / 1.75): the modes as
    // text on a drum down the right edge, the chosen one level with a white bar at the edge
    fn mode_item_y(pos: f64, idx: usize, h: f64) -> f64 {
        let max = (MODES.len() - 1) as f64;
        let th = 6f64.to_radians() * (idx as f64 - max * pos);
        h / 2.0 - th.sin() * h * th.cos().powi(5)
    }

    fn draw_mode_wheel(&self, cr: &cairo::Context, w: f64, h: f64) {
        let pos = self.st.borrow().mode_pos;
        // the strip's shade: clear at its left, a quarter black at the edge
        let g = cairo::LinearGradient::new(0.0, 0.0, w, 0.0);
        g.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, 0.0);
        g.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 0.25);
        let _ = cr.set_source(&g);
        cr.rectangle(0.0, 0.0, w, h);
        let _ = cr.fill();
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.rectangle(w - 10.0, h / 2.0 - 21.0, 10.0, 42.0);
        let _ = cr.fill();
        let max = (MODES.len() - 1) as f64;
        let base = (max * pos).floor() as i64;
        cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
        // stock's size, unless the longest label wouldn't fit (our font is wider than stock's)
        cr.set_font_size(55.0);
        let widest = MODES
            .iter()
            .filter_map(|m| cr.text_extents(m.label()).ok())
            .map(|e| e.width())
            .fold(0.0, f64::max);
        let size = 55.0 * ((w - 48.0 - 16.0) / widest).min(1.0);
        for i in -2i64..=2 {
            let idx = base + i;
            if idx < 0 || idx > max as i64 {
                continue;
            }
            let th = 6f64.to_radians() * (idx as f64 - max * pos);
            let y = Self::mode_item_y(pos, idx as usize, h);
            // stock's perspective: the slot's exponent, and neighbours at 3/4 alpha
            let k = if i == 0 { 26 } else { 52 / i.abs() as i32 };
            let alpha = th.cos() * if i == 0 { 1.0 } else { 0.75 };
            let label = MODES[idx as usize].label();
            cr.set_font_size(size * th.cos().powi(k));
            let Ok(e) = cr.text_extents(label) else { continue };
            cr.move_to(w - e.width() - 48.0 - e.x_bearing(), y - e.height() / 2.0 - e.y_bearing());
            cr.text_path(label);
            cr.set_source_rgba(0.0, 0.0, 0.0, alpha);
            cr.set_line_width(2.0);
            let _ = cr.stroke_preserve();
            cr.set_source_rgba(1.0, 1.0, 1.0, alpha);
            let _ = cr.fill();
        }
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
            (st.zoom, preview_module(st.zoom))
        };
        self.view.set_zoom_next_frame(zoom / MODULE_MM[module]);
        self.apply_wb();
        if want != module {
            self.switch_module(want);
        }
    }

    fn set_zoom(self: &Rc<Self>, zoom: f64) {
        let zoom = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
        let before = self.st.borrow().zoom;
        let (lo, hi) = (before.min(zoom), before.max(zoom));
        if PRIMES.iter().any(|&p| p > lo + 0.01 && p <= hi + 0.01 && (p - before).abs() > 0.01) {
            self.buzz(15);
        } else if zoom_dot(before) != zoom_dot(zoom) {
            self.buzz(5);
        }
        {
            let mut st = self.st.borrow_mut();
            st.zoom = zoom;
            st.zoom_wheel_until = Some(Instant::now() + Duration::from_millis(700));
            if let Some(id) = st.settle.take() {
                id.remove();
            }
            self.view.set_zoom(zoom / MODULE_MM[st.module]);
            // the ASICs follow the zoom as stock's app sends it, every 30-50 ms
            if st.zoom_sent.elapsed() >= Duration::from_millis(40) {
                st.zoom_sent = Instant::now();
                let _ = self.ctl_tx.send((ccb::ZOOM, (zoom / ZOOM_MIN * 1000.0).round() as i32));
            }
        }
        // once it settles: the last factor, the mirrors, and the preview module
        let app = self.clone();
        let id = glib::timeout_add_local_once(Duration::from_millis(200), move || {
            let (want, have, busy, zoom) = {
                let mut st = app.st.borrow_mut();
                st.settle = None;
                (preview_module(st.zoom), st.module, st.busy || st.switching.is_some(), st.zoom)
            };
            let _ = app.ctl_tx.send((ccb::ZOOM, (zoom / ZOOM_MIN * 1000.0).round() as i32));
            let _ = app.ctl_tx.send((ccb::MIRRORS, 1));
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
        self.focus_run(at, true);
    }

    // @marks: show the focus marks (not for continuous focus's runs)
    fn focus_run(self: &Rc<Self>, at: Option<(f64, f64)>, marks: bool) {
        if self.st.borrow().busy || self.focusing.swap(true, Ordering::SeqCst) {
            return;
        }
        // continuous focus waits for the scene to change from here
        self.st.borrow_mut().caf_ref = None;
        let (w, h) = (self.view.width() as f64, self.view.height() as f64);
        let z = self.view.zoom();
        let (px, py) = at.unwrap_or((w / 2.0, h / 2.0));
        let sx = 2080.0 + (px - w / 2.0) / w * 4160.0 / z;
        let sy = 1560.0 + (py - h / 2.0) / h * 3120.0 / z;
        let fx = ((sx - 100.0).round() as i32).clamp(0, 4160 - 200);
        let fy = ((sy - 100.0).round() as i32).clamp(0, 3120 - 200);
        if marks {
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
        self.settings_page.set_visible(false);
        self.show_toolbar(false);
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
        self.hold_sleep();
        self.feedback("camera-shutter");
        let (zoom, burst, seq, dark, stacked) = {
            let mut st = self.st.borrow_mut();
            st.busy = true;
            st.saving += 1;
            st.seq += 1;
            (st.zoom, BURSTS[st.burst], st.seq, st.mode == Mode::Auto && st.live_iso > 400, st.stacked)
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
        let view = {
            let st = self.st.borrow();
            let iso = if st.mode.fixes_iso() { iso_at(st.iso) } else { st.live_iso };
            let secs = if st.mode.fixes_shutter() { secs_at(st.shutter) } else { st.live_secs };
            let mut v = vec![
                "--iso".to_string(),
                iso.to_string(),
                "--exposure-us".into(),
                ((secs * 1e6).round() as u64).to_string(),
                "--awb-mode".into(),
                wb::AWB_MODE[st.wb].to_string(),
            ];
            if let Some((r, b)) = self.cal.gains(st.wb, st.module) {
                v.push("--wb".into());
                v.push(format!("{r},{b}"));
            }
            v
        };
        self.photo_args.borrow_mut().insert(dir.clone(), view);
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
        // the data, less noise; the settings can turn stacking off). Bursts are always quick.
        let flags = if dark && burst == 1 {
            if stacked {
                0
            } else {
                ccb::CAPTURE_NO_STACK
            }
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
            let secs = if st.mode.fixes_shutter() { secs_at(st.shutter) } else { st.live_secs };
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

    // No suspend while photos are on their way: the ASICs lose them when powered off, and
    // the phone suspends seconds after the screen blanks. A logind block inhibitor, held
    // until the last photo is saved.
    fn hold_sleep(&self) {
        if self.sleep_inhibitor.borrow().is_some() {
            return;
        }
        let r = gtk::gio::bus_get_sync(gtk::gio::BusType::System, None::<&gtk::gio::Cancellable>)
            .and_then(|bus| {
                bus.call_with_unix_fd_list_sync(
                    Some("org.freedesktop.login1"),
                    "/org/freedesktop/login1",
                    "org.freedesktop.login1.Manager",
                    "Inhibit",
                    Some(&("sleep", "Camera", "Saving photos", "block").to_variant()),
                    Some(glib::VariantTy::new("(h)").unwrap()),
                    gtk::gio::DBusCallFlags::NONE,
                    -1,
                    None::<&gtk::gio::UnixFDList>,
                    None::<&gtk::gio::Cancellable>,
                )
            });
        match r {
            Ok((_, Some(fds))) => match fds.get(0) {
                Ok(fd) => *self.sleep_inhibitor.borrow_mut() = Some(fd),
                Err(e) => eprintln!("l16-camera: sleep inhibitor: {e}"),
            },
            Ok((_, None)) => eprintln!("l16-camera: sleep inhibitor: no fd"),
            Err(e) => eprintln!("l16-camera: sleep inhibitor: {e}"),
        }
    }

    // a feedbackd event (camera-shutter, camera-focus): the sound and vibration Phosh's
    // feedback profile gives it, or none in silent mode
    // a pulse of the vibration motor, at the haptics setting's strength
    fn buzz(&self, ms: u16) {
        let strength = [0, 30000, 60000][self.st.borrow().haptics as usize];
        self.motor.play(ms, strength);
    }

    fn feedback(&self, event: &str) {
        let Ok(bus) = gtk::gio::bus_get_sync(gtk::gio::BusType::Session, None::<&gtk::gio::Cancellable>)
        else {
            return;
        };
        let hints = glib::VariantDict::new(None).end();
        bus.call(
            Some("org.sigxcpu.Feedback"),
            "/org/sigxcpu/Feedback",
            "org.sigxcpu.Feedback",
            "TriggerEvent",
            Some(&("org.l16linux.Camera", event, hints, -1i32).to_variant()),
            None,
            gtk::gio::DBusCallFlags::NONE,
            -1,
            None::<&gtk::gio::Cancellable>,
            |_| {},
        );
    }

    fn saved(&self) {
        let left = {
            let mut st = self.st.borrow_mut();
            st.saving = st.saving.saturating_sub(1);
            st.saving
        };
        if left == 0 {
            self.thumb.set_opacity(1.0);
            // the photos are safe: the camera may sleep again
            self.sleep_inhibitor.borrow_mut().take();
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
                        let view = self.photo_args.borrow_mut().remove(&dir).unwrap_or_default();
                        thread::spawn(move || {
                            let mut raws: Vec<PathBuf> = (1..=3)
                                .map(|a| dir.join(format!("asic{a}.raw")))
                                .filter(|p| p.exists())
                                .collect();
                            raws.sort();
                            let r = App::run(Command::new("l16-lri-assemble").args(&view).arg(&out).args(&raws));
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
        if self.st.borrow().mode != Mode::Manual && !self.st.borrow().busy {
            let iso = self.metered[0].load(Ordering::Relaxed);
            let us = self.metered[1].load(Ordering::Relaxed);
            let mut st = self.st.borrow_mut();
            st.live_iso = iso;
            st.live_secs = us as f64 / 1e6;
        }
        self.continuous_focus();
        self.follow_screen();
        let (show, asleep) = {
            let st = self.st.borrow();
            (st.histogram, st.asleep)
        };
        if show && !asleep {
            self.update_histogram();
            self.marks.queue_draw();
        }
        self.refresh();
    }

    // the photo transfer streams, beside the preview (done: Stage::Transferred); started after it
    fn start_transfers(&self) {
        let (done_tx, done_rx) = mpsc::channel();
        match transfer::Transfers::start(done_tx) {
            Ok(t) => *self.transfers.borrow_mut() = Some(t),
            Err(e) => {
                self.status.set_text(&format!("no photo transfers: {e}"));
                self.status.set_visible(true);
            }
        }
        let tx = self.stage_tx.clone();
        thread::spawn(move || {
            while let Ok(r) = done_rx.recv() {
                if tx.send(Stage::Transferred(r)).is_err() {
                    break;
                }
            }
        });
    }

    // The preview (and the ASICs, which the driver powers down a while after) stops while the
    // screen is off, and starts again when it comes back. Not while a photo is on its way:
    // the ASICs hold it until it is transferred. As when the app closes and opens: the
    // transfer streams stop first and start after the preview (the preview cannot restart
    // under them).
    // the buttons, the touch strip and the photos' progress, every 15 ms (stopped while the
    // screen is off, so the app leaves the CPU alone)
    fn fast_loop(self: &Rc<Self>) {
        let a = self.clone();
        glib::timeout_add_local(Duration::from_millis(15), move || {
            if a.st.borrow().asleep {
                return glib::ControlFlow::Break;
            }
            while let Ok(ev) = a.input_rx.try_recv() {
                a.on_input(ev);
            }
            a.switched();
            while let Ok(stage) = a.stage_rx.try_recv() {
                a.on_stage(stage);
            }
            glib::ControlFlow::Continue
        });
    }

    fn follow_screen(self: &Rc<Self>) {
        let on = std::fs::read_to_string("/sys/class/drm/card0-DSI-1/dpms")
            .map_or(true, |s| s.trim() == "On");
        let (asleep, busy) = {
            let st = self.st.borrow();
            (st.asleep, st.busy || st.saving > 0 || st.counting)
        };
        if !on && !asleep && !busy {
            self.st.borrow_mut().asleep = true;
            if let Some(mut t) = self.transfers.borrow_mut().take() {
                t.stop();
            }
            self.stop_preview();
        } else if on && asleep {
            self.st.borrow_mut().asleep = false;
            let _ = self.pipeline.set_state(gst::State::Playing);
            self.apply_exposure();
            self.apply_wb();
            self.start_transfers();
            // presses while the screen was off are not for the camera
            while self.input_rx.try_recv().is_ok() {}
            self.fast_loop();
        }
    }

    // AF-D as stock's app runs it (there is no ASIC mode): the centre is focused again once the
    // scene has changed, judged by the metered exposure (2/3 EV) or the zoom
    fn continuous_focus(self: &Rc<Self>) {
        let (want, scene) = {
            let st = self.st.borrow();
            let settled = st.settle.is_none() && st.switching.is_none();
            let want = st.caf && st.mode != Mode::Manual && !st.busy && settled && st.live_iso > 0;
            (want, ((st.live_iso as f64 * st.live_secs).max(1e-9).ln(), st.zoom))
        };
        if !want || self.focusing.load(Ordering::SeqCst) {
            return;
        }
        let r = self.st.borrow().caf_ref;
        match r {
            None => self.st.borrow_mut().caf_ref = Some(scene),
            Some((ev, zoom)) => {
                if (scene.0 - ev).abs() > 0.46 || (scene.1 - zoom).abs() > 1.0 {
                    self.focus_run(None, false);
                }
            }
        }
    }

    fn on_input(self: &Rc<Self>, ev: input::Ev) {
        match ev {
            input::Ev::Key(input::KEY_CAMERA_FOCUS, true) => self.focus(None),
            input::Ev::Key(input::KEY_CAMERA | input::KEY_VOLUMEUP, true) => self.shutter_pressed(),
            input::Ev::Key(..) => {}
            // the strip's position comes before its touch-down in each report
            input::Ev::StripX(_) | input::Ev::StripTouch(_) if !self.st.borrow().strip_zoom => {}
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

    // the preview's brightness (Rec. 601 luma, 64 bins) from its current frame, sampled; with
    // the preview's digital gain, as shown
    fn update_histogram(&self) {
        // the frame as drawn, small: the sink's current image isn't always a plain texture,
        // and 160x120 is plenty for 64 bins
        let Some(renderer) = self.view.native().and_then(|n| n.renderer()) else { return };
        let (w, h) = (160.0f32, 120.0f32);
        let snap = gtk::Snapshot::new();
        self.paintable.snapshot(&snap, w as f64, h as f64);
        let Some(node) = snap.to_node() else { return };
        let tex = renderer.render_texture(&node, Some(&gtk::graphene::Rect::new(0.0, 0.0, w, h)));
        let (tw, th) = (tex.width() as usize, tex.height() as usize);
        let mut buf = vec![0u8; tw * th * 4];
        tex.download(&mut buf, tw * 4);
        let gain = self.view.gain();
        let mut bins = vec![0u32; 64];
        for p in buf.chunks_exact(4) {
            // GDK's download format is B8G8R8A8 (premultiplied; the preview is opaque)
            let luma = (p[2] as f64 * 0.299 + p[1] as f64 * 0.587 + p[0] as f64 * 0.114) * gain;
            bins[((luma / 256.0 * 64.0) as usize).min(63)] += 1;
        }
        *self.hist.borrow_mut() = bins;
    }

    fn draw_histogram(&self, cr: &cairo::Context) {
        let bins = self.hist.borrow();
        let max = bins.iter().copied().max().unwrap_or(0).max(1) as f64;
        let (x0, y0, bw, bh) = (16.0, 16.0, 192.0, 72.0);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.45);
        cr.rectangle(x0, y0, bw, bh);
        let _ = cr.fill();
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.85);
        let step = bw / bins.len() as f64;
        for (i, &n) in bins.iter().enumerate() {
            let hgt = (n as f64 / max).sqrt() * (bh - 4.0);
            cr.rectangle(x0 + i as f64 * step, y0 + bh - hgt, step, hgt);
        }
        let _ = cr.fill();
    }

    fn draw_marks(&self, cr: &cairo::Context, w: f64, h: f64) {
        let st = self.st.borrow();
        if st.grid > 0 {
            // thirds, or the golden ratio's lines (0.382 and 0.618 of the way across)
            let at = if st.grid == 1 { [1.0 / 3.0, 2.0 / 3.0] } else { [0.382, 0.618] };
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.4);
            cr.set_line_width(1.0);
            for f in at {
                let x = (w * f).round() + 0.5;
                let y = (h * f).round() + 0.5;
                cr.move_to(x, 0.0);
                cr.line_to(x, h);
                cr.move_to(0.0, y);
                cr.line_to(w, y);
            }
            let _ = cr.stroke();
        }
        if st.histogram {
            self.draw_histogram(cr);
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
                Dial::Ev => (st.ev, fmt_ev(ev_at(st.ev)), (-9..=9).map(|e| (ev_pos(e), fmt_ev(e))).collect()),
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
        // auto has no top dial
        let (top_dial, bottom_dial) = st.mode.dials();
        let Some(d) = (if top { top_dial } else { Some(bottom_dial) }) else { return };
        let label = match d {
            Dial::Iso => "ISO",
            Dial::Shutter => "S",
            Dial::Ev => "EV",
        };
        let (enabled, dial) = (true, Some(d));
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
    let hud_box = gtk::Box::new(gtk::Orientation::Vertical, 16);
    for (l, unit) in hud.iter().zip(["ev", "iso", "s", "mm"]) {
        hud_box.append(&hud_item(l, unit));
    }
    left.append(&hud_box);

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

    // the toolbar: flash, timer, grid, burst, and the settings screen (the mode wheel beside it)
    let timer_btn = gtk::Button::with_label("timer off");
    let grid_btn = gtk::Button::with_label("grid off");
    let hist_btn = gtk::Button::with_label("histogram off");
    let burst_btn = gtk::Button::with_label("burst off");
    let flash_btn = gtk::Button::with_label("flash off");
    let wb_btn = gtk::Button::with_label("wb auto");
    let settings_btn = gtk::Button::with_label("settings");
    let close_btn = gtk::Button::with_label("✕");
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    bar.add_css_class("toolbar");
    bar.append(&flash_btn);
    bar.append(&wb_btn);
    bar.append(&timer_btn);
    bar.append(&grid_btn);
    bar.append(&hist_btn);
    bar.append(&burst_btn);
    let fill = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    fill.set_hexpand(true);
    bar.append(&fill);
    bar.append(&settings_btn);
    bar.append(&close_btn);

    // the settings screen (OpenLight's: a list of title, explanation and value)
    let settings_list = gtk::ListBox::new();
    settings_list.set_selection_mode(gtk::SelectionMode::None);
    let settings_scroll = gtk::ScrolledWindow::new();
    settings_scroll.set_child(Some(&settings_list));
    settings_scroll.set_vexpand(true);
    settings_scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    let settings_back = gtk::Button::with_label("‹  settings");
    settings_back.add_css_class("flat-white");
    settings_back.set_halign(gtk::Align::Start);
    settings_back.set_margin_start(16);
    let settings_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    settings_box.add_css_class("settings");
    settings_box.append(&settings_back);
    settings_box.append(&settings_scroll);
    let settings_page = gtk::Overlay::new();
    settings_page.set_child(Some(&settings_box));
    settings_page.set_visible(false);
    // the list a setting's value is chosen from, over the settings screen
    let chooser_title = gtk::Label::new(None);
    chooser_title.add_css_class("chooser-title");
    chooser_title.set_halign(gtk::Align::Start);
    let chooser_list = gtk::ListBox::new();
    chooser_list.set_selection_mode(gtk::SelectionMode::None);
    let chooser_card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    chooser_card.add_css_class("chooser-card");
    chooser_card.set_halign(gtk::Align::Center);
    chooser_card.set_valign(gtk::Align::Center);
    chooser_card.set_size_request(420, -1);
    chooser_card.set_overflow(gtk::Overflow::Hidden);
    chooser_card.append(&chooser_title);
    chooser_card.append(&chooser_list);
    let chooser = gtk::Box::new(gtk::Orientation::Vertical, 0);
    chooser.add_css_class("chooser");
    chooser.append(&chooser_card);
    chooser_card.set_vexpand(true);
    chooser.set_visible(false);
    settings_page.add_overlay(&chooser);
    let toolbar = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideUp)
        .child(&bar)
        .valign(gtk::Align::End)
        .build();
    // the mode wheel: drawn down the whole right edge, but touched only around its labels, so
    // taps elsewhere (the toolbar) pass through
    let mode_wheel = gtk::DrawingArea::new();
    mode_wheel.set_halign(gtk::Align::End);
    mode_wheel.set_size_request(386, -1); // stock's 225 dp
    mode_wheel.set_can_target(false);
    mode_wheel.set_visible(false);
    let mode_touch = gtk::Box::new(gtk::Orientation::Vertical, 0);
    mode_touch.set_halign(gtk::Align::End);
    mode_touch.set_valign(gtk::Align::Center);
    mode_touch.set_size_request(386, MODE_TOUCH_H);
    mode_touch.set_visible(false);

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
    root.add_overlay(&mode_wheel);
    root.add_overlay(&mode_touch);
    root.add_overlay(&burst_screen);
    root.add_overlay(&settings_page);
    window.set_child(Some(&root));

    let (stage_tx, stage_rx) = mpsc::channel();
    // control writes that can wait for the driver (an AF run holds it for seconds)
    let (ctl_tx, ctl_rx) = mpsc::channel::<(u32, i32)>();
    thread::spawn(move || {
        let c = ccb::Ccb::open();
        while let Ok((id, v)) = ctl_rx.recv() {
            if let Some(c) = &c {
                c.set(id, v);
            }
        }
    });
    let (input_tx, input_rx) = mpsc::channel();
    input::spawn(input_tx);
    // the metered exposure, off the UI thread (reading a control waits for the driver, which
    // a focus run holds for seconds: the UI stalled and drags were dropped)
    let metered: Arc<[std::sync::atomic::AtomicI32; 2]> = Arc::new(Default::default());
    let m = metered.clone();
    thread::spawn(move || {
        let Some(c) = ccb::Ccb::open() else { return };
        loop {
            m[0].store(c.get(ccb::AE_ISO).unwrap_or(0), Ordering::Relaxed);
            m[1].store(c.get(ccb::AE_EXPOSURE_US).unwrap_or(0), Ordering::Relaxed);
            thread::sleep(Duration::from_millis(300));
        }
    });
    let app = Rc::new(App {
        st: RefCell::new(State {
            mode: Mode::Auto,
            mode_pos: 0.0,
            mode_start: 0.0,
            mode_swiped: false,
            iso: 1.0,
            ev: 0.5,
            shutter: secs_pos(1.0 / 60.0),
            zoom: ZOOM_MIN,
            module: 0,
            timer: 0,
            grid: 0,
            histogram: false,
            busy: false,
            counting: false,
            saving: 0,
            seq: 0,
            burst_count: 0,
            burst_captured: false,
            burst: 0,
            flash: 0,
            wb: 0,
            dragged: false,
            wheel: None,
            wheel_start: 0.0,
            wheel_close: None,
            haptics: 1,
            zoom_start: ZOOM_MIN,
            zoom_wheel_until: None,
            focus_until: None,
            focus_at: None,
            zoom_sent: Instant::now(),
            metering: 1, // stock's default: touch-weighted
            caf: true,
            caf_ref: None,
            stacked: true,
            exposure_info: true,
            inverse_wheel: false,
            strip_zoom: true,
            asleep: false,
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
        input_rx,
        ctl_tx,
        metered: metered.clone(),
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
        right,
        mode_wheel,
        mode_touch,
        timer_btn,
        grid_btn,
        hist_btn,
        hist: RefCell::new(vec![0; 64]),
        burst_btn,
        flash_btn,
        wb_btn,
        cal: wb::Calibration::load(),
        motor: haptics::Haptics::open(),
        photo_args: RefCell::new(HashMap::new()),
        hud_box,
        settings_page,
        last_saved: RefCell::new(String::new()),
        sleep_inhibitor: RefCell::new(None),
        status,
        countdown,
    });
    {
        let mut st = app.st.borrow_mut();
        st.load(&settings::load());
        *app.last_saved.borrow_mut() = st.saved();
    }
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
            a.show_toolbar(false);
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

    // the dials: tap and drag (which dial is which depends on the mode)
    for (widget, top) in [(app.top.clone(), true), (app.bottom.clone(), false)] {
        let drag = gtk::GestureDrag::new();
        let a = app.clone();
        drag.connect_drag_begin(move |_, _, _| {
            let mut st = a.st.borrow_mut();
            let (top_dial, bottom_dial) = st.mode.dials();
            let Some(dial) = (if top { top_dial } else { Some(bottom_dial) }) else { return };
            if let Some(id) = st.wheel_close.take() {
                id.remove();
            }
            st.wheel = Some(dial);
            st.wheel_start = match dial {
                Dial::Iso => st.iso,
                Dial::Shutter => st.shutter,
                Dial::Ev => st.ev,
            };
            drop(st);
            a.buzz(15);
            a.refresh();
            a.wheels.queue_draw();
        });
        let a = app.clone();
        drag.connect_drag_update(move |_, _, dy| {
            let (dial, start, dir) = {
                let st = a.st.borrow();
                (st.wheel, st.wheel_start, if st.inverse_wheel { -1.0 } else { 1.0 })
            };
            if let Some(dial) = dial {
                a.set_dial(dial, start + dir * dy * 0.001);
            }
        });
        let a = app.clone();
        drag.connect_drag_end(move |_, _, _| {
            if a.st.borrow().wheel.is_none() {
                return;
            }
            a.buzz(10);
            let b = a.clone();
            let id = glib::timeout_add_local_once(Duration::from_millis(600), move || {
                let mut st = b.st.borrow_mut();
                st.wheel_close = None;
                st.wheel = None;
                drop(st);
                b.refresh();
                b.wheels.queue_draw();
            });
            if let Some(old) = a.st.borrow_mut().wheel_close.replace(id) {
                old.remove();
            }
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
        a.show_toolbar(!open);
    });
    let a = app.clone();
    close_btn.connect_clicked(move |_| a.show_toolbar(false));
    // the mode wheel: drag it up and down (a mode per ~70 px, as stock's 0.002 of its pixels), or
    // tap a mode; it settles on the mode when let go
    let a = app.clone();
    app.mode_wheel.set_draw_func(move |_, cr, w, h| a.draw_mode_wheel(cr, w as f64, h as f64));
    let drag = gtk::GestureDrag::new();
    let a = app.clone();
    drag.connect_drag_begin(move |_, _, _| {
        let mut st = a.st.borrow_mut();
        st.mode_start = st.mode.index() as f64 / (MODES.len() - 1) as f64;
        st.mode_swiped = false;
    });
    let a = app.clone();
    drag.connect_drag_update(move |_, _, dy| {
        if dy.abs() > 8.0 {
            a.st.borrow_mut().mode_swiped = true;
        }
        let start = a.st.borrow().mode_start;
        a.set_mode_pos(start + dy * 0.0035, true);
    });
    let a = app.clone();
    drag.connect_drag_end(move |_, _, _| {
        // settle on the chosen mode (already applied as the wheel passed it)
        let pos = a.st.borrow().mode.index() as f64 / (MODES.len() - 1) as f64;
        a.set_mode_pos(pos, false);
    });
    app.mode_touch.add_controller(drag);
    let click = gtk::GestureClick::new();
    let a = app.clone();
    click.connect_released(move |_, _, _, y| {
        // the end of a swipe is not a tap on the label under the finger
        if a.st.borrow().mode_swiped {
            return;
        }
        let (pos, h) = (a.st.borrow().mode_pos, a.mode_wheel.height() as f64);
        // the touch band sits centred on the wheel
        let y = y + (h - MODE_TOUCH_H as f64) / 2.0;
        let near = (0..MODES.len())
            .map(|i| (i, (App::mode_item_y(pos, i, h) - y).abs()))
            .min_by(|p, q| p.1.total_cmp(&q.1));
        if let Some((i, d)) = near {
            if d < 30.0 {
                a.set_mode_pos(i as f64 / (MODES.len() - 1) as f64, true);
            }
        }
    });
    app.mode_touch.add_controller(click);
    let a = app.clone();
    app.wb_btn.connect_clicked(move |_| {
        {
            let mut st = a.st.borrow_mut();
            st.wb = (st.wb + 1) % wb::PRESETS.len();
        }
        a.apply_wb();
        a.refresh();
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
        let f = {
            let mut st = a.st.borrow_mut();
            st.flash = (st.flash + 1) % 3;
            st.flash
        };
        let _ = a.ctl_tx.send((ccb::FLASH, f as i32));
        a.refresh();
    });
    let a = app.clone();
    let ch = chooser.clone();
    settings_btn.connect_clicked(move |_| {
        a.show_toolbar(false);
        ch.set_visible(false);
        a.settings_page.set_visible(true);
    });
    let a = app.clone();
    settings_back.connect_clicked(move |_| a.settings_page.set_visible(false));
    // after a setting changes: the driver's side of it, the screen, the settings file
    let changed = {
        let a = app.clone();
        move || {
            let meter = a.st.borrow().metering;
            let _ = a.ctl_tx.send((ccb::METERING, meter as i32));
            a.refresh();
        }
    };
    // the chooser: which setting it is choosing for, and that row's value label
    let choosing: Rc<RefCell<Option<(usize, gtk::Label)>>> = Rc::new(RefCell::new(None));
    // a row's tap: a switch flips, a choice opens the chooser
    let mut on_tap: Vec<Box<dyn Fn()>> = Vec::new();
    for (i, row) in SETTINGS.iter().enumerate() {
        let title = gtk::Label::new(Some(row.title));
        title.add_css_class("set-title");
        title.set_halign(gtk::Align::Start);
        let sub = gtk::Label::new(Some(row.sub));
        sub.add_css_class("set-sub");
        sub.set_halign(gtk::Align::Start);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text.append(&title);
        text.append(&sub);
        text.set_hexpand(true);
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        line.append(&text);
        match row.kind {
            SettingKind::Switch(get, set) => {
                let sw = gtk::Switch::new();
                sw.set_active(get(&app.st.borrow()));
                sw.set_valign(gtk::Align::Center);
                sw.set_can_target(false); // the row's tap flips it
                let a = app.clone();
                let changed = changed.clone();
                sw.connect_active_notify(move |sw| {
                    set(&mut a.st.borrow_mut(), sw.is_active());
                    changed();
                });
                line.append(&sw);
                on_tap.push(Box::new(move || sw.set_active(!sw.is_active())));
            }
            SettingKind::Choice(options, get, _) => {
                let value = gtk::Label::new(Some(options[get(&app.st.borrow())]));
                value.add_css_class("set-value");
                let chevron = gtk::Label::new(Some("›"));
                chevron.add_css_class("set-chevron");
                line.append(&value);
                line.append(&chevron);
                let a = app.clone();
                let choosing = choosing.clone();
                let (chooser, chooser_title, chooser_list) =
                    (chooser.clone(), chooser_title.clone(), chooser_list.clone());
                on_tap.push(Box::new(move || {
                    let now = get(&a.st.borrow());
                    chooser_title.set_text(row.title);
                    while let Some(c) = chooser_list.first_child() {
                        chooser_list.remove(&c);
                    }
                    for (k, o) in options.iter().enumerate() {
                        let l = gtk::Label::new(Some(o));
                        l.add_css_class("set-title");
                        l.set_halign(gtk::Align::Start);
                        l.set_hexpand(true);
                        let check = gtk::Label::new(Some(if k == now { "✓" } else { "" }));
                        check.add_css_class("chooser-check");
                        let b = gtk::Box::new(gtk::Orientation::Horizontal, 16);
                        b.append(&l);
                        b.append(&check);
                        chooser_list.append(&b);
                    }
                    *choosing.borrow_mut() = Some((i, value.clone()));
                    chooser.set_visible(true);
                }));
            }
        }
        settings_list.append(&line);
    }
    settings_list.connect_row_activated(move |_, r| on_tap[r.index() as usize]());
    // a choice made: set it, show it on its row, close the chooser
    let a = app.clone();
    let (ch, ch_choosing) = (chooser.clone(), choosing.clone());
    chooser_list.connect_row_activated(move |_, r| {
        if let Some((i, value)) = ch_choosing.borrow_mut().take() {
            if let SettingKind::Choice(options, _, set) = SETTINGS[i].kind {
                let k = r.index() as usize;
                set(&mut a.st.borrow_mut(), k);
                value.set_text(options[k]);
                changed();
            }
        }
        ch.set_visible(false);
    });
    // a tap beside the list: nothing chosen
    let backdrop = gtk::GestureClick::new();
    let (ch, card) = (chooser.clone(), chooser_card.clone());
    backdrop.connect_released(move |_, _, x, y| {
        let inside = card.compute_bounds(&ch).is_some_and(|b| {
            b.contains_point(&gtk::graphene::Point::new(x as f32, y as f32))
        });
        if !inside {
            choosing.borrow_mut().take();
            ch.set_visible(false);
        }
    });
    chooser.add_controller(backdrop);
    let a = app.clone();
    app.burst_btn.connect_clicked(move |_| {
        {
            let mut st = a.st.borrow_mut();
            st.burst = (st.burst + 1) % BURSTS.len();
        }
        a.refresh();
    });
    let a = app.clone();
    app.hist_btn.connect_clicked(move |_| {
        {
            let mut st = a.st.borrow_mut();
            st.histogram = !st.histogram;
        }
        a.refresh();
    });
    let a = app.clone();
    app.grid_btn.connect_clicked(move |_| {
        {
            let mut st = a.st.borrow_mut();
            st.grid = (st.grid + 1) % 3;
        }
        a.refresh();
    });

    // hardware: shutter button, touch strip
    let a = app.clone();
    a.fast_loop();
    let a = app.clone();
    glib::timeout_add_local(Duration::from_millis(300), move || {
        a.poll();
        glib::ControlFlow::Continue
    });

    let a = app.clone();
    // closing: the window goes at once (the shell's close animation doesn't wait for the
    // camera), then the transfer streams and the preview stop in their order and the app ends
    window.connect_close_request(move |w| {
        w.set_visible(false);
        let (a, w) = (a.clone(), w.clone());
        glib::idle_add_local_once(move || {
            let t = Instant::now();
            if let Some(t) = a.transfers.borrow_mut().as_mut() {
                t.stop();
            }
            a.stop_preview();
            eprintln!("l16-camera: closed in {:.2} s", t.elapsed().as_secs_f64());
            // quit outright: started from the app grid, the application is registered on the
            // session bus and stayed running (hidden) once its window was gone
            let gapp = w.application();
            w.destroy();
            if let Some(gapp) = gapp {
                gapp.quit();
            }
        });
        glib::Propagation::Stop
    });
    // a kill (TERM, INT, HUP) closes the window as the user would, so the transfer streams
    // and the preview stop in their order (killed under a running preview, the streams leave
    // CAMSS unable to start it again until a reboot)
    for sig in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        let w = window.clone();
        glib::unix_signal_add_local(sig, move || {
            w.close();
            glib::ControlFlow::Break
        });
    }

    if let Some(c) = &app.ccb {
        let st = app.st.borrow();
        c.set(ccb::MODULE, 0);
        c.set(ccb::FLASH, st.flash as i32);
        c.set(ccb::METERING, st.metering as i32);
        c.set(ccb::ZOOM, 1000);
    }
    app.start_preview();
    app.apply_exposure();
    app.apply_wb();
    app.start_transfers();
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
