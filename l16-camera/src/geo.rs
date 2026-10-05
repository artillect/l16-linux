// Geotagging: where photos are taken, from geoclue (which l16-gnss feeds the modem's GPS).
// A geoclue client runs only while it is wanted (the setting on, the preview running), so the
// GPS doesn't run behind a closed or hidden camera.

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const BUS: &str = "org.freedesktop.GeoClue2";
const DESKTOP_ID: &str = "org.l16linux.Camera";
// a photo is tagged with a fix no older than this, and no worse (geoclue's IP fallback is
// ~25 km: a wrong place, not a rough one)
const MAX_AGE: Duration = Duration::from_secs(600);
const MAX_ACCURACY_M: f64 = 1000.0;
// after geoclue refuses (location services off), ask again only this much later: start() is
// called on every refresh (each step of a dial), and each refusal is a D-Bus round trip on
// the UI thread (hundreds a minute made the whole app stutter)
const RETRY: Duration = Duration::from_secs(30);

#[derive(Clone, Copy)]
pub struct Fix {
    pub lat: f64,
    pub lon: f64,
    pub accuracy: f64,
    pub altitude: Option<f64>,
    pub unix_secs: u64,
    at: Instant,
}

#[derive(Default)]
pub struct Geo {
    client: Option<gio::DBusProxy>,
    last: Rc<RefCell<Option<Fix>>>,
    retry_at: Option<Instant>,
    // the "location services are off" message was shown (since the client was last wanted)
    warned: bool,
}

impl Geo {
    // true the first time geoclue refuses because location services are off (to say so)
    pub fn start(&mut self) -> bool {
        if self.client.is_some() || self.retry_at.is_some_and(|t| Instant::now() < t) {
            return false;
        }
        match client(self.last.clone()) {
            Ok(c) => {
                self.client = Some(c);
                self.retry_at = None;
                false
            }
            Err(e) => {
                eprintln!("l16-camera: location: {e}");
                self.retry_at = Some(Instant::now() + RETRY);
                let off = e.to_string().contains("AccessDenied") && !self.warned;
                self.warned |= off;
                off
            }
        }
    }

    pub fn stop(&mut self) {
        self.retry_at = None;
        self.warned = false;
        if let Some(c) = self.client.take() {
            let _ = c.call_sync("Stop", None, gio::DBusCallFlags::NONE, 2000, None::<&gio::Cancellable>);
        }
    }

    // the fix for a photo taken now, if there is a good recent one
    pub fn fix(&self) -> Option<Fix> {
        let f = (*self.last.borrow())?;
        (f.at.elapsed() <= MAX_AGE && f.accuracy <= MAX_ACCURACY_M).then_some(f)
    }
}

fn client(last: Rc<RefCell<Option<Fix>>>) -> Result<gio::DBusProxy, glib::Error> {
    let conn = gio::bus_get_sync(gio::BusType::System, None::<&gio::Cancellable>)?;
    let path: String = conn
        .call_sync(
            Some(BUS),
            "/org/freedesktop/GeoClue2/Manager",
            "org.freedesktop.GeoClue2.Manager",
            "GetClient",
            None,
            Some(glib::VariantTy::new("(o)").unwrap()),
            gio::DBusCallFlags::NONE,
            5000,
            None::<&gio::Cancellable>,
        )?
        .child_value(0)
        .str()
        .unwrap_or_default()
        .to_string();
    let set = |name: &str, value: glib::Variant| {
        conn.call_sync(
            Some(BUS),
            &path,
            "org.freedesktop.DBus.Properties",
            "Set",
            Some(&("org.freedesktop.GeoClue2.Client", name, value).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            2000,
            None::<&gio::Cancellable>,
        )
    };
    set("DesktopId", DESKTOP_ID.to_variant())?;
    // 8: exact (GPS)
    set("RequestedAccuracyLevel", 8u32.to_variant())?;
    let proxy = gio::DBusProxy::for_bus_sync(
        gio::BusType::System,
        gio::DBusProxyFlags::DO_NOT_LOAD_PROPERTIES,
        None,
        BUS,
        &path,
        "org.freedesktop.GeoClue2.Client",
        None::<&gio::Cancellable>,
    )?;
    proxy.connect_local("g-signal", false, move |args| {
        let signal = args[2].get::<String>().ok()?;
        if signal != "LocationUpdated" {
            return None;
        }
        let params = args[3].get::<glib::Variant>().ok()?;
        let new = params.child_value(1).str()?.to_string();
        if let Some(f) = location(&new) {
            *last.borrow_mut() = Some(f);
        }
        None
    });
    proxy.call_sync("Start", None, gio::DBusCallFlags::NONE, 5000, None::<&gio::Cancellable>)?;
    Ok(proxy)
}

fn location(path: &str) -> Option<Fix> {
    let loc = gio::DBusProxy::for_bus_sync(
        gio::BusType::System,
        gio::DBusProxyFlags::NONE,
        None,
        BUS,
        path,
        "org.freedesktop.GeoClue2.Location",
        None::<&gio::Cancellable>,
    )
    .ok()?;
    let num = |name: &str| loc.cached_property(name).and_then(|v| v.get::<f64>());
    // geoclue's "unknown" altitude is -G_MAXDOUBLE
    let altitude = num("Altitude").filter(|a| a.is_finite() && *a > -1e300);
    let unix_secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    Some(Fix {
        lat: num("Latitude")?,
        lon: num("Longitude")?,
        accuracy: num("Accuracy")?,
        altitude,
        unix_secs,
        at: Instant::now(),
    })
}
