// A photo's place: the nearest town of 1000 people or more to its location, looked up on the
// camera (the location isn't sent anywhere). The table is tools/make-places' (GeoNames,
// CC BY 4.0): name, latitude, longitude, division (a state...), country, a line each.

use std::cell::OnceCell;

const TABLE: &str = "/usr/share/l16-gallery/places.tsv";

struct Places {
    text: String,
    // latitude, longitude (degrees) and the line's start in text
    index: Vec<(f32, f32, u32)>,
}

thread_local! {
    static PLACES: OnceCell<Option<Places>> = const { OnceCell::new() };
}

fn load() -> Option<Places> {
    let text = std::fs::read_to_string(TABLE).ok()?;
    let mut index = Vec::with_capacity(180_000);
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        let mut f = line.split('\t');
        if let (Some(_), Some(lat), Some(lon)) = (f.next(), f.next(), f.next()) {
            if let (Ok(lat), Ok(lon)) = (lat.parse::<f32>(), lon.parse::<f32>()) {
                index.push((lat, lon, start as u32));
            }
        }
        start += line.len();
    }
    Some(Places { text, index })
}

// "Town, State" (or "Town, Country" where there's no division) nearest to @lat, @lon
pub fn near(lat: f64, lon: f64) -> Option<String> {
    PLACES.with(|p| {
        let places = p.get_or_init(load).as_ref()?;
        let (lat, lon) = (lat as f32, lon as f32);
        let k = lat.to_radians().cos();
        let mut best = None;
        let mut best_d = f32::MAX;
        for &(plat, plon, start) in &places.index {
            let dlon = {
                let d = (plon - lon).abs();
                if d > 180.0 { 360.0 - d } else { d }
            };
            let d = (plat - lat).powi(2) + (dlon * k).powi(2);
            if d < best_d {
                best_d = d;
                best = Some(start);
            }
        }
        let line = places.text[best? as usize..].lines().next()?;
        let f: Vec<&str> = line.split('\t').collect();
        let (name, division, country) = (f.first()?, f.get(3)?, f.get(4)?);
        Some(if division.is_empty() { format!("{name}, {country}") } else { format!("{name}, {division}") })
    })
}
