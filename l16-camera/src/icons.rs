// Icons: Material Design's, from the Symbols Nerd Font (font-nerd-fonts-symbols), drawn as
// text so they take the text's colour. Shared with l16-gallery.
#![allow(dead_code)]

use gtk::prelude::*;

pub const FAMILY: &str = "Symbols Nerd Font";

pub const ARROW_LEFT: char = '\u{f004d}';
pub const CAMERA: char = '\u{f0100}';
pub const CHECK: char = '\u{f012c}';
pub const CHEVRON_RIGHT: char = '\u{f0142}';
pub const CHEVRON_UP: char = '\u{f0143}';
pub const CLOSE: char = '\u{f0156}';
pub const COG: char = '\u{f0493}';
pub const DELETE: char = '\u{f01b4}';
pub const HISTOGRAM: char = '\u{f0129}';
pub const INFO: char = '\u{f02fd}';
pub const PROCESS: char = '\u{f0068}'; // auto-fix: a wand
pub const GRID: char = '\u{f02c1}';
pub const GRID_OFF: char = '\u{f02c2}';
pub const TIMER: char = '\u{f051b}';
pub const TIMER_OFF: char = '\u{f051e}';
pub const BURST: char = '\u{f0693}';
pub const FLASH: char = '\u{f0241}';
pub const FLASH_AUTO: char = '\u{f0242}';
pub const FLASH_OFF: char = '\u{f0243}';
pub const FOCUS_AUTO: char = '\u{f0f4e}';
pub const CAMERA_LOCK: char = '\u{f1a15}'; // camera-lock-outline: tripod mode on
pub const MOON: char = '\u{f0594}'; // weather-night: stock's low-light assist (a stacked capture)
pub const CHEVRON_DOWN: char = '\u{f0140}';
// white balance presets, in wb::PRESETS' order: auto, incandescent, fluorescent, daylight, cloudy
pub const WB: [char; 5] = ['\u{f05a5}', '\u{f05a6}', '\u{f05a7}', '\u{f05a8}', '\u{f0590}'];

// Pango markup for an icon, with optional text after it
pub fn markup(icon: char, text: &str) -> String {
    let text = gtk::glib::markup_escape_text(text);
    let gap = if text.is_empty() { "" } else { " " };
    format!("<span font_family=\"{FAMILY}\" size=\"150%\">{icon}</span>{gap}{text}")
}

// a button's label as an icon (and text)
pub fn set(b: &gtk::Button, icon: char, text: &str) {
    b.set_label(&markup(icon, text));
    if let Some(l) = b.child().and_downcast::<gtk::Label>() {
        l.set_use_markup(true);
    }
}

pub fn button(icon: char, text: &str) -> gtk::Button {
    let b = gtk::Button::new();
    set(&b, icon, text);
    b
}

pub fn label(icon: char) -> gtk::Label {
    let l = gtk::Label::new(None);
    l.set_markup(&markup(icon, ""));
    l
}
