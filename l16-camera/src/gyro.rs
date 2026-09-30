// Stillness, from the gyro (the SLPI's BMI160, qcom-smgr-gyro): for tripod mode, which lets
// an auto photo take up to 100 ms rather than the handheld 42 ms. Stock turns it on while the
// camera is still (its trigger is in a library we don't have); here: still once the turn
// rate has stayed under STILL for a second, moving again as soon as it passes MOVING.
// The gyro is read only while `on` is set (the preview running). Its buffer and device are
// the video group's (device-light-lfc's udev rule).

use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const STILL: f64 = 0.01; // rad/s
const MOVING: f64 = 0.03;
const SETTLE: Duration = Duration::from_secs(1);

fn find() -> Option<(PathBuf, PathBuf)> {
    for e in fs::read_dir("/sys/bus/iio/devices").ok()?.flatten() {
        if fs::read_to_string(e.path().join("name")).ok()?.trim() == "qcom-smgr-gyro" {
            let dev = PathBuf::from("/dev").join(e.file_name());
            return Some((e.path(), dev));
        }
    }
    None
}

fn set(p: PathBuf, v: &str) -> std::io::Result<()> {
    fs::write(p, v)
}

// the x, y, z turn rates (s32 each) into the buffer, no timestamp; enabled or not
fn enable(sys: &PathBuf, on: bool) -> std::io::Result<()> {
    set(sys.join("buffer/enable"), "0")?;
    if on {
        for a in ["x", "y", "z"] {
            set(sys.join(format!("scan_elements/in_anglvel_{a}_en")), "1")?;
        }
        set(sys.join("scan_elements/in_timestamp_en"), "0")?;
        set(sys.join("buffer/length"), "64")?;
        set(sys.join("buffer/enable"), "1")?;
    }
    Ok(())
}

// the thread: `still` follows the camera while `on`; false while off
pub fn spawn(on: Arc<AtomicBool>, still: Arc<AtomicBool>) {
    thread::spawn(move || {
        let Some((sys, dev)) = find() else { return };
        let scale: f64 = fs::read_to_string(sys.join("in_anglvel_scale"))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0.000015258);
        loop {
            while !on.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(200));
            }
            if let Err(e) = enable(&sys, true) {
                eprintln!("l16-camera: gyro: {e}");
                return;
            }
            let Ok(mut f) = fs::File::open(&dev) else { return };
            let mut quiet_since = Instant::now();
            let mut buf = [0u8; 12];
            while on.load(Ordering::Relaxed) && f.read_exact(&mut buf).is_ok() {
                let w: f64 = (0..3)
                    .map(|k| i32::from_le_bytes(buf[k * 4..k * 4 + 4].try_into().unwrap()) as f64 * scale)
                    .map(|v| v * v)
                    .sum::<f64>()
                    .sqrt();
                if w > STILL {
                    quiet_since = Instant::now();
                }
                if w > MOVING {
                    still.store(false, Ordering::Relaxed);
                } else if quiet_since.elapsed() >= SETTLE {
                    still.store(true, Ordering::Relaxed);
                }
            }
            still.store(false, Ordering::Relaxed);
            let _ = enable(&sys, false);
        }
    });
}
