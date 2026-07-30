// N-Body, fully-unrolled scalar locals — same program shape as nbody.almd,
// to isolate Almide codegen overhead from data-layout differences.

const PI: f64 = 3.141592653589793;
const SOLAR_MASS: f64 = 4.0 * PI * PI;
const DAYS_PER_YEAR: f64 = 365.24;

macro_rules! interact {
    ($dt:expr, $ax:ident,$ay:ident,$az:ident, $avx:ident,$avy:ident,$avz:ident, $am:expr,
                $bx:ident,$by:ident,$bz:ident, $bvx:ident,$bvy:ident,$bvz:ident, $bm:expr) => {{
        let dx = $ax - $bx;
        let dy = $ay - $by;
        let dz = $az - $bz;
        let dsq = dx * dx + dy * dy + dz * dz;
        let inv = 1.0 / dsq.sqrt();
        let mag = $dt * inv * inv * inv;
        $avx -= dx * $bm * mag;
        $avy -= dy * $bm * mag;
        $avz -= dz * $bm * mag;
        $bvx += dx * $am * mag;
        $bvy += dy * $am * mag;
        $bvz += dz * $am * mag;
    }};
}

fn dist(x1: f64, y1: f64, z1: f64, x2: f64, y2: f64, z2: f64) -> f64 {
    let dx = x1 - x2;
    let dy = y1 - y2;
    let dz = z1 - z2;
    (dx * dx + dy * dy + dz * dz).sqrt()
}

#[allow(clippy::too_many_arguments)]
fn energy(
    s_x: f64, s_y: f64, s_z: f64, s_vx: f64, s_vy: f64, s_vz: f64,
    j_x: f64, j_y: f64, j_z: f64, j_vx: f64, j_vy: f64, j_vz: f64, j_m: f64,
    a_x: f64, a_y: f64, a_z: f64, a_vx: f64, a_vy: f64, a_vz: f64, a_m: f64,
    u_x: f64, u_y: f64, u_z: f64, u_vx: f64, u_vy: f64, u_vz: f64, u_m: f64,
    n_x: f64, n_y: f64, n_z: f64, n_vx: f64, n_vy: f64, n_vz: f64, n_m: f64,
) -> f64 {
    let ke = 0.5 * SOLAR_MASS * (s_vx * s_vx + s_vy * s_vy + s_vz * s_vz)
        + 0.5 * j_m * (j_vx * j_vx + j_vy * j_vy + j_vz * j_vz)
        + 0.5 * a_m * (a_vx * a_vx + a_vy * a_vy + a_vz * a_vz)
        + 0.5 * u_m * (u_vx * u_vx + u_vy * u_vy + u_vz * u_vz)
        + 0.5 * n_m * (n_vx * n_vx + n_vy * n_vy + n_vz * n_vz);
    let pe = SOLAR_MASS * j_m / dist(s_x, s_y, s_z, j_x, j_y, j_z)
        + SOLAR_MASS * a_m / dist(s_x, s_y, s_z, a_x, a_y, a_z)
        + SOLAR_MASS * u_m / dist(s_x, s_y, s_z, u_x, u_y, u_z)
        + SOLAR_MASS * n_m / dist(s_x, s_y, s_z, n_x, n_y, n_z)
        + j_m * a_m / dist(j_x, j_y, j_z, a_x, a_y, a_z)
        + j_m * u_m / dist(j_x, j_y, j_z, u_x, u_y, u_z)
        + j_m * n_m / dist(j_x, j_y, j_z, n_x, n_y, n_z)
        + a_m * u_m / dist(a_x, a_y, a_z, u_x, u_y, u_z)
        + a_m * n_m / dist(a_x, a_y, a_z, n_x, n_y, n_z)
        + u_m * n_m / dist(u_x, u_y, u_z, n_x, n_y, n_z);
    ke - pe
}

