// float_math decomposition variant A (Rust twin): same loop, compare + constant
// print instead of float formatting.
fn main() {
    let mut i: i64 = 0;
    let mut x: f64 = 1.5;
    while i < 20000000 {
        x = x * 1.0000001 + 0.0000003;
        i += 1;
    }
    if x > 30.0 { println!("gt") } else { println!("le") }
}
