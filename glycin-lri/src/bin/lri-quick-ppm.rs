// lri-quick-ppm IN.lri OUT.ppm: the loader's quick look, as a file (for testing)
#[path = "../lri.rs"]
mod lri;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let t = std::time::Instant::now();
    let f = std::fs::File::open(&args[1]).expect("open");
    let p = lri::quick(&mut std::io::BufReader::with_capacity(1 << 20, f)).expect("decode");
    let mut out = format!("P6\n{} {}\n255\n", p.width, p.height).into_bytes();
    out.extend_from_slice(&p.rgb);
    std::fs::write(&args[2], out).expect("write");
    eprintln!("{}x{} in {:.2} s", p.width, p.height, t.elapsed().as_secs_f64());
}
