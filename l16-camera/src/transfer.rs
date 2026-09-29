// Photo transfers: each ASIC sends a photo's records over virtual channel 1 of its CSI-2
// link, beside the preview, into its own CAMSS RDI (/dev/video1-3). Those streams run for as
// long as the app does: stopping an RDI that is not receiving times out in CAMSS and leaves
// it unusable. Each arriving frame (one record) goes into the current photo's files.

use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

// what CAMSS stores of a record: 1408 x 9219 RAW10 (1760 bytes a line)
const FRAME: usize = 1760 * 9219;

pub struct Photo {
    pub dir: PathBuf,
    left: [u16; 3],
    files: [Option<File>; 3],
}

impl Photo {
    // records ASIC @a has sent so far
    pub fn received(&self, a: usize, records: u16) -> u16 {
        records - self.left[a]
    }

    pub fn new(dir: PathBuf, records: [u16; 3]) -> std::io::Result<Photo> {
        std::fs::create_dir_all(&dir)?;
        let mut files = [None, None, None];
        for a in 0..3 {
            if records[a] > 0 {
                files[a] = Some(File::create(dir.join(format!("asic{}.raw", a + 1)))?);
            }
        }
        Ok(Photo { dir, left: records, files })
    }
}

pub struct Transfers {
    pub photo: Arc<Mutex<Option<Photo>>>,
    streams: Vec<Child>,
}

impl Transfers {
    // links and formats the paths, starts the streams; @done gets each photo's directory
    // once all its records are in
    pub fn start(done: mpsc::Sender<Result<PathBuf, String>>) -> Result<Transfers, String> {
        let o = Command::new("l16-shoot").arg("setup").output().map_err(|e| e.to_string())?;
        if !o.status.success() {
            return Err(String::from_utf8_lossy(&o.stderr).trim().to_string());
        }
        let photo: Arc<Mutex<Option<Photo>>> = Arc::new(Mutex::new(None));
        let mut streams = Vec::new();
        for a in 0..3usize {
            let mut child = Command::new("v4l2-ctl")
                .args(["-d", &format!("/dev/video{}", a + 1)])
                // eight buffers: a photo's records (six per ASIC) all fit, so the RDI never
                // runs dry mid-photo (CAMSS's re-arm of an idle RDI loses what follows)
                .args(["--stream-mmap=8", "--stream-to=-"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| e.to_string())?;
            let mut out = child.stdout.take().unwrap();
            let photo = photo.clone();
            let done = done.clone();
            thread::spawn(move || {
                let mut buf = vec![0u8; FRAME];
                while out.read_exact(&mut buf).is_ok() {
                    let mut p = photo.lock().unwrap();
                    let Some(ph) = p.as_mut() else {
                        eprintln!("l16-camera: ASIC{} record with no photo waiting, dropped", a + 1);
                        continue;
                    };
                    if ph.left[a] == 0 {
                        eprintln!("l16-camera: ASIC{} record beyond the photo's, dropped", a + 1);
                        continue;
                    }
                    eprintln!("l16-camera: ASIC{} record in ({} left)", a + 1, ph.left[a] - 1);
                    let r = ph.files[a].as_mut().map(|f| f.write_all(&buf));
                    ph.left[a] -= 1;
                    if let Some(Err(e)) = r {
                        let dir = p.take().unwrap().dir;
                        let _ = done.send(Err(format!("{}: {e}", dir.display())));
                    } else if ph.left.iter().all(|&n| n == 0) {
                        let _ = done.send(Ok(p.take().unwrap().dir));
                    }
                }
            });
            streams.push(child);
        }
        Ok(Transfers { photo, streams })
    }

    pub fn stop(&mut self) {
        for c in &mut self.streams {
            let _ = c.kill();
            let _ = c.wait();
        }
        self.streams.clear();
    }
}
