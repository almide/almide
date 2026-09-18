// A running total driven by one command per stdin line.
//   add <n>   adds n to the total
//   sub <n>   subtracts n from the total
//   print     prints the total
// Any other line is reported as an error and skipped.
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
            "print" => println!("{}", total),
            _ => println!("error: unknown command {}", cmd),
        }
    }
}
