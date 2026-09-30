// A quick look at an LRI file: the reference module's frame, as the file is streamed past
// (LRIs are 200-700 MB and can't be seeked here), debayered at half resolution and white
// balanced with the file's own gains. Not the fused picture: one module, in about a second.
//
// An LRI is a series of blocks, each {"LELR", u64 length, u64 message offset, u32 message
// length, u8 type, ...} and its data. The camera ASICs' blocks hold module images, then a
// LightHeader protobuf naming the reference module (field 5) and where each module's image
// is (field 12, sensor_data_surface). Images go round by round (one frame of every module,
// then the next), so the reference's first frame is always among a block's first records.

use std::io::{self, Read};

// the first bytes of the reference's block kept while waiting for its header: a round of
// frames (at most 6 modules on ASIC1, 16-17 MB each)
const KEEP: usize = 112 << 20;
const BLACK: f32 = 42.0; // the ASICs' 10-bit pedestal

pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>, // sRGB, 8 bits a channel
}

#[derive(Default, Clone, Copy)]
struct Surface {
    offset: u64,
    width: usize,
    height: usize,
    stride: usize,
}

#[derive(Default, Clone)]
struct Module {
    id: u64,
    frame: u64,
    surface: Surface,
    red: Option<(i64, i64)>, // the red pixel's (x, y) in a 2x2 quad; none: monochrome
    gain: f32,                // analog x digital
}

// --- protobuf, enough of it ----------------------------------------------------------------

fn varint(b: &[u8], i: &mut usize) -> Option<u64> {
    let (mut v, mut shift) = (0u64, 0);
    loop {
        let c = *b.get(*i)?;
        *i += 1;
        v |= ((c & 0x7f) as u64).checked_shl(shift)?;
        shift += 7;
        if c < 0x80 {
            return Some(v);
        }
    }
}

enum Val<'a> {
    Int(u64),
    Bytes(&'a [u8]),
}

fn fields(b: &[u8]) -> Vec<(u64, Val<'_>)> {
    let (mut out, mut i) = (Vec::new(), 0);
    while i < b.len() {
        let Some(key) = varint(b, &mut i) else { break };
        let v = match key & 7 {
            0 => match varint(b, &mut i) {
                Some(v) => Val::Int(v),
                None => break,
            },
            1 | 5 => {
                let n = if key & 7 == 1 { 8 } else { 4 };
                let Some(s) = b.get(i..i + n) else { break };
                i += n;
                Val::Bytes(s)
            }
            2 => {
                let Some(n) = varint(b, &mut i) else { break };
                let Some(s) = b.get(i..i + n as usize) else { break };
                i += n as usize;
                Val::Bytes(s)
            }
            _ => break,
        };
        if key >> 3 == 0 {
            break;
        }
        out.push((key >> 3, v));
    }
    out
}

fn int(v: &Val) -> Option<u64> {
    match v {
        Val::Int(i) => Some(*i),
        _ => None,
    }
}

fn f32_of(v: &Val) -> Option<f32> {
    match v {
        Val::Bytes(b) if b.len() == 4 => Some(f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        _ => None,
    }
}

fn sub<'a>(v: &Val<'a>) -> Vec<(u64, Val<'a>)> {
    match v {
        Val::Bytes(b) => fields(b),
        _ => Vec::new(),
    }
}

// a Point2I {x 1, y 2} (sint? int32 as varint; -1 as a 64-bit varint)
fn point(v: &Val) -> (i64, i64) {
    let mut p = (0i64, 0i64);
    for (n, f) in sub(v) {
        let x = int(&f).unwrap_or(0) as i64;
        if n == 1 {
            p.0 = x;
        } else if n == 2 {
            p.1 = x;
        }
    }
    p
}