fn main() {
    let n: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);

    let mut s_x = 0.0f64; let mut s_y = 0.0f64; let mut s_z = 0.0f64;
    let mut j_x = 4.84143144246472090e+00;
    let mut j_y = -1.16032004402742839e+00;
    let mut j_z = -1.03622044471123109e-01;
    let mut a_x = 8.34336671824457987e+00;
    let mut a_y = 4.12479856412430479e+00;
    let mut a_z = -4.03523417114321381e-01;
    let mut u_x = 1.28943695621391310e+01;
    let mut u_y = -1.51111514016986312e+01;
    let mut u_z = -2.23307578892655734e-01;
    let mut n_x = 1.53796971148509165e+01;
    let mut n_y = -2.59193146099879641e+01;
    let mut n_z = 1.79258772950371181e-01;

    let mut s_vx = 0.0f64; let mut s_vy = 0.0f64; let mut s_vz = 0.0f64;
    let mut j_vx = 1.66007664274403694e-03 * DAYS_PER_YEAR;
    let mut j_vy = 7.69901118419740425e-03 * DAYS_PER_YEAR;
    let mut j_vz = -6.90460016972063023e-05 * DAYS_PER_YEAR;
    let mut a_vx = -2.76742510726862411e-03 * DAYS_PER_YEAR;
    let mut a_vy = 4.99852801234917238e-03 * DAYS_PER_YEAR;
    let mut a_vz = 2.30417297573763929e-05 * DAYS_PER_YEAR;
    let mut u_vx = 2.96460137564761618e-03 * DAYS_PER_YEAR;
    let mut u_vy = 2.37847173959480950e-03 * DAYS_PER_YEAR;
    let mut u_vz = -2.96589568540237556e-05 * DAYS_PER_YEAR;
    let mut n_vx = 2.68067772490389322e-03 * DAYS_PER_YEAR;
    let mut n_vy = 1.62824170038242295e-03 * DAYS_PER_YEAR;
    let mut n_vz = -9.51592254519715870e-05 * DAYS_PER_YEAR;

    let j_m = 9.54791938424326609e-04 * SOLAR_MASS;
    let a_m = 2.85885980666130812e-04 * SOLAR_MASS;
    let u_m = 4.36624404335156298e-05 * SOLAR_MASS;
    let n_m = 5.15138902046611451e-05 * SOLAR_MASS;

    s_vx = -(j_vx * j_m + a_vx * a_m + u_vx * u_m + n_vx * n_m) / SOLAR_MASS;
    s_vy = -(j_vy * j_m + a_vy * a_m + u_vy * u_m + n_vy * n_m) / SOLAR_MASS;
    s_vz = -(j_vz * j_m + a_vz * a_m + u_vz * u_m + n_vz * n_m) / SOLAR_MASS;

    println!("{:.9}", energy(
        s_x, s_y, s_z, s_vx, s_vy, s_vz,
        j_x, j_y, j_z, j_vx, j_vy, j_vz, j_m,
        a_x, a_y, a_z, a_vx, a_vy, a_vz, a_m,
        u_x, u_y, u_z, u_vx, u_vy, u_vz, u_m,
        n_x, n_y, n_z, n_vx, n_vy, n_vz, n_m,
    ));

    let dt = 0.01;
    for _ in 0..n {
        interact!(dt, s_x,s_y,s_z, s_vx,s_vy,s_vz, SOLAR_MASS, j_x,j_y,j_z, j_vx,j_vy,j_vz, j_m);
        interact!(dt, s_x,s_y,s_z, s_vx,s_vy,s_vz, SOLAR_MASS, a_x,a_y,a_z, a_vx,a_vy,a_vz, a_m);
        interact!(dt, s_x,s_y,s_z, s_vx,s_vy,s_vz, SOLAR_MASS, u_x,u_y,u_z, u_vx,u_vy,u_vz, u_m);
        interact!(dt, s_x,s_y,s_z, s_vx,s_vy,s_vz, SOLAR_MASS, n_x,n_y,n_z, n_vx,n_vy,n_vz, n_m);
        interact!(dt, j_x,j_y,j_z, j_vx,j_vy,j_vz, j_m, a_x,a_y,a_z, a_vx,a_vy,a_vz, a_m);
        interact!(dt, j_x,j_y,j_z, j_vx,j_vy,j_vz, j_m, u_x,u_y,u_z, u_vx,u_vy,u_vz, u_m);
        interact!(dt, j_x,j_y,j_z, j_vx,j_vy,j_vz, j_m, n_x,n_y,n_z, n_vx,n_vy,n_vz, n_m);
        interact!(dt, a_x,a_y,a_z, a_vx,a_vy,a_vz, a_m, u_x,u_y,u_z, u_vx,u_vy,u_vz, u_m);
        interact!(dt, a_x,a_y,a_z, a_vx,a_vy,a_vz, a_m, n_x,n_y,n_z, n_vx,n_vy,n_vz, n_m);
        interact!(dt, u_x,u_y,u_z, u_vx,u_vy,u_vz, u_m, n_x,n_y,n_z, n_vx,n_vy,n_vz, n_m);

        s_x += dt * s_vx; s_y += dt * s_vy; s_z += dt * s_vz;
        j_x += dt * j_vx; j_y += dt * j_vy; j_z += dt * j_vz;
        a_x += dt * a_vx; a_y += dt * a_vy; a_z += dt * a_vz;
        u_x += dt * u_vx; u_y += dt * u_vy; u_z += dt * u_vz;
        n_x += dt * n_vx; n_y += dt * n_vy; n_z += dt * n_vz;
    }

    println!("{:.9}", energy(
        s_x, s_y, s_z, s_vx, s_vy, s_vz,
        j_x, j_y, j_z, j_vx, j_vy, j_vz, j_m,
        a_x, a_y, a_z, a_vx, a_vy, a_vz, a_m,
        u_x, u_y, u_z, u_vx, u_vy, u_vz, u_m,
        n_x, n_y, n_z, n_vx, n_vy, n_vz, n_m,
    ));
}
