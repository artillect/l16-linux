// Turning a photo: the LRI's own orientation (its view preferences' field 9, which the camera
// sets for a photo taken in portrait), so every program that reads LRIs turns it the same:
// Lightbox and the thumbnailer (glycin-lri), Light's renderer (l16-render), chiaro. Changed in
// place in the file's last view preferences (LightHeader field 19, in a padded block); a file
// with no room there, or none at all, gets a small LightHeader block of its own at the end,
// whose fields readers take over the earlier ones'. The images are never touched.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::ops::Range;
use std::path::Path;

// ViewPreferences.Orientation's turns: as taken, a quarter clockwise, half, a quarter
// counter-clockwise (the flipped ones, which no camera writes, count as taken)
const TURNS: [u64; 4] = [0, 1, 7, 2];

fn read_varint(b: &[u8], i: &mut usize) -> Option<u64> {
    let (mut v, mut shift) = (0u64, 0);
    loop {
        let byte = *b.get(*i)?;
        *i += 1;
        v |= ((byte & 0x7f) as u64) << shift;
        if byte < 0x80 {
            return Some(v);
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
}

fn varint(mut v: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return out;
        }
        out.push(byte | 0x80);
    }
}

// a protobuf message's fields: (number, wire type, the whole field's bytes, its value's)
fn fields(m: &[u8]) -> Option<Vec<(u64, u64, Range<usize>, Range<usize>)>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < m.len() {
        let start = i;
        let key = read_varint(m, &mut i)?;
        let value_start = i;
        match key & 7 {
            0 => {
                read_varint(m, &mut i)?;
            }
            1 => i += 8,
            2 => {
                let n = read_varint(m, &mut i)? as usize;
                let at = i;
                i = at.checked_add(n)?;
                if i > m.len() {
                    return None;
                }
                out.push((key >> 3, 2, start..i, at..i));
                continue;
            }
            5 => i += 4,
            _ => return None,
        }
        if i > m.len() {
            return None;
        }
        out.push((key >> 3, key & 7, start..i, value_start..i));
    }
    Some(out)
}

// the last LightHeader block holding view preferences: where it is, and its message
struct Prefs {
    block: u64,
    length: u64,
    msg_off: u64,
    msg: Vec<u8>,
    field: Range<usize>, // field 19 in msg
    value: Range<usize>, // its value (the ViewPreferences message)
}

fn last_prefs(f: &mut File) -> io::Result<Option<Prefs>> {
    let size = f.metadata()?.len();
    let (mut o, mut found) = (0u64, None);
    while o + 32 <= size {
        let mut h = [0u8; 32];
        f.seek(SeekFrom::Start(o))?;
        f.read_exact(&mut h)?;
        if &h[..4] != b"LELR" {
            break;
        }
        let length = u64::from_le_bytes(h[4..12].try_into().unwrap());
        let msg_off = u64::from_le_bytes(h[12..20].try_into().unwrap());
        let msg_len = u32::from_le_bytes(h[20..24].try_into().unwrap()) as u64;
        if length < 32 || msg_off < 32 || msg_off + msg_len > length {
            break;
        }
        // LightHeader messages are a few KB at most; view preferences are in the small ones
        if h[28] == 0 && msg_len < (1 << 20) {
            let mut msg = vec![0u8; msg_len as usize];
            f.seek(SeekFrom::Start(o + msg_off))?;
            f.read_exact(&mut msg)?;
            if let Some(fs) = fields(&msg) {
                if let Some((_, _, field, value)) = fs.into_iter().filter(|(n, wt, _, _)| *n == 19 && *wt == 2).last() {
                    found = Some(Prefs { block: o, length, msg_off, msg, field, value });
                }
            }
        }
        o += length;
    }
    Ok(found)
}

// the photo's orientation, as a number of quarter turns clockwise
fn quarters(prefs: &[u8]) -> usize {
    let o = fields(prefs)
        .unwrap_or_default()
        .into_iter()
        .filter(|(n, wt, _, _)| *n == 9 && *wt == 0)
        .last()
        .and_then(|(_, _, _, v)| read_varint(prefs, &mut v.start.clone()));
    o.and_then(|o| TURNS.iter().position(|t| *t == o)).unwrap_or(0)
}

// turn the photo @by quarters clockwise (-1: counter-clockwise)
pub fn turn(path: &Path, by: i32) -> io::Result<()> {
    let mut f = OpenOptions::new().read(true).write(true).open(path)?;
    let found = last_prefs(&mut f)?;
    let old: &[u8] = found.as_ref().map_or(&[], |p| &p.msg[p.value.clone()]);
    let to = TURNS[(quarters(old) as i32 + by).rem_euclid(4) as usize];
    // the view preferences without their orientation, then the new one (written even when
    // "as taken", to stand over an earlier one)
    let mut prefs: Vec<u8> = fields(old)
        .unwrap_or_default()
        .into_iter()
        .filter(|(n, _, _, _)| *n != 9)
        .flat_map(|(_, _, whole, _)| old[whole].to_vec())
        .collect();
    if found.is_none() {
        // no view preferences at all: the whole frame as the crop, as l16-lri-assemble's
        // (without one, Light's renderer crops in): 14 {start 1 {0, 0}, size 2 {1, 1}}
        let point = |x: f32, y: f32| [&[0x0du8][..], &x.to_le_bytes(), &[0x15], &y.to_le_bytes()].concat();
        let crop = [length_field(1, &point(0.0, 0.0)), length_field(2, &point(1.0, 1.0))].concat();
        prefs.extend(length_field(14, &crop));
    }
    prefs.extend(varint(9 << 3));
    prefs.extend(varint(to));
    let field = length_field(19, &prefs);

    if let Some(p) = &found {
        let msg = [&p.msg[..p.field.start], &field[..], &p.msg[p.field.end..]].concat();
        if p.msg_off + msg.len() as u64 <= p.length {
            // in place: the message (zeros over what's left of the old one) and its length
            let mut out = msg.clone();
            out.resize(msg.len().max(p.msg.len()), 0);
            f.seek(SeekFrom::Start(p.block + p.msg_off))?;
            f.write_all(&out)?;
            f.seek(SeekFrom::Start(p.block + 20))?;
            f.write_all(&(msg.len() as u32).to_le_bytes())?;
            return f.sync_all();
        }
    }
    // a LightHeader block of its own at the end, padded to 1 KB as the camera's
    let size = (32 + field.len() as u64 + 1023) / 1024 * 1024;
    let mut block = Vec::with_capacity(size as usize);
    block.extend(b"LELR");
    block.extend(size.to_le_bytes());
    block.extend(32u64.to_le_bytes());
    block.extend((field.len() as u32).to_le_bytes());
    block.extend([0u8; 8]); // type 0 (LightHeader), reserved
    block.extend(&field);
    block.resize(size as usize, 0);
    f.seek(SeekFrom::End(0))?;
    f.write_all(&block)?;
    f.sync_all()
}

fn length_field(n: u64, value: &[u8]) -> Vec<u8> {
    [varint(n << 3 | 2), varint(value.len() as u64), value.to_vec()].concat()
}
