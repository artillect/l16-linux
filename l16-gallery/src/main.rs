// l16-gallery: the L16's photos (LRI files, ~/Pictures/L16). A grid of them, newest first,
// with the desktop's own thumbnails (made by glycin-thumbnailer through the glycin-lri
// loader); tap one for a quick look (the reference module's frame, about a second). As in
// stock, the full picture is only made when asked: "process" runs Light's own renderer
// (l16-render: the stock gallery's libcp) on it, and the result is kept and shown from then
// on. As stock's, that's a JPEG beside the LRI, same name: the photo as every other app
// sees it (the LRI stays the untouched original, to process again).

#[path = "../../l16-camera/src/icons.rs"]
mod icons;
#[path = "../../glycin-lri/src/lri.rs"]
mod lri;
mod places;

use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use md5::{Digest, Md5};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const CSS: &str = "
window.gallery, .gallery-page { background: #000; }
.bar button { background: none; border: none; box-shadow: none; color: #fff;
    font-size: 17px; font-weight: 600; min-height: 48px; min-width: 56px; }
.bar { background: rgba(0,0,0,0.6); }
.bar button:checked { background: none; color: #00B1ED; }
.bar label { color: #fff; font-size: 17px; font-weight: 600; margin-left: 4px; }
.info { background: rgba(0,0,0,0.75); border-radius: 10px; padding: 12px 16px; }
.info label { color: #fff; font-size: 15px; }
.info .key { color: rgba(255,255,255,0.6); }
.day { color: #fff; font-size: 17px; font-weight: 600; margin: 16px 12px 8px 12px; }
flowbox { padding: 0 2px; }
flowboxchild { padding: 2px; }
.status { color: #fff; font-size: 15px; background: rgba(0,0,0,0.55);
    border-radius: 8px; padding: 4px 12px; }
.empty { color: rgba(255,255,255,0.6); font-size: 18px; }
.bar menubutton > button { background: none; border: none; box-shadow: none; color: #fff;
    font-size: 17px; min-height: 48px; min-width: 56px; }
popover.lightbox-menu > contents { background: #1c1c1c; padding: 4px 0; }
popover.lightbox-menu button { background: none; border: none; box-shadow: none; color: #fff;
    font-size: 16px; padding: 10px 20px; }
popover.lightbox-menu button:disabled { color: rgba(255,255,255,0.35); }
.check { color: #00B1ED; font-size: 26px; text-shadow: 0 0 3px rgba(0,0,0,0.8); }
";

fn photos_dir() -> PathBuf {
    glib::user_special_dir(glib::UserDirectory::Pictures)
        .unwrap_or_else(|| glib::home_dir().join("Pictures"))
        .join("L16")
}

// the LRIs, newest first
fn scan() -> Vec<PathBuf> {
    let mut v: Vec<(i64, PathBuf)> = std::fs::read_dir(photos_dir())
        .map(|d| {
            d.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("lri")))
                .map(|p| (taken(&p).map_or(0, |t| t.to_unix()), p))
                .collect()
        })
        .unwrap_or_default();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.into_iter().map(|(_, p)| p).collect()
}

thread_local! {
    static TAKEN: RefCell<HashMap<PathBuf, Option<glib::DateTime>>> = RefCell::new(HashMap::new());
}

// when a photo was taken, in the time where it was taken: the LRI's own time (kept, it
// doesn't change)
fn taken(p: &Path) -> Option<glib::DateTime> {
    if let Some(t) = TAKEN.with(|c| c.borrow().get(p).cloned()) {
        return t;
    }
    let t = std::fs::File::open(p)
        .ok()
        .and_then(|f| lri::taken(&mut std::io::BufReader::new(f)).ok().flatten())
        .and_then(|t| {
            let z = glib::TimeZone::from_offset(t.utc_offset);
            glib::DateTime::new(&z, t.year, t.month, t.day, t.hour, t.minute, t.second as f64).ok()
        });
    // a file not there (yet) isn't kept
    if t.is_some() {
        TAKEN.with(|c| c.borrow_mut().insert(p.to_path_buf(), t.clone()));
    }
    t
}

// the freedesktop thumbnail (large, 256 px) of a file: ~/.cache/thumbnails/large/md5(uri).png
fn thumb_path(p: &Path) -> PathBuf {
    let uri = gio::File::for_path(p).uri();
    let hash: String = Md5::digest(uri.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
    glib::user_cache_dir().join("thumbnails/large").join(format!("{hash}.png"))
}

// the processed picture: the JPEG beside the LRI
fn render_path(p: &Path) -> PathBuf {
    p.with_extension("jpg")
}

fn stamp(p: &Path) -> String {
    taken(p)
        .and_then(|d| d.format("%-d %B %Y, %H:%M").ok())
        .map(|s| s.to_string())
        .unwrap_or_default()
}

// the grid's heading for the day a photo was taken
fn day_of(p: &Path) -> String {
    let (Some(t), Ok(now)) = (taken(p), glib::DateTime::now_local()) else { return String::new() };
    let ymd = |d: &glib::DateTime| (d.year(), d.day_of_year());
    let fmt = if ymd(&t) == ymd(&now) {
        return "Today".into();
    } else if now.add_days(-1).is_ok_and(|y| ymd(&y) == ymd(&t)) {
        return "Yesterday".into();
    } else if t.year() == now.year() {
        "%A %-d %B"
    } else {
        "%-d %B %Y"
    };
    t.format(fmt).map(|s| s.to_string()).unwrap_or_default()
}

fn shutter(ns: u64) -> String {
    let s = ns as f64 / 1e9;
    if s >= 0.95 {
        format!("{} s", (s * 10.0).round() / 10.0)
    } else {
        format!("1/{} s", (1.0 / s).round())
    }
}

fn megabytes(p: &Path) -> Option<String> {
    Some(format!("{:.1} MB", p.metadata().ok()?.len() as f64 / 1e6))
}

// another of the camera's apps, as the desktop launches it (so it's brought up, even when
// it's running already)
fn launch(desktop: &str, file: Option<&Path>) {
    let Some(app) = gio::DesktopAppInfo::new(desktop) else { return };
    let files: Vec<gio::File> = file.map(gio::File::for_path).into_iter().collect();
    let ctx = gdk::Display::default().map(|d| d.app_launch_context());
    if let Err(e) = app.launch(&files, ctx.as_ref()) {
        eprintln!("launching {desktop}: {e}");
    }
}

enum Done {
    Thumb(PathBuf),
    Quick(PathBuf, Result<lri::Picture, String>),
    Render(PathBuf, Result<PathBuf, String>),
}

struct Gallery {
    photos: RefCell<Vec<PathBuf>>,
    days: gtk::Box,
    thumbs: RefCell<HashMap<PathBuf, gtk::Picture>>,
    // choosing photos in the grid (a long press starts it): the chosen, their marks, and the
    // bar's buttons for it
    selecting: RefCell<bool>,
    selected: RefCell<HashSet<PathBuf>>,
    checks: RefCell<HashMap<PathBuf, gtk::Label>>,
    grid_camera: gtk::Button,
    sel_start: gtk::Button,
    sel_cancel: gtk::Button,
    sel_label: gtk::Label,
    sel_delete: gtk::Button,
    // the grid's cells: width, height, how many a row (fit_grid)
    cell: RefCell<(i32, i32, u32)>,
    stack: gtk::Stack,
    empty: gtk::Label,
    picture: gtk::Picture,
    scroller: gtk::ScrolledWindow,
    status: gtk::Label,
    title: gtk::Label,
    process: gtk::Button,
    info_box: gtk::Grid,
    info: RefCell<Option<(PathBuf, lri::Info)>>,
    processing: RefCell<HashSet<PathBuf>>,
    current: RefCell<Option<usize>>,
    zoom: RefCell<f64>,
    zoom_start: RefCell<f64>,
    // a pinch: where it started (in the picture, at zoom_start) and where the view is going
    pinch_at: RefCell<(f64, f64)>,
    scroll_to: RefCell<Option<(f64, f64)>>,
    thumb_tx: mpsc::Sender<PathBuf>,
    render_tx: mpsc::Sender<PathBuf>,
    done_tx: mpsc::Sender<Done>,
    window: gtk::ApplicationWindow,
}

impl Gallery {
    // the grid: the photos by day, newest first
    fn refresh(self: &Rc<Self>) {
        let photos = scan();
        if *self.photos.borrow() == photos && self.days.first_child().is_some() {
            return;
        }
        while let Some(c) = self.days.first_child() {
            self.days.remove(&c);
        }
        self.thumbs.borrow_mut().clear();
        self.checks.borrow_mut().clear();
        self.selected.borrow_mut().retain(|p| photos.contains(p));
        self.empty.set_visible(photos.is_empty());
        let mut flow: Option<gtk::FlowBox> = None;
        let mut day = String::new();
        for (i, p) in photos.iter().enumerate() {
            let d = day_of(p);
            if flow.is_none() || d != day {
                let head = gtk::Label::new(Some(&d));
                head.add_css_class("day");
                head.set_xalign(0.0);
                self.days.append(&head);
                let f = gtk::FlowBox::new();
                f.set_selection_mode(gtk::SelectionMode::None);
                f.set_homogeneous(true);
                let n = self.cell.borrow().2;
                f.set_min_children_per_line(n);
                f.set_max_children_per_line(n);
                f.set_activate_on_single_click(true);
                let a = self.clone();
                f.connect_child_activated(move |_, c| {
                    if let Ok(i) = c.widget_name().parse::<usize>() {
                        if *a.selecting.borrow() {
                            a.toggle_selected(i);
                        } else {
                            a.open(i);
                        }
                    }
                });
                self.days.append(&f);
                flow = Some(f);
                day = d;
            }
            let pic = gtk::Picture::new();
            // whole in its cell (4:3, as a landscape photo): a portrait one isn't cropped
            pic.set_content_fit(gtk::ContentFit::Contain);
            pic.set_size_request(-1, self.cell.borrow().1);
            self.bind_thumb(&pic, p);
            let chosen = self.selected.borrow().contains(p);
            pic.set_opacity(if chosen { 0.6 } else { 1.0 });
            let check = icons::label(if chosen { icons::CHECK_CIRCLE } else { icons::CIRCLE_OUTLINE });
            check.add_css_class("check");
            check.set_halign(gtk::Align::End);
            check.set_valign(gtk::Align::Start);
            check.set_margin_top(6);
            check.set_margin_end(6);
            check.set_can_target(false);
            check.set_visible(*self.selecting.borrow());
            let cell = gtk::Overlay::new();
            cell.set_child(Some(&pic));
            cell.add_overlay(&check);
            let child = gtk::FlowBoxChild::new();
            child.set_child(Some(&cell));
            child.set_widget_name(&i.to_string());
            // a long press: start choosing, with this one (claimed, so it isn't a tap too)
            let press = gtk::GestureLongPress::new();
            let a = self.clone();
            press.connect_pressed(move |g, _, _| {
                g.set_state(gtk::EventSequenceState::Claimed);
                if !*a.selecting.borrow() {
                    a.set_selecting(true);
                }
                a.toggle_selected(i);
            });
            child.add_controller(press);
            flow.as_ref().unwrap().append(&child);
            self.thumbs.borrow_mut().insert(p.clone(), pic);
            self.checks.borrow_mut().insert(p.clone(), check);
        }
        *self.photos.borrow_mut() = photos;
    }

    // choosing photos: on or off (off clears the choice), and one more or less
    fn set_selecting(&self, on: bool) {
        *self.selecting.borrow_mut() = on;
        if !on {
            for p in self.selected.borrow_mut().drain() {
                if let Some(pic) = self.thumbs.borrow().get(&p) {
                    pic.set_opacity(1.0);
                }
            }
        }
        // every photo's box while choosing (an empty one, or ticked), none after
        for c in self.checks.borrow().values() {
            c.set_markup(&icons::markup(icons::CIRCLE_OUTLINE, ""));
            c.set_visible(on);
        }
        self.selection_bar();
    }

    fn toggle_selected(&self, i: usize) {
        let Some(p) = self.photos.borrow().get(i).cloned() else { return };
        let on = {
            let mut s = self.selected.borrow_mut();
            if s.remove(&p) {
                false
            } else {
                s.insert(p.clone());
                true
            }
        };
        if let Some(c) = self.checks.borrow().get(&p) {
            c.set_markup(&icons::markup(if on { icons::CHECK_CIRCLE } else { icons::CIRCLE_OUTLINE }, ""));
        }
        if let Some(pic) = self.thumbs.borrow().get(&p) {
            pic.set_opacity(if on { 0.6 } else { 1.0 });
        }
        self.selection_bar();
    }

    // the grid's bar: the camera button, or (choosing) cancel, how many, delete
    fn selection_bar(&self) {
        let on = *self.selecting.borrow();
        let n = self.selected.borrow().len();
        self.grid_camera.set_visible(!on);
        self.sel_start.set_visible(!on);
        self.sel_cancel.set_visible(on);
        self.sel_label.set_visible(on);
        self.sel_delete.set_visible(on);
        self.sel_delete.set_sensitive(n > 0);
        self.sel_label.set_text(&format!("{n} selected"));
    }

    fn delete_selected(self: &Rc<Self>) {
        let chosen: Vec<PathBuf> = self.selected.borrow().iter().cloned().collect();
        if chosen.is_empty() {
            return;
        }
        let what = if chosen.len() == 1 { "this photo".to_string() } else { format!("these {} photos", chosen.len()) };
        let dialog = gtk::AlertDialog::builder()
            .message(format!("Delete {what}?"))
            .detail("The LRIs go, with their processed JPEGs. This can't be undone.")
            .buttons(["Cancel", "Delete"])
            .cancel_button(0)
            .default_button(0)
            .build();
        let s = self.clone();
        dialog.choose(Some(&self.window), None::<&gio::Cancellable>, move |r| {
            if r != Ok(1) {
                return;
            }
            for path in &chosen {
                let _ = std::fs::remove_file(path);
                let _ = std::fs::remove_file(render_path(path));
                let _ = std::fs::remove_file(thumb_path(path));
            }
            s.set_selecting(false);
            s.refresh();
        });
    }

    // the viewer's menu: the LRI or the processed JPEG (render) on the clipboard, as a file
    // (to paste in the file manager); the JPEG as an image too
    fn copy_file(&self, render: bool) {
        let Some(lri) = self.showing() else { return };
        let path = if render { render_path(&lri) } else { lri };
        if !path.exists() {
            return;
        }
        let files = gdk::FileList::from_array(&[gio::File::for_path(&path)]);
        let mut providers = vec![gdk::ContentProvider::for_value(&files.to_value())];
        if render {
            if let Ok(t) = gdk::Texture::from_filename(&path) {
                providers.push(gdk::ContentProvider::for_value(&t.to_value()));
            }
        }
        let content = gdk::ContentProvider::new_union(&providers);
        if self.window.clipboard().set_content(Some(&content)).is_ok() {
            self.flash_status("copied");
        }
    }

    // and the LRI or the render in its folder, in the file manager
    fn show_in_folder(&self, render: bool) {
        let Some(lri) = self.showing() else { return };
        let target = if render { render_path(&lri) } else { lri };
        if !target.exists() {
            return;
        }
        let uri = gio::File::for_path(&target).uri().to_string();
        let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>) else { return };
        bus.call(
            Some("org.freedesktop.FileManager1"),
            "/org/freedesktop/FileManager1",
            "org.freedesktop.FileManager1",
            "ShowItems",
            Some(&(vec![uri], "").to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            -1,
            None::<&gio::Cancellable>,
            |r| {
                if let Err(e) = r {
                    eprintln!("l16-gallery: show in folder: {e}");
                }
            },
        );
    }

    // a word in the viewer's status for a moment
    fn flash_status(&self, msg: &str) {
        self.status.set_text(msg);
        self.status.set_visible(true);
        let (status, msg) = (self.status.clone(), msg.to_string());
        glib::timeout_add_local_once(Duration::from_secs(2), move || {
            if status.text() == msg.as_str() {
                status.set_visible(false);
            }
        });
    }

    // the grid's cells for its width: 4 a row in landscape, 3 in portrait, 4:3 each (a fixed
    // 248 px made a portrait window's grid wider than the screen, the bar with it)
    fn fit_grid(&self, width: f64) {
        if width < 1.0 {
            return;
        }
        let n = if width >= 900.0 { 4 } else { 3 };
        // each cell's padding and spacing, about 8 px
        let w = ((width - 8.0 * n as f64) / n as f64).floor() as i32;
        let h = w * 3 / 4;
        if *self.cell.borrow() == (w, h, n) {
            return;
        }
        *self.cell.borrow_mut() = (w, h, n);
        // the height only: the row shares its width out, and a width here held a rotated
        // window at its old width (the grid never saw the new one)
        for pic in self.thumbs.borrow().values() {
            pic.set_size_request(-1, h);
        }
        let mut c = self.days.first_child();
        while let Some(w) = c {
            if let Some(f) = w.downcast_ref::<gtk::FlowBox>() {
                f.set_min_children_per_line(n);
                f.set_max_children_per_line(n);
            }
            c = w.next_sibling();
        }
    }

    // the thumbnail when it's cached; otherwise it's asked for (a placeholder meanwhile)
    fn bind_thumb(&self, pic: &gtk::Picture, path: &Path) {
        let t = thumb_path(path);
        match gdk::Texture::from_filename(&t) {
            Ok(tex) => pic.set_paintable(Some(&tex)),
            Err(_) => {
                pic.set_paintable(None::<&gdk::Paintable>);
                let _ = self.thumb_tx.send(path.to_path_buf());
            }
        }
    }

    fn open(&self, i: usize) {
        let Some(path) = self.photos.borrow().get(i).cloned() else { return };
        *self.current.borrow_mut() = Some(i);
        self.set_zoom(1.0);
        self.title.set_text(&stamp(&path));
        self.stack.set_visible_child_name("viewer");
        // the processed picture, if there is one; else the thumbnail at once and the quick
        // look in about a second
        let render = render_path(&path);
        let busy = self.processing.borrow().contains(&path);
        self.status.set_text("processing…");
        self.status.set_visible(busy);
        if let Ok(tex) = gdk::Texture::from_filename(&render) {
            self.picture.set_paintable(Some(&tex));
            self.process.set_visible(false);
        } else {
            self.process.set_visible(!busy);
            self.picture.set_paintable(gdk::Texture::from_filename(thumb_path(&path)).ok().as_ref());
        }
        self.show_info();
        // the quick look (and the photo's details) either way
        let tx = self.done_tx.clone();
        let p = path.clone();
        thread::spawn(move || {
            let r = std::fs::File::open(&p)
                .and_then(|f| lri::quick(&mut std::io::BufReader::with_capacity(1 << 20, f)))
                .map_err(|e| e.to_string());
            let _ = tx.send(Done::Quick(p, r));
        });
    }

    // the full picture, by Light's renderer (about 15 s); queued, so several can be asked for
    fn process(&self) {
        let Some(path) = self.showing() else { return };
        if self.processing.borrow_mut().insert(path.clone()) {
            let _ = self.render_tx.send(path);
        }
        self.process.set_visible(false);
        self.status.set_text("processing…");
        self.status.set_visible(true);
    }

    fn showing(&self) -> Option<PathBuf> {
        let i = (*self.current.borrow())?;
        self.photos.borrow().get(i).cloned()
    }

    fn step(&self, by: i64) {
        let Some(i) = *self.current.borrow() else { return };
        let n = self.photos.borrow().len() as i64;
        let j = i as i64 + by;
        if (0..n).contains(&j) {
            self.open(j as usize);
        }
    }

    fn close_viewer(&self) {
        *self.current.borrow_mut() = None;
        self.picture.set_paintable(None::<&gdk::Paintable>);
        self.stack.set_visible_child_name("grid");
    }

    fn on_done(&self, done: Done) {
        match done {
            Done::Thumb(path) => {
                if let Some(pic) = self.thumbs.borrow().get(&path) {
                    pic.set_paintable(gdk::Texture::from_filename(thumb_path(&path)).ok().as_ref());
                }
            }
            Done::Quick(path, r) => {
                if let Ok(p) = &r {
                    *self.info.borrow_mut() = Some((path.clone(), p.info.clone()));
                    if self.showing().as_ref() == Some(&path) {
                        self.show_info();
                    }
                }
                // unless the processed picture got there first
                if self.showing().as_ref() != Some(&path) || render_path(&path).exists() {
                    return;
                }
                if let Ok(p) = r {
                    let tex = gdk::MemoryTexture::new(
                        p.width as i32,
                        p.height as i32,
                        gdk::MemoryFormat::R8g8b8,
                        &glib::Bytes::from_owned(p.rgb),
                        p.width as usize * 3,
                    );
                    self.picture.set_paintable(Some(&tex));
                }
            }
            Done::Render(path, r) => {
                self.processing.borrow_mut().remove(&path);
                if self.showing().as_ref() != Some(&path) {
                    return;
                }
                match r.and_then(|f| gdk::Texture::from_filename(&f).map_err(|e| e.to_string())) {
                    Ok(tex) => {
                        self.picture.set_paintable(Some(&tex));
                        self.status.set_visible(false);
                        self.show_info();
                    }
                    Err(e) => {
                        self.status.set_text(&format!("processing failed: {e}"));
                        self.process.set_visible(true);
                    }
                }
            }
        }
    }

    // pinch zoom: the picture in its scroller, @z times the scroller's size (1: fitted)
    fn set_zoom(&self, z: f64) {
        let z = z.clamp(1.0, 6.0);
        *self.zoom.borrow_mut() = z;
        if z <= 1.0 {
            self.picture.set_size_request(-1, -1);
        } else {
            let (w, h) = (self.scroller.width() as f64, self.scroller.height() as f64);
            self.picture.set_size_request((w * z) as i32, (h * z) as i32);
        }
    }

    // a pinch: the point of the picture that was between the fingers stays between them
    fn pinch_begin(&self, centre: (f64, f64)) {
        let (h, v) = (self.scroller.hadjustment(), self.scroller.vadjustment());
        *self.zoom_start.borrow_mut() = *self.zoom.borrow();
        *self.pinch_at.borrow_mut() = (h.value() + centre.0, v.value() + centre.1);
    }

    fn pinch(&self, scale: f64, centre: (f64, f64)) {
        let z0 = *self.zoom_start.borrow();
        self.set_zoom(z0 * scale);
        let k = *self.zoom.borrow() / z0;
        let (px, py) = *self.pinch_at.borrow();
        *self.scroll_to.borrow_mut() = Some((px * k - centre.0, py * k - centre.1));
        self.scroll();
    }

    // the scroll a pinch wants, once the picture's new size is laid out
    fn scroll(&self) {
        if let Some((x, y)) = *self.scroll_to.borrow() {
            self.scroller.hadjustment().set_value(x);
            self.scroller.vadjustment().set_value(y);
        }
    }

    // the photo's details, in the info panel
    fn show_info(&self) {
        let grid = &self.info_box;
        while let Some(c) = grid.first_child() {
            grid.remove(&c);
        }
        let Some(path) = self.showing() else { return };
        let mut rows: Vec<(&str, String)> = vec![
            ("File", path.file_name().unwrap_or_default().to_string_lossy().into_owned()),
            ("Taken", stamp(&path)),
        ];
        if let Some((_, i)) = self.info.borrow().as_ref().filter(|(p, _)| *p == path) {
            // where it was taken (geotagging): the nearest town
            if let Some(place) = i.location.and_then(|(lat, lon)| places::near(lat, lon)) {
                rows.insert(2, ("Place", place));
            }
            if let Some(f) = i.focal_length {
                rows.push(("Focal length", format!("{f} mm")));
            }
            if let Some(t) = i.exposure_ns {
                rows.push(("Shutter", shutter(t)));
            }
            if let Some(iso) = i.iso {
                rows.push(("ISO", iso.to_string()));
            }
            let wb = match i.awb_mode {
                Some(0) => Some("Auto"),
                Some(1) => Some("Daylight"),
                Some(3) => Some("Cloudy"),
                Some(4) => Some("Incandescent"),
                Some(5) => Some("Fluorescent"),
                _ => None,
            };
            if let Some(wb) = wb {
                rows.push(("White balance", wb.to_string()));
            }
        }
        if let Some(mb) = megabytes(&path) {
            rows.push(("LRI", mb));
        }
        rows.push(match megabytes(&render_path(&path)) {
            Some(mb) => {
                let size = self
                    .picture
                    .paintable()
                    .filter(|_| !self.process.is_visible() && !self.status.is_visible())
                    .map(|t| format!("{} × {}, ", t.intrinsic_width(), t.intrinsic_height()))
                    .unwrap_or_default();
                ("Processed", format!("{size}{mb}"))
            }
            None => ("Processed", "not yet".to_string()),
        });
        for (i, (k, v)) in rows.into_iter().enumerate() {
            let key = gtk::Label::new(Some(k));
            key.add_css_class("key");
            key.set_xalign(0.0);
            let val = gtk::Label::new(Some(&v));
            val.set_xalign(0.0);
            grid.attach(&key, 0, i as i32, 1, 1);
            grid.attach(&val, 1, i as i32, 1, 1);
        }
    }

    fn delete(self: &Rc<Self>) {
        let Some(path) = self.showing() else { return };
        let dialog = gtk::AlertDialog::builder()
            .message("Delete this photo?")
            .detail("The LRI goes, with its processed JPEG. This can't be undone.")
            .buttons(["Cancel", "Delete"])
            .cancel_button(0)
            .default_button(0)
            .build();
        let s = self.clone();
        dialog.choose(Some(&self.window), None::<&gio::Cancellable>, move |r| {
            if r != Ok(1) {
                return;
            }
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_file(render_path(&path));
            let _ = std::fs::remove_file(thumb_path(&path));
            let i = s.current.borrow().unwrap_or(0);
            s.refresh();
            let n = s.photos.borrow().len();
            if n == 0 {
                s.close_viewer();
            } else {
                s.open(i.min(n - 1));
            }
        });
    }
}

fn build(app: &gtk::Application) -> Rc<Gallery> {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(CSS);
    gtk::style_context_add_provider_for_display(
        &gdk::Display::default().expect("display"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let window = gtk::ApplicationWindow::builder().application(app).title("Lightbox").build();
    window.add_css_class("gallery");
    window.set_decorated(false);

    // the grid: a heading and the photos for each day
    let days = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let grid_scroll = gtk::ScrolledWindow::new();
    grid_scroll.set_child(Some(&days));
    grid_scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    let empty = gtk::Label::new(Some("No photos yet"));
    empty.add_css_class("empty");
    empty.set_can_target(false);
    let grid_overlay = gtk::Overlay::new();
    grid_overlay.set_child(Some(&grid_scroll));
    grid_overlay.add_overlay(&empty);
    grid_overlay.set_vexpand(true);
    let grid_camera = icons::button(icons::CAMERA, "");
    let grid_bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    grid_bar.add_css_class("bar");
    grid_bar.set_halign(gtk::Align::Fill);
    // choosing photos (a long press on one): cancel, how many, delete
    let sel_cancel = icons::button(icons::CLOSE, "");
    let sel_label = gtk::Label::new(None);
    let sel_delete = icons::button(icons::DELETE, "");
    let sel_start = icons::button(icons::SELECT, "");
    for w in [sel_cancel.upcast_ref::<gtk::Widget>(), sel_label.upcast_ref(), sel_delete.upcast_ref()] {
        w.set_visible(false);
    }
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    grid_bar.append(&sel_cancel);
    grid_bar.append(&sel_label);
    grid_bar.append(&spacer);
    grid_bar.append(&sel_delete);
    grid_bar.append(&sel_start);
    grid_bar.append(&grid_camera);
    let grid_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    grid_page.append(&grid_bar);
    grid_page.append(&grid_overlay);
    grid_page.add_css_class("gallery-page");

    // the viewer
    let picture = gtk::Picture::new();
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture.set_can_shrink(true);
    picture.set_hexpand(true);
    picture.set_vexpand(true);
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_child(Some(&picture));
    let back = icons::button(icons::ARROW_LEFT, "");
    let title = gtk::Label::new(None);
    title.set_hexpand(true);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_xalign(0.0);
    let process = icons::button(icons::PROCESS, "process");
    let info = gtk::ToggleButton::new();
    icons::set(info.upcast_ref(), icons::INFO, "");
    let delete = icons::button(icons::DELETE, "");
    let camera = icons::button(icons::CAMERA, "");
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    bar.add_css_class("bar");
    bar.set_valign(gtk::Align::Start);
    bar.append(&back);
    bar.append(&title);
    bar.append(&process);
    bar.append(&info);
    bar.append(&delete);
    // more: the LRI or the processed JPEG (render) copied, or shown in its folder (the
    // render's greyed out until there is one)
    let more = gtk::MenuButton::new();
    more.set_child(Some(&icons::label(icons::MORE)));
    let copy_lri = icons::button(icons::COPY, "copy LRI");
    let copy_render = icons::button(icons::COPY, "copy render");
    let folder_lri = icons::button(icons::FOLDER, "show LRI in folder");
    let folder_render = icons::button(icons::FOLDER, "show render in folder");
    let menu_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    for b in [&copy_lri, &copy_render, &folder_lri, &folder_render] {
        b.set_halign(gtk::Align::Fill);
        if let Some(l) = b.child() {
            l.set_halign(gtk::Align::Start);
        }
        menu_box.append(b);
    }
    let menu = gtk::Popover::new();
    menu.add_css_class("lightbox-menu");
    menu.set_has_arrow(false);
    menu.set_child(Some(&menu_box));
    more.set_popover(Some(&menu));
    bar.append(&more);
    bar.append(&camera);
    let info_box = gtk::Grid::new();
    info_box.set_column_spacing(16);
    info_box.set_row_spacing(6);
    info_box.add_css_class("info");
    info_box.set_halign(gtk::Align::End);
    info_box.set_valign(gtk::Align::Start);
    info_box.set_margin_top(64);
    info_box.set_margin_end(12);
    info_box.set_can_target(false);
    info.bind_property("active", &info_box, "visible").sync_create().build();
    let status = gtk::Label::new(None);
    status.add_css_class("status");
    status.set_valign(gtk::Align::End);
    status.set_halign(gtk::Align::Center);
    status.set_margin_bottom(16);
    status.set_can_target(false);
    status.set_visible(false);
    let viewer = gtk::Overlay::new();
    viewer.set_child(Some(&scroller));
    viewer.add_overlay(&bar);
    viewer.add_overlay(&info_box);
    viewer.add_overlay(&status);
    viewer.add_css_class("gallery-page");

    let stack = gtk::Stack::new();
    stack.add_named(&grid_page, Some("grid"));
    stack.add_named(&viewer, Some("viewer"));
    window.set_child(Some(&stack));

    // workers: thumbnails (glycin-thumbnailer, the desktop's own) and renders (l16-render),
    // one at a time each; renders newest request first
    let (done_tx, done_rx) = mpsc::channel::<Done>();
    let (thumb_tx, thumb_rx) = mpsc::channel::<PathBuf>();
    let tx = done_tx.clone();
    thread::spawn(move || {
        while let Ok(p) = thumb_rx.recv() {
            let out = thumb_path(&p);
            if !out.exists() {
                let _ = std::fs::create_dir_all(out.parent().unwrap());
                let _ = Command::new("glycin-thumbnailer")
                    .args(["--input", &gio::File::for_path(&p).uri(), "--output"])
                    .arg(&out)
                    .args(["--size", "256"])
                    .output();
            }
            let _ = tx.send(Done::Thumb(p));
        }
    });
    let (render_tx, render_rx) = mpsc::channel::<PathBuf>();
    let tx = done_tx.clone();
    thread::spawn(move || {
        while let Ok(p) = render_rx.recv() {
            let out = render_path(&p);
            if out.exists() {
                let _ = tx.send(Done::Render(p, Ok(out)));
                continue;
            }
            // written under a hidden name, then put in place whole
            let part = out.with_file_name(format!(".{}.part.jpg", out.file_stem().unwrap_or_default().to_string_lossy()));
            let r = Command::new("nice")
                .args(["-n", "10", "l16-render"])
                .arg(&p)
                .arg(&part)
                .output();
            let r = match r {
                Ok(o) if o.status.success() => std::fs::rename(&part, &out).map(|_| out).map_err(|e| e.to_string()),
                Ok(o) => Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("failed").to_string()),
                Err(e) => Err(e.to_string()),
            };
            let _ = tx.send(Done::Render(p, r));
        }
    });

    let g = Rc::new(Gallery {
        photos: RefCell::new(Vec::new()),
        days,
        thumbs: RefCell::new(HashMap::new()),
        selecting: RefCell::new(false),
        selected: RefCell::new(HashSet::new()),
        checks: RefCell::new(HashMap::new()),
        grid_camera: grid_camera.clone(),
        sel_start: sel_start.clone(),
        sel_cancel: sel_cancel.clone(),
        sel_label: sel_label.clone(),
        sel_delete: sel_delete.clone(),
        cell: RefCell::new((248, 186, 3)),
        stack,
        empty,
        picture,
        scroller,
        status,
        title,
        process: process.clone(),
        info_box,
        info: RefCell::new(None),
        processing: RefCell::new(HashSet::new()),
        current: RefCell::new(None),
        zoom: RefCell::new(1.0),
        zoom_start: RefCell::new(1.0),
        pinch_at: RefCell::new((0.0, 0.0)),
        scroll_to: RefCell::new(None),
        thumb_tx,
        render_tx,
        done_tx,
        window: window.clone(),
    });

    let a = g.clone();
    grid_scroll.hadjustment().connect_changed(move |adj| a.fit_grid(adj.page_size()));
    let a = g.clone();
    sel_start.connect_clicked(move |_| a.set_selecting(true));
    let a = g.clone();
    sel_cancel.connect_clicked(move |_| a.set_selecting(false));
    let a = g.clone();
    sel_delete.connect_clicked(move |_| a.delete_selected());
    for (b, render, copy) in [(&copy_lri, false, true), (&copy_render, true, true), (&folder_lri, false, false), (&folder_render, true, false)] {
        let (a, m) = (g.clone(), menu.clone());
        b.connect_clicked(move |_| {
            m.popdown();
            if copy {
                a.copy_file(render);
            } else {
                a.show_in_folder(render);
            }
        });
    }
    // the render's entries only once the photo is processed
    let a = g.clone();
    let (cr, fr) = (copy_render.clone(), folder_render.clone());
    menu.connect_show(move |_| {
        let done = a.showing().is_some_and(|p| render_path(&p).exists());
        cr.set_sensitive(done);
        fr.set_sensitive(done);
    });
    let a = g.clone();
    back.connect_clicked(move |_| a.close_viewer());
    let a = g.clone();
    process.connect_clicked(move |_| a.process());
    let a = g.clone();
    delete.connect_clicked(move |_| a.delete());
    camera.connect_clicked(|_| launch("org.l16linux.Camera.desktop", None));
    grid_camera.connect_clicked(|_| launch("org.l16linux.Camera.desktop", None));

    // swipe between photos (when not zoomed in); pinch to zoom
    let swipe = gtk::GestureSwipe::new();
    swipe.set_touch_only(false);
    let a = g.clone();
    swipe.connect_swipe(move |_, vx, vy| {
        if *a.zoom.borrow() > 1.0 || vx.abs() < 400.0 || vx.abs() < vy.abs() {
            return;
        }
        a.step(if vx < 0.0 { 1 } else { -1 });
    });
    g.scroller.add_controller(swipe);
    let pinch = gtk::GestureZoom::new();
    let a = g.clone();
    pinch.connect_begin(move |p, _| {
        if let Some(c) = p.bounding_box_center() {
            a.pinch_begin(c);
        }
    });
    let a = g.clone();
    pinch.connect_scale_changed(move |p, s| {
        if let Some(c) = p.bounding_box_center() {
            a.pinch(s, c);
        }
    });
    let a = g.clone();
    pinch.connect_end(move |_, _| {
        let a = a.clone();
        glib::timeout_add_local_once(Duration::from_millis(200), move || *a.scroll_to.borrow_mut() = None);
    });
    g.scroller.add_controller(pinch);
    // the picture's new size makes the scroll range: then the pinch's scroll can be made
    for adj in [g.scroller.hadjustment(), g.scroller.vadjustment()] {
        let a = g.clone();
        adj.connect_changed(move |_| a.scroll());
    }

    // finished work, from the threads
    let a = g.clone();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        while let Ok(d) = done_rx.try_recv() {
            a.on_done(d);
        }
        glib::ControlFlow::Continue
    });
    // new photos (the camera's) when the gallery comes back into view
    let a = g.clone();
    window.connect_is_active_notify(move |w| {
        if w.is_active() && a.current.borrow().is_none() {
            a.refresh();
        }
    });

    g.refresh();
    window.present();
    g
}

fn main() -> glib::ExitCode {
    let app = gtk::Application::builder()
        .application_id("org.l16linux.Gallery")
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();
    let gallery: Rc<RefCell<Option<Rc<Gallery>>>> = Rc::new(RefCell::new(None));
    let g = gallery.clone();
    app.connect_activate(move |app| {
        let gal = g.borrow().clone();
        match gal {
            Some(gal) => gal.window.present(),
            None => *g.borrow_mut() = Some(build(app)),
        }
    });
    // l16-gallery FILE.lri: straight to that photo (the camera's last-photo button)
    let g = gallery.clone();
    app.connect_open(move |app, files, _| {
        if g.borrow().is_none() {
            *g.borrow_mut() = Some(build(app));
        }
        let gal = g.borrow().clone().unwrap();
        gal.refresh();
        if let Some(path) = files.first().and_then(|f| f.path()) {
            let i = gal.photos.borrow().iter().position(|p| *p == path);
            if let Some(i) = i {
                gal.open(i);
            }
        }
        gal.window.present();
    });
    app.run()
}
