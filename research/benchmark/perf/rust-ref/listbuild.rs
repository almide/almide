// listbuild — reference for the three listbuild_*.almd shapes (#1337).
// `Vec::with_capacity` + push: one sized allocation, no intermediates. Same
// arithmetic, same interleaving, same checksum, same output line.

fn main() {
    let size: u32 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);

    let n = 1usize << size;
    let pi = 3.14159265358979323846f64;

    let mut data: Vec<f64> = Vec::with_capacity(2 * n);
    for i in 0..n {
        let theta = i as f64 / n as f64 * pi;
        let re = 1.0 * (10.0 * theta).cos() + 0.5 * (25.0 * theta).cos();
        let im = 1.0 * (10.0 * theta).sin() + 0.5 * (25.0 * theta).sin();
        data.push((re * 100.0).round() / 100.0);
        data.push((im * 100.0).round() / 100.0);
    }

    let mut acc = 0.0f64;
    for i in 0..(2 * n) {
        acc += data[i];
    }
    println!("len: {} checksum: {:.6}", data.len(), acc);
}