fn module(v: &Val) -> Module {
    let mut m = Module { gain: 1.0, ..Default::default() };
    let (mut ag, mut dg) = (1.0f32, 1.0f32);
    for (n, f) in sub(v) {
        match n {
            2 => m.id = int(&f).unwrap_or(99),
            7 => ag = f32_of(&f).unwrap_or(1.0),
            14 => dg = f32_of(&f).unwrap_or(1.0),
            15 => m.frame = int(&f).unwrap_or(0),
            13 => {
                let p = point(&f);
                m.red = (p.0 >= 0 && p.1 >= 0 && p.0 < 2 && p.1 < 2).then_some(p);
            }
            9 => {
                for (s, g) in sub(&f) {
                    match s {
                        2 => {
                            let p = point(&g);
                            m.surface.width = p.0 as usize;
                            m.surface.height = p.1 as usize;
                        }
                        4 => m.surface.stride = int(&g).unwrap_or(0) as usize,
                        5 => m.surface.offset = int(&g).unwrap_or(0),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    m.gain = ag * dg;
    m
}

// ViewPreferences: awb_gains (15: ChannelGain {r 1, g_r 2, g_b 3, b 4}), image_gain (10)
fn view(v: &[(u64, Val)], wb: &mut Option<(f32, f32)>, image_gain: &mut Option<f32>) {
    for (n, f) in v {
        match n {
            15 => {
                let g: Vec<_> = sub(f);
                let get = |k| g.iter().find(|(n, _)| *n == k).and_then(|(_, v)| f32_of(v));
                if let (Some(r), Some(b)) = (get(1), get(4)) {
                    if r > 0.0 && b > 0.0 {
                        *wb = Some((r, b));
                    }
                }
            }
            10 => *image_gain = f32_of(f).filter(|g| *g > 0.0),
            _ => {}
        }
    }
}

// --- reading the stream --------------------------------------------------------------------

fn skip(r: &mut impl Read, mut n: u64) -> io::Result<()> {
    let mut buf = vec![0u8; 1 << 20];
    while n > 0 {
        let k = (n as usize).min(buf.len());
        r.read_exact(&mut buf[..k])?;
        n -= k as u64;
    }
    Ok(())
}

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_string())
}

pub fn quick(r: &mut impl Read) -> io::Result<Picture> {
    let mut kept: Option<(Vec<u8>, Vec<Module>, Option<u64>)> = None;
    let (mut wb, mut image_gain) = (None, None);
    loop {
        let mut h = [0u8; 32];
        match r.read_exact(&mut h) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        }
        if &h[..4] != b"LELR" {
            break;
        }
        let u64_at = |p: usize| u64::from_le_bytes(h[p..p + 8].try_into().unwrap());
        let (len, off) = (u64_at(4), u64_at(12));
        let n = u32::from_le_bytes(h[20..24].try_into().unwrap()) as u64;
        if len < 32 || off < 32 || off + n > len {
            return Err(bad("damaged LRI block"));
        }
        // the first block with images: keep its first round, then read its header
        let has_images = off - 32 > (1 << 20);
        if has_images && kept.is_none() {
            let keep = ((off - 32) as usize).min(KEEP);
            let mut buf = vec![0u8; keep];
            r.read_exact(&mut buf)?;
            skip(r, off - 32 - keep as u64)?;
            let mut msg = vec![0u8; n as usize];
            r.read_exact(&mut msg)?;
            let fl = fields(&msg);
            let refcam = fl.iter().find(|(n, _)| *n == 5).and_then(|(_, v)| int(v));
            let mods = fl.iter().filter(|(n, _)| *n == 12).map(|(_, v)| module(v)).collect();
            kept = Some((buf, mods, refcam));
            skip(r, len - off - n)?;
        } else if !has_images && len < (16 << 20) {
            // calibration and view preferences: the view preferences are a type-1 block,
            // or field 19 of a LightHeader
            let mut rest = vec![0u8; (len - 32) as usize];
            r.read_exact(&mut rest)?;
            let msg = &rest[(off - 32) as usize..(off - 32 + n) as usize];
            if h[24] == 1 {
                view(&fields(msg), &mut wb, &mut image_gain);
            } else {
                for (f, v) in fields(msg) {
                    if f == 19 {
                        view(&sub(&v), &mut wb, &mut image_gain);
                    }
                }
            }
        } else {
            skip(r, len - 32)?;
        }
    }
    let Some((buf, mods, refcam)) = kept else { return Err(bad("no images in the LRI")) };
    // the reference module's first frame, or else any colour frame that was kept
    let usable = |m: &&Module| {
        m.red.is_some()
            && m.surface.stride > 0
            && m.surface.offset >= 32
            && (m.surface.offset - 32) as usize + m.surface.stride * m.surface.height <= buf.len()
    };
    let pick = mods
        .iter()
        .filter(usable)
        .find(|m| Some(m.id) == refcam && m.frame == 0)
        .or_else(|| mods.iter().filter(usable).next())
        .ok_or_else(|| bad("no colour frame in reach"))?;
    // the reference runs a stop under the others: bring it up to the photo's gain
    let others = mods.iter().filter(|m| m.id != pick.id).map(|m| m.gain).fold(0.0f32, f32::max);
    let target = image_gain.unwrap_or(if others > 0.0 { others } else { pick.gain });
    let exposure = (target / pick.gain.max(0.01)).clamp(1.0, 16.0);
    Ok(render(&buf[(pick.surface.offset - 32) as usize..], pick, wb, exposure))
}

// --- the picture ---------------------------------------------------------------------------

// pixel (x, y) of a little-endian 10-bit bitstream, @stride bytes a row
#[inline]
fn px(img: &[u8], stride: usize, x: usize, y: usize) -> f32 {
    let bit = x * 10;
    let o = y * stride + bit / 8;
    let v = img[o] as u32 | (img.get(o + 1).copied().unwrap_or(0) as u32) << 8;
    ((v >> (bit % 8)) & 1023) as f32
}

fn render(img: &[u8], m: &Module, wb: Option<(f32, f32)>, exposure: f32) -> Picture {
    let s = m.surface;
    let (w, h) = (s.width / 2, s.height / 2);
    let (rx, ry) = m.red.map(|(x, y)| (x as usize, y as usize)).unwrap_or((0, 0));
    let (bx, by) = (1 - rx, 1 - ry);
    // quads: red, blue and the two greens, black level off, into linear RGB
    let mut lin = vec![0f32; w * h * 3];
    let (mut sr, mut sg, mut sb) = (0f64, 0f64, 0f64);
    for qy in 0..h {
        for qx in 0..w {
            let (x0, y0) = (qx * 2, qy * 2);
            let r = px(img, s.stride, x0 + rx, y0 + ry) - BLACK;
            let b = px(img, s.stride, x0 + bx, y0 + by) - BLACK;
            let g = (px(img, s.stride, x0 + bx, y0 + ry) + px(img, s.stride, x0 + rx, y0 + by)) / 2.0 - BLACK;
            let i = (qy * w + qx) * 3;
            lin[i] = r.max(0.0);
            lin[i + 1] = g.max(0.0);
            lin[i + 2] = b.max(0.0);
            if r < 900.0 && g < 900.0 && b < 900.0 {
                sr += r.max(0.0) as f64;
                sg += g.max(0.0) as f64;
                sb += b.max(0.0) as f64;
            }
        }
    }
    // the file's white balance, or grey world
    let (gr, gb) = wb.unwrap_or_else(|| {
        if sr > 0.0 && sb > 0.0 { ((sg / sr) as f32, (sg / sb) as f32) } else { (1.0, 1.0) }
    });
    // the photo's exposure, held back if it would clip more than the brightest 0.5% (a
    // preview can't bring highlights back; the reference runs a stop under for them)
    let mut hist = [0u32; 256];
    for p in lin.chunks_exact(3) {
        let v = (p[0] * gr).max(p[1]).max(p[2] * gb) / (1023.0 - BLACK) * exposure;
        hist[((v * 128.0) as usize).min(255)] += 1;
    }
    let (mut n, total) = (0u32, (lin.len() / 3) as u32);
    let mut top = 255;
    while top > 0 && n + hist[top] < total / 200 {
        n += hist[top];
        top -= 1;
    }
    let bright = top as f32 / 128.0; // the 99.5th percentile, 1.0 = white
    let exposure = if bright > 1.0 { (exposure / bright).max(1.0) } else { exposure };
    let scale = exposure / (1023.0 - BLACK);
    let lut: Vec<u8> = (0..4096)
        .map(|i| {
            let v = i as f32 / 4095.0;
            let s = if v <= 0.0031308 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
            (s * 255.0).round() as u8
        })
        .collect();
    let enc = |v: f32| lut[((v * 4095.0).clamp(0.0, 4095.0)) as usize];
    let mut rgb = vec![0u8; w * h * 3];
    for (o, p) in rgb.chunks_exact_mut(3).zip(lin.chunks_exact(3)) {
        o[0] = enc(p[0] * gr * scale);
        o[1] = enc(p[1] * scale);
        o[2] = enc(p[2] * gb * scale);
    }
    Picture { width: w as u32, height: h as u32, rgb }
}
