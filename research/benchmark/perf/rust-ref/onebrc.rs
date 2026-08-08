// onebrc — handwritten Rust reference, same shape as onebrc/onebrc.almd:
// same LCG, chunked appends (one open per append, like fs.append), an eager
// Vec<String> of lines (like fs.read_lines), integer-tenths math, identical
// output bytes. Streaming-anything is deliberately NOT used here; this ref
// answers "what does the same program cost in Rust", not "what is optimal".

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;

const IM: i64 = 139968;
const IA: i64 = 3877;
const IC: i64 = 29573;
const CHUNK_LINES: usize = 65536;

const STATIONS: [&str; 50] = [
    "Abha", "Accra", "Adelaide", "Amsterdam", "Anchorage", "Athens", "Auckland",
    "Baghdad", "Bangkok", "Barcelona", "Beijing", "Berlin", "Bogota", "Boston",
    "Brisbane", "Cairo", "Calgary", "Chicago", "Copenhagen", "Dallas", "Denver",
    "Dubai", "Dublin", "Helsinki", "Houston", "Jakarta", "Karachi", "Kingston",
    "Lagos", "Lima", "Lisbon", "London", "Madrid", "Melbourne", "Mexico City",
    "Miami", "Moscow", "Mumbai", "Nairobi", "Oslo", "Paris", "Perth", "Prague",
    "Rome", "Seattle", "Seoul", "Singapore", "Stockholm", "Sydney", "Tokyo",
];

struct Stats {
    min: i64,
    max: i64,
    sum: i64,
    count: i64,
}

fn fmt_tenths(t: i64) -> String {
    if t < 0 {
        format!("-{}", fmt_tenths(-t))
    } else {
        format!("{}.{}", t / 10, t % 10)
    }
}

fn parse_tenths(s: &str) -> i64 {
    if let Some(rest) = s.strip_prefix('-') {
        return -parse_tenths(rest);
    }
    let (whole, frac) = s.split_once('.').unwrap_or((s, "0"));
    whole.parse::<i64>().unwrap_or(0) * 10 + frac.parse::<i64>().unwrap_or(0)
}

fn mean_tenths(sum: i64, count: i64) -> i64 {
    if sum < 0 {
        -((-sum) / count)
    } else {
        sum / count
    }
}

fn append(path: &str, content: &str) {
    let mut f = OpenOptions::new().append(true).open(path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

fn generate(n: i64, path: &str) {
    std::fs::write(path, "").unwrap();
    let n_stations = STATIONS.len() as i64;
    let mut seed: i64 = 42;
    let mut chunk: Vec<String> = Vec::new();
    for _ in 0..n {
        seed = (seed * IA + IC) % IM;
        let station = STATIONS[(seed % n_stations) as usize];
        seed = (seed * IA + IC) % IM;
        let t = (seed % 1999) - 999;
        chunk.push(format!("{};{}", station, fmt_tenths(t)));
        if chunk.len() == CHUNK_LINES {
            append(path, &(chunk.join("\n") + "\n"));
            chunk.clear();
        }
    }
    if !chunk.is_empty() {
        append(path, &(chunk.join("\n") + "\n"));
    }
}

fn aggregate(path: &str) -> String {
    let lines: Vec<String> = std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(String::from)
        .collect();
    let mut stats: HashMap<String, Stats> = HashMap::new();
    for line in &lines {
        if line.is_empty() {
            continue;
        }
        let (station, temp) = line.split_once(';').unwrap();
        let t = parse_tenths(temp);
        match stats.get_mut(station) {
            Some(s) => {
                if t < s.min {
                    s.min = t;
                }
                if t > s.max {
                    s.max = t;
                }
                s.sum += t;
                s.count += 1;
            }
            None => {
                stats.insert(station.to_string(), Stats { min: t, max: t, sum: t, count: 1 });
            }
        }
    }
    let mut names: Vec<&String> = stats.keys().collect();
    names.sort();
    let body = names
        .iter()
        .map(|name| {
            let s = &stats[*name];
            format!(
                "{}={}/{}/{}",
                name,
                fmt_tenths(s.min),
                fmt_tenths(mean_tenths(s.sum, s.count)),
                fmt_tenths(s.max)
            )
        })
        .collect::<Vec<String>>()
        .join(", ");
    format!("{{{}}}", body)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("");
    match mode {
        "gen" => {
            let n: i64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1000);
            let default = "onebrc_data.txt".to_string();
            let path = args.get(2).unwrap_or(&default);
            generate(n, path);
        }
        "agg" => {
            let default = "onebrc_data.txt".to_string();
            let path = args.get(1).unwrap_or(&default);
            println!("{}", aggregate(path));
        }
        first => {
            let n: i64 = first.parse().unwrap_or(1000);
            let path = std::env::temp_dir().join(format!("almide_onebrc_{}.txt", n));
            let path = path.to_str().unwrap();
            generate(n, path);
            println!("{}", aggregate(path));
            std::fs::remove_file(path).unwrap();
        }
    }
}
