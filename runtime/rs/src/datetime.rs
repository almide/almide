// datetime extern — Rust native implementations (no chrono crate)
// All timestamps are Unix epoch seconds (i64).

pub fn almide_rt_datetime_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64
}

pub fn almide_rt_datetime_year(ts: i64) -> i64 { civil_from_epoch(ts).0 }
pub fn almide_rt_datetime_month(ts: i64) -> i64 { civil_from_epoch(ts).1 }
pub fn almide_rt_datetime_day(ts: i64) -> i64 { civil_from_epoch(ts).2 }
pub fn almide_rt_datetime_hour(ts: i64) -> i64 { ((ts % 86400 + 86400) % 86400) / 3600 }
pub fn almide_rt_datetime_minute(ts: i64) -> i64 { ((ts % 3600 + 3600) % 3600) / 60 }
pub fn almide_rt_datetime_second(ts: i64) -> i64 { ((ts % 60) + 60) % 60 }

pub fn almide_rt_datetime_weekday(ts: i64) -> String {
    let days = ["Thursday", "Friday", "Saturday", "Sunday", "Monday", "Tuesday", "Wednesday"];
    let d = ((ts / 86400) % 7 + 7) % 7;
    days[d as usize].to_string()
}

pub fn almide_rt_datetime_from_parts(y: i64, m: i64, d: i64, h: i64, min: i64, s: i64) -> i64 {
    epoch_from_civil(y, m, d) + h * 3600 + min * 60 + s
}

pub fn almide_rt_datetime_to_iso(ts: i64) -> String {
    let (y, m, d) = civil_from_epoch(ts);
    let h = almide_rt_datetime_hour(ts);
    let mi = almide_rt_datetime_minute(ts);
    let s = almide_rt_datetime_second(ts);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, h, mi, s)
}

pub fn almide_rt_datetime_parse_iso(s: String) -> Result<i64, String> {
    // Minimal ISO 8601: "2024-01-15T10:30:00Z"
    let s = s.trim().trim_end_matches('Z');
    let parts: Vec<&str> = s.split('T').collect();
    if parts.len() != 2 { return Err("expected YYYY-MM-DDTHH:MM:SSZ".into()); }
    let date: Vec<i64> = parts[0].split('-').filter_map(|p| p.parse().ok()).collect();
    let time: Vec<i64> = parts[1].split(':').filter_map(|p| p.parse().ok()).collect();
    if date.len() != 3 || time.len() != 3 { return Err("invalid datetime format".into()); }
    Ok(almide_rt_datetime_from_parts(date[0], date[1], date[2], time[0], time[1], time[2]))
}

pub fn almide_rt_datetime_format(ts: i64, pattern: String) -> String {
    let (y, m, d) = civil_from_epoch(ts);
    let h = almide_rt_datetime_hour(ts);
    let mi = almide_rt_datetime_minute(ts);
    let s = almide_rt_datetime_second(ts);
    pattern
        .replace("YYYY", &format!("{:04}", y))
        .replace("MM", &format!("{:02}", m))
        .replace("DD", &format!("{:02}", d))
        .replace("HH", &format!("{:02}", h))
        .replace("mm", &format!("{:02}", mi))
        .replace("ss", &format!("{:02}", s))
}

pub fn almide_rt_datetime_add_days(ts: i64, n: i64) -> i64 { ts + n * 86400 }
pub fn almide_rt_datetime_add_hours(ts: i64, n: i64) -> i64 { ts + n * 3600 }
pub fn almide_rt_datetime_add_minutes(ts: i64, n: i64) -> i64 { ts + n * 60 }
pub fn almide_rt_datetime_add_seconds(ts: i64, n: i64) -> i64 { ts + n }
pub fn almide_rt_datetime_diff_seconds(a: i64, b: i64) -> i64 { a - b }
pub fn almide_rt_datetime_is_before(a: i64, b: i64) -> bool { a < b }
pub fn almide_rt_datetime_is_after(a: i64, b: i64) -> bool { a > b }
pub fn almide_rt_datetime_from_unix(seconds: i64) -> i64 { seconds }
pub fn almide_rt_datetime_to_unix(ts: i64) -> i64 { ts }

// Civil date ↔ epoch conversion (Howard Hinnant's algorithm)
fn civil_from_epoch(ts: i64) -> (i64, i64, i64) {
    let z = ts / 86400 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as i64, d as i64)
}

fn epoch_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let m = m as u64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146097 + doe as i64 - 719468) * 86400
}
