// FFT — reference for fft.almd: same iterative Cooley-Tukey on an interleaved
// Vec<f64>, same input signal, same output format (line 2 is self-timed).

fn main() {
    let size: u32 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);

    let n = 1usize << size;
    let pi = 3.14159265358979323846f64;

    let mut data = Vec::with_capacity(2 * n);
    for i in 0..n {
        let theta = i as f64 / n as f64 * pi;
        let re = 1.0 * (10.0 * theta).cos() + 0.5 * (25.0 * theta).cos();
        let im = 1.0 * (10.0 * theta).sin() + 0.5 * (25.0 * theta).sin();
        data.push((re * 100.0).round() / 100.0);
        data.push((im * 100.0).round() / 100.0);
    }

    let start = std::time::Instant::now();

    // Bit-reversal permutation
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n / 2;
        while bit > 0 && j >= bit {
            j -= bit;
            bit /= 2;
        }
        j += bit;
        if i < j {
            data.swap(2 * i, 2 * j);
            data.swap(2 * i + 1, 2 * j + 1);
        }
    }

    // Butterfly passes
    let mut len = 2usize;
    while len <= n {
        let ang = -2.0 * pi / len as f64;
        let wn_re = ang.cos();
        let wn_im = ang.sin();
        let mut i = 0usize;
        while i < n {
            let mut w_re = 1.0f64;
            let mut w_im = 0.0f64;
            for k in 0..len / 2 {
                let ui = 2 * (i + k);
                let vi = 2 * (i + k + len / 2);
                let u_re = data[ui];
                let u_im = data[ui + 1];
                let vr = data[vi];
                let vim = data[vi + 1];
                let v_re = w_re * vr - w_im * vim;
                let v_im = w_re * vim + w_im * vr;
                data[ui] = u_re + v_re;
                data[ui + 1] = u_im + v_im;
                data[vi] = u_re - v_re;
                data[vi + 1] = u_im - v_im;
                let new_w_re = w_re * wn_re - w_im * wn_im;
                let new_w_im = w_re * wn_im + w_im * wn_re;
                w_re = new_w_re;
                w_im = new_w_im;
            }
            i += len;
        }
        len *= 2;
    }

    // Normalize
    let factor = 1.0 / (n as f64).sqrt();
    for x in data.iter_mut() {
        *x *= factor;
    }

    let elapsed = start.elapsed().as_millis();
    println!("size: 2^{} = {}", size, n);
    println!("execution time: {} ms", elapsed);
    // Keep `data` observable so the optimizer cannot delete the transform.
    std::hint::black_box(&data);
}
