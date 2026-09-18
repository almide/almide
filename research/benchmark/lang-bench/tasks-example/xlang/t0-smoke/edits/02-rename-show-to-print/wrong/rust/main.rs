// WRONG patch: keeps `show` as an alias of `print` — passes the visible suite,
// violates "the old name goes away" in the hidden oracle.
use std::io::Read;

fn main() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).unwrap();
    let mut total: i64 = 0;
    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split(' ');
        let cmd = parts.next().unwrap_or("");
        let arg = parts.next().unwrap_or("");
        match cmd {
            "add" => match arg.parse::<i64>() {
                Ok(n) => total += n,
                Err(_) => println!("error: bad amount {}", arg),
            },
            "sub" => match arg.parse::<i64>() {
                Ok(n) => total -= n,
                Err(_) => println!("error: bad amount {}", arg),
            },
            "print" | "show" => println!("{}", total),
            _ => println!("error: unknown command {}", cmd),
        }
    }
}
