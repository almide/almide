// fasta — reference for fasta.almd: same LCG (seed 42), same threshold trees,
// same section sizes, byte-identical output.

use std::io::Write;

const ALU: &[u8] = b"GGCCGGGCGCGGTGGCTCACGCCTGTAATCCCAGCACTTTGGGAGGCCGAGGCGGGCGGATCACCTGAGGTCAGGAGTTCGAGACCAGCCTGGCCAACATGGTGAAACCCCGTCTCTACTAAAAATACAAAAATTAGCCGGGCGTGGTGGCGCGCGCCTGTAATCCCAGCTACTCGGGAGGCTGAGGCAGGAGAATCGCTTGAACCCGGGAGGCGGAGGTTGCAGTGAGCCGAGATCGCGCCACTGCACTCCAGCCTGGGCGACAGAGCGAGACTCCGTCTCAAAAA";

const IM: i64 = 139968;
const IA: i64 = 3877;
const IC: i64 = 29573;

fn hs_select(r: f64) -> u8 {
    if r < 0.3029549426680 {
        97
    } else if r < 0.5009432431601 {
        99
    } else if r < 0.6984905497992 {
        103
    } else {
        116
    }
}

fn iub_select(r: f64) -> u8 {
    if r < 0.51 {
        if r < 0.27 {
            97
        } else if r < 0.39 {
            99
        } else {
            103
        }
    } else if r < 0.78 {
        116
    } else if r < 0.88 {
        if r < 0.80 {
            66
        } else if r < 0.82 {
            68
        } else if r < 0.84 {
            72
        } else if r < 0.86 {
            75
        } else {
            77
        }
    } else if r < 0.90 {
        78
    } else if r < 0.92 {
        82
    } else if r < 0.94 {
        83
    } else if r < 0.96 {
        86
    } else if r < 0.98 {
        87
    } else {
        89
    }
}

fn main() {
    let n: i64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut seed: i64 = 42;
    let mut buf = Vec::with_capacity(61);

    // ONE: repeat ALU
    out.write_all(b">ONE Homo sapiens alu\n").unwrap();
    let mut pos = 0usize;
    let mut remaining = n * 2;
    while remaining > 0 {
        let line_len = remaining.min(60);
        buf.clear();
        for _ in 0..line_len {
            buf.push(ALU[pos]);
            pos = (pos + 1) % ALU.len();
        }
        buf.push(b'\n');
        out.write_all(&buf).unwrap();
        remaining -= line_len;
    }

    // TWO: IUB ambiguity codes
    out.write_all(b">TWO IUB ambiguity codes\n").unwrap();
    remaining = n * 3;
    while remaining > 0 {
        let line_len = remaining.min(60);
        buf.clear();
        for _ in 0..line_len {
            seed = (seed * IA + IC) % IM;
            let r = seed as f64 / 139968.0;
            buf.push(iub_select(r));
        }
        buf.push(b'\n');
        out.write_all(&buf).unwrap();
        remaining -= line_len;
    }

    // THREE: Homo sapiens frequency
    out.write_all(b">THREE Homo sapiens frequency\n").unwrap();
    remaining = n * 5;
    while remaining > 0 {
        let line_len = remaining.min(60);
        buf.clear();
        for _ in 0..line_len {
            seed = (seed * IA + IC) % IM;
            let r = seed as f64 / 139968.0;
            buf.push(hs_select(r));
        }
        buf.push(b'\n');
        out.write_all(&buf).unwrap();
        remaining -= line_len;
    }
}
