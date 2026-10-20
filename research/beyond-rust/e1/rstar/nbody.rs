// E1 n-body, R*: expert Rust, same algorithm and the same float operations in
// the same order as nbody.almd (no reassociation, no FMA), so the output is
// byte-identical to A0.
//
// What the expert does: positions do not change while velocities are updated,
// so the ten square roots and divides (the latency-bound part) are independent;
// they run as three 4-lane groups ([f64; 4], padded to 12 with 1.0), and the
// rest stays scalar in nbody.almd's order.
// Usage: nbody <steps>

const PI: f64 = 3.141592653589793;
const SOLAR_MASS: f64 = 4.0 * PI * PI;
const DAYS_PER_YEAR: f64 = 365.24;
const P: usize = 12; // 10 pairs, padded
const PA: [usize; 10] = [0, 0, 0, 0, 1, 1, 1, 2, 2, 3];
const PB: [usize; 10] = [1, 2, 3, 4, 2, 3, 4, 3, 4, 4];

struct Sys {
    x: [f64; 5],
    y: [f64; 5],
    z: [f64; 5],
    vx: [f64; 5],
    vy: [f64; 5],
    vz: [f64; 5],
    m: [f64; 5],
}

#[inline(always)]
fn advance(s: &mut Sys, dt: f64) {
    // 1. pair deltas and squared distances (scalar; positions are fixed during the step)
    let mut d2 = [1.0f64; P];
    let (mut dx, mut dy, mut dz) = ([0.0f64; P], [0.0f64; P], [0.0f64; P]);
    dx[0] = s.x[0] - s.x[1]; dy[0] = s.y[0] - s.y[1]; dz[0] = s.z[0] - s.z[1];
    d2[0] = dx[0] * dx[0] + dy[0] * dy[0] + dz[0] * dz[0];
    dx[1] = s.x[0] - s.x[2]; dy[1] = s.y[0] - s.y[2]; dz[1] = s.z[0] - s.z[2];
    d2[1] = dx[1] * dx[1] + dy[1] * dy[1] + dz[1] * dz[1];
    dx[2] = s.x[0] - s.x[3]; dy[2] = s.y[0] - s.y[3]; dz[2] = s.z[0] - s.z[3];
    d2[2] = dx[2] * dx[2] + dy[2] * dy[2] + dz[2] * dz[2];
    dx[3] = s.x[0] - s.x[4]; dy[3] = s.y[0] - s.y[4]; dz[3] = s.z[0] - s.z[4];
    d2[3] = dx[3] * dx[3] + dy[3] * dy[3] + dz[3] * dz[3];
    dx[4] = s.x[1] - s.x[2]; dy[4] = s.y[1] - s.y[2]; dz[4] = s.z[1] - s.z[2];
    d2[4] = dx[4] * dx[4] + dy[4] * dy[4] + dz[4] * dz[4];
    dx[5] = s.x[1] - s.x[3]; dy[5] = s.y[1] - s.y[3]; dz[5] = s.z[1] - s.z[3];
    d2[5] = dx[5] * dx[5] + dy[5] * dy[5] + dz[5] * dz[5];
    dx[6] = s.x[1] - s.x[4]; dy[6] = s.y[1] - s.y[4]; dz[6] = s.z[1] - s.z[4];
    d2[6] = dx[6] * dx[6] + dy[6] * dy[6] + dz[6] * dz[6];
    dx[7] = s.x[2] - s.x[3]; dy[7] = s.y[2] - s.y[3]; dz[7] = s.z[2] - s.z[3];
    d2[7] = dx[7] * dx[7] + dy[7] * dy[7] + dz[7] * dz[7];
    dx[8] = s.x[2] - s.x[4]; dy[8] = s.y[2] - s.y[4]; dz[8] = s.z[2] - s.z[4];
    d2[8] = dx[8] * dx[8] + dy[8] * dy[8] + dz[8] * dz[8];
    dx[9] = s.x[3] - s.x[4]; dy[9] = s.y[3] - s.y[4]; dz[9] = s.z[3] - s.z[4];
    d2[9] = dx[9] * dx[9] + dy[9] * dy[9] + dz[9] * dz[9];
    // 2. the ten sqrt / divide / mag chains, independent: 3 x 4 SIMD lanes
    let mut mag = [0.0f64; P];
    for c in 0..P / 4 {
        let mut d = [0.0f64; 4];
        d.copy_from_slice(&d2[c * 4..c * 4 + 4]);
        let mut m = [0.0f64; 4];
        for l in 0..4 {
            let inv = 1.0 / d[l].sqrt();
            m[l] = dt * inv * inv * inv;
        }
        mag[c * 4..c * 4 + 4].copy_from_slice(&m);
    }
    // 3. velocity updates in nbody.almd's pair order (sequential per body)
    { let (ma, mb, g) = (s.m[0], s.m[1], mag[0]);
      s.vx[0] = s.vx[0] - dx[0] * mb * g; s.vx[1] = s.vx[1] + dx[0] * ma * g;
      s.vy[0] = s.vy[0] - dy[0] * mb * g; s.vy[1] = s.vy[1] + dy[0] * ma * g;
      s.vz[0] = s.vz[0] - dz[0] * mb * g; s.vz[1] = s.vz[1] + dz[0] * ma * g;
    }
    { let (ma, mb, g) = (s.m[0], s.m[2], mag[1]);
      s.vx[0] = s.vx[0] - dx[1] * mb * g; s.vx[2] = s.vx[2] + dx[1] * ma * g;
      s.vy[0] = s.vy[0] - dy[1] * mb * g; s.vy[2] = s.vy[2] + dy[1] * ma * g;
      s.vz[0] = s.vz[0] - dz[1] * mb * g; s.vz[2] = s.vz[2] + dz[1] * ma * g;
    }
    { let (ma, mb, g) = (s.m[0], s.m[3], mag[2]);
      s.vx[0] = s.vx[0] - dx[2] * mb * g; s.vx[3] = s.vx[3] + dx[2] * ma * g;
      s.vy[0] = s.vy[0] - dy[2] * mb * g; s.vy[3] = s.vy[3] + dy[2] * ma * g;
      s.vz[0] = s.vz[0] - dz[2] * mb * g; s.vz[3] = s.vz[3] + dz[2] * ma * g;
    }
    { let (ma, mb, g) = (s.m[0], s.m[4], mag[3]);
      s.vx[0] = s.vx[0] - dx[3] * mb * g; s.vx[4] = s.vx[4] + dx[3] * ma * g;
      s.vy[0] = s.vy[0] - dy[3] * mb * g; s.vy[4] = s.vy[4] + dy[3] * ma * g;
      s.vz[0] = s.vz[0] - dz[3] * mb * g; s.vz[4] = s.vz[4] + dz[3] * ma * g;
    }
    { let (ma, mb, g) = (s.m[1], s.m[2], mag[4]);
      s.vx[1] = s.vx[1] - dx[4] * mb * g; s.vx[2] = s.vx[2] + dx[4] * ma * g;
      s.vy[1] = s.vy[1] - dy[4] * mb * g; s.vy[2] = s.vy[2] + dy[4] * ma * g;
      s.vz[1] = s.vz[1] - dz[4] * mb * g; s.vz[2] = s.vz[2] + dz[4] * ma * g;
    }
    { let (ma, mb, g) = (s.m[1], s.m[3], mag[5]);
      s.vx[1] = s.vx[1] - dx[5] * mb * g; s.vx[3] = s.vx[3] + dx[5] * ma * g;
      s.vy[1] = s.vy[1] - dy[5] * mb * g; s.vy[3] = s.vy[3] + dy[5] * ma * g;
      s.vz[1] = s.vz[1] - dz[5] * mb * g; s.vz[3] = s.vz[3] + dz[5] * ma * g;
    }
    { let (ma, mb, g) = (s.m[1], s.m[4], mag[6]);
      s.vx[1] = s.vx[1] - dx[6] * mb * g; s.vx[4] = s.vx[4] + dx[6] * ma * g;
      s.vy[1] = s.vy[1] - dy[6] * mb * g; s.vy[4] = s.vy[4] + dy[6] * ma * g;
      s.vz[1] = s.vz[1] - dz[6] * mb * g; s.vz[4] = s.vz[4] + dz[6] * ma * g;
    }
    { let (ma, mb, g) = (s.m[2], s.m[3], mag[7]);
      s.vx[2] = s.vx[2] - dx[7] * mb * g; s.vx[3] = s.vx[3] + dx[7] * ma * g;
      s.vy[2] = s.vy[2] - dy[7] * mb * g; s.vy[3] = s.vy[3] + dy[7] * ma * g;
      s.vz[2] = s.vz[2] - dz[7] * mb * g; s.vz[3] = s.vz[3] + dz[7] * ma * g;
    }
    { let (ma, mb, g) = (s.m[2], s.m[4], mag[8]);
      s.vx[2] = s.vx[2] - dx[8] * mb * g; s.vx[4] = s.vx[4] + dx[8] * ma * g;
      s.vy[2] = s.vy[2] - dy[8] * mb * g; s.vy[4] = s.vy[4] + dy[8] * ma * g;
      s.vz[2] = s.vz[2] - dz[8] * mb * g; s.vz[4] = s.vz[4] + dz[8] * ma * g;
    }
    { let (ma, mb, g) = (s.m[3], s.m[4], mag[9]);
      s.vx[3] = s.vx[3] - dx[9] * mb * g; s.vx[4] = s.vx[4] + dx[9] * ma * g;
      s.vy[3] = s.vy[3] - dy[9] * mb * g; s.vy[4] = s.vy[4] + dy[9] * ma * g;
      s.vz[3] = s.vz[3] - dz[9] * mb * g; s.vz[4] = s.vz[4] + dz[9] * ma * g;
    }
    for k in 0..5 {
        s.x[k] = s.x[k] + dt * s.vx[k];
        s.y[k] = s.y[k] + dt * s.vy[k];
        s.z[k] = s.z[k] + dt * s.vz[k];
    }
}

