// mandelbrot — the ORDINARY sequential Rust for the program: P4 bitmap, one
// byte per 8 pixels, the escape loop as a person writes it. One thread, no
// `unsafe`, no SIMD. The arithmetic is kept in the same order as
// mandelbrot.almd (`cr = -1.5 + x * (2/n)`, `ci = 2y/n - 1`, the escape test
// on the squares before the update, 50 iterations) so the bitmap is
// byte-identical — bench.py verifies that before timing.
//
// `mandelbrot.almd` maps its 64 row-chunks with `fan.map`, which on the native
// leg is SEQUENTIAL (an `Rc<dyn Fn>` thunk cannot cross a thread scope), so
// this row compares one thread against one thread: the Almide side gets no
// parallelism here, and the row measures codegen parity, not the #1330 win.

use std::io::Write;

fn main() {
    let n: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(200);
    let mut out = std::io::stdout().lock();
    write!(out, "P4\n{} {}\n", n, n).unwrap();
    let inv_n = n as f64;
    let cr_step = 2.0 / inv_n;
    let bytes_per_row = (n + 7) / 8;
    let mut bitmap: Vec<u8> = Vec::with_capacity(n * bytes_per_row);
    for y in 0..n {
        let ci = 2.0 * y as f64 / inv_n - 1.0;
        for x_byte in 0..bytes_per_row {
            let mut byte_val: u8 = 0;
            for bit in 0..8 {
                let x = x_byte * 8 + bit;
                byte_val <<= 1;
                if x < n {
                    let cr = -1.5 + x as f64 * cr_step;
                    let mut zr = 0.0f64;
                    let mut zi = 0.0f64;
                    let mut inside = true;
                    let mut iter = 0;
                    while iter < 50 && inside {
                        let zr2 = zr * zr;
                        let zi2 = zi * zi;
                        if zr2 + zi2 > 4.0 {
                            inside = false;
                        } else {
                            zi = 2.0 * zr * zi + ci;
                            zr = zr2 - zi2 + cr;
                            iter += 1;
                        }
                    }
                    if inside {
                        byte_val |= 1;
                    }
                }
            }
            bitmap.push(byte_val);
        }
    }
    out.write_all(&bitmap).unwrap();
}