fn dist(s: &Sys, a: usize, b: usize) -> f64 {
    let dx = s.x[a] - s.x[b];
    let dy = s.y[a] - s.y[b];
    let dz = s.z[a] - s.z[b];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn energy(s: &Sys) -> f64 {
    let ke = |k: usize| 0.5 * s.m[k] * (s.vx[k] * s.vx[k] + s.vy[k] * s.vy[k] + s.vz[k] * s.vz[k]);
    let ke = ke(0) + ke(1) + ke(2) + ke(3) + ke(4);
    let mut pe = s.m[0] * s.m[1] / dist(s, 0, 1);
    for p in 1..10 {
        pe = pe + s.m[PA[p]] * s.m[PB[p]] / dist(s, PA[p], PB[p]);
    }
    ke - pe
}

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1000);
    let m = [
        SOLAR_MASS,
        9.54791938424326609e-04 * SOLAR_MASS,
        2.85885980666130812e-04 * SOLAR_MASS,
        4.36624404335156298e-05 * SOLAR_MASS,
        5.15138902046611451e-05 * SOLAR_MASS,
    ];
    let mut s = Sys {
        x: [0.0, 4.84143144246472090e+00, 8.34336671824457987e+00, 1.28943695621391310e+01, 1.53796971148509165e+01],
        y: [0.0, -1.16032004402742839e+00, 4.12479856412430479e+00, -1.51111514016986312e+01, -2.59193146099879641e+01],
        z: [0.0, -1.03622044471123109e-01, -4.03523417114321381e-01, -2.23307578892655734e-01, 1.79258772950371181e-01],
        vx: [0.0, 1.66007664274403694e-03 * DAYS_PER_YEAR, -2.76742510726862411e-03 * DAYS_PER_YEAR, 2.96460137564761618e-03 * DAYS_PER_YEAR, 2.68067772490389322e-03 * DAYS_PER_YEAR],
        vy: [0.0, 7.69901118419740425e-03 * DAYS_PER_YEAR, 4.99852801234917238e-03 * DAYS_PER_YEAR, 2.37847173959480950e-03 * DAYS_PER_YEAR, 1.62824170038242295e-03 * DAYS_PER_YEAR],
        vz: [0.0, -6.90460016972063023e-05 * DAYS_PER_YEAR, 2.30417297573763929e-05 * DAYS_PER_YEAR, -2.96589568540237556e-05 * DAYS_PER_YEAR, -9.51592254519715870e-05 * DAYS_PER_YEAR],
        m,
    };
    // offset momentum, in nbody.almd's order
    s.vx[0] = 0.0 - (s.vx[1] * m[1] + s.vx[2] * m[2] + s.vx[3] * m[3] + s.vx[4] * m[4]) / SOLAR_MASS;
    s.vy[0] = 0.0 - (s.vy[1] * m[1] + s.vy[2] * m[2] + s.vy[3] * m[3] + s.vy[4] * m[4]) / SOLAR_MASS;
    s.vz[0] = 0.0 - (s.vz[1] * m[1] + s.vz[2] * m[2] + s.vz[3] * m[3] + s.vz[4] * m[4]) / SOLAR_MASS;
    println!("{:.9}", energy(&s));
    for _ in 0..n {
        advance(&mut s, 0.01);
    }
    println!("{:.9}", energy(&s));
}
