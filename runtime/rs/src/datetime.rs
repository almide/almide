// datetime extern — Rust native implementations (no chrono crate)
// All timestamps are Unix epoch seconds (i64).

pub fn almide_rt_datetime_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64
}

pub fn almide_rt_datetime_now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64
}

/// Monotonic nanosecond clock for benchmarking. Returns nanoseconds
/// since an unspecified reference point; only differences are
/// meaningful. Never goes backwards, unlike `now()` which follows
/// wall-clock adjustments.
pub fn almide_rt_datetime_monotonic_ns() -> i64 {
    use std::time::Instant;
    use std::sync::OnceLock;
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    let start = ORIGIN.get_or_init(Instant::now);
    start.elapsed().as_nanos() as i64
}

pub fn almide_rt_datetime_year(ts: i64) -> i64 { almide_rt_civil_from_epoch(ts).0 }
pub fn almide_rt_datetime_month(ts: i64) -> i64 { almide_rt_civil_from_epoch(ts).1 }
pub fn almide_rt_datetime_day(ts: i64) -> i64 { almide_rt_civil_from_epoch(ts).2 }
pub fn almide_rt_datetime_hour(ts: i64) -> i64 { ((ts % 86400 + 86400) % 86400) / 3600 }
pub fn almide_rt_datetime_minute(ts: i64) -> i64 { ((ts % 3600 + 3600) % 3600) / 60 }
pub fn almide_rt_datetime_second(ts: i64) -> i64 { ((ts % 60) + 60) % 60 }

pub fn almide_rt_datetime_weekday(ts: i64) -> AlmideString {
    let __almide_out: String = {
        let days = ["Thursday", "Friday", "Saturday", "Sunday", "Monday", "Tuesday", "Wednesday"];
        let d = almide_rt_days_from_epoch(ts).rem_euclid(7);
        days[d as usize].to_string()
    };
    __almide_out.into()
}

/// The UTC instant of six civil fields, or the first field out of range by
/// name (#4149): year 0..=9999, month 1..=12, day 1..=days-in-month, hour
/// 0..=23, minute and second 0..=59 — `parse_iso`'s rules and messages, which
/// it reads from here, so the two constructors agree on what a date is.
/// Before this, `from_parts(2026, 2, 30, …)` rolled over to 2026-03-02 while
/// `parse_iso` refused the same fields.
pub fn almide_rt_datetime_from_parts(y: i64, m: i64, d: i64, h: i64, min: i64, s: i64) -> Result<i64, AlmideString> {
    almide_rt_datetime_civil_fields(y, m, d, h, min, s).map_err(Into::into)
}

/// The one range check of both constructors, then the exact arithmetic (every
/// field bounded, so no step can wrap).
fn almide_rt_datetime_civil_fields(y: i64, mo: i64, dd: i64, h: i64, mi: i64, sec: i64) -> Result<i64, String> {
    if !(0..=9999).contains(&y) { return Err(format!("year out of range: {y}")); }
    if !(1..=12).contains(&mo) { return Err(format!("month out of range: {mo}")); }
    if dd < 1 || dd > almide_rt_days_in_month(y, mo) { return Err(format!("day out of range: {dd}")); }
    if !(0..=23).contains(&h) { return Err(format!("hour out of range: {h}")); }
    if !(0..=59).contains(&mi) { return Err(format!("minute out of range: {mi}")); }
    if !(0..=59).contains(&sec) { return Err(format!("second out of range: {sec}")); }
    Ok(almide_rt_epoch_from_civil(y, mo, dd) + h * 3600 + mi * 60 + sec)
}

pub fn almide_rt_datetime_to_iso(ts: i64) -> AlmideString {
    let __almide_out: String = {
        let (y, m, d) = almide_rt_civil_from_epoch(ts);
        let h = almide_rt_datetime_hour(ts);
        let mi = almide_rt_datetime_minute(ts);
        let s = almide_rt_datetime_second(ts);
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, h, mi, s)
    };
    __almide_out.into()
}

/// `YYYY-MM-DDTHH:MM:SS` followed by `Z`, `+HH:MM`, `-HH:MM` or nothing (UTC).
/// Every field is ASCII digits of any width (`2024-1-5T1:2:3` is accepted; a
/// sign, a space or a letter is not), exactly three date and three time
/// fields; year 0..=9999 (RFC 3339 `date-fullyear = 4DIGIT`), month 1..=12, day 1..=days-in-month (proleptic Gregorian), hour
/// 0..=23, minute and second 0..=59 (no leap second), offset hours 0..=23 and
/// minutes 0..=59. The offset is APPLIED — the answer is the UTC instant, so
/// `…T10:30:00+09:00` equals `…T01:30:00Z`.
///
/// Before #2489 this dropped every part `parse` refused (`filter_map`) and fed
/// the rest to `from_parts`, which rolls over: `+09:00` was silently ignored
/// (`00+09` failed to parse and vanished), and month 13 / day 45 / hour 99
/// were "ok" — February of the next year. The self-host twin is
/// stdlib/datetime_parse_iso.almd; the two must answer the same bytes.
///
/// The year bound (#4118) is what keeps the arithmetic exact: before it,
/// `584554051223-01-01T00:00:00Z` overflowed `epoch_from_civil` and answered
/// ok(-26982016) — a day in 1969. With every field bounded (year ≤ 9999,
/// offset < 1 day) the largest magnitude reached is ~3.2e11 seconds, so no
/// step below can wrap on either leg; an out-of-range year is an `err` that
/// names it, checked first, before the fields whose range depends on it.
pub fn almide_rt_datetime_parse_iso(s: &str) -> Result<i64, AlmideString> {
    let __almide_r: Result<i64, String> = (|| {
        let t = s.trim();
        let mut halves = t.split('T');
        let (Some(date), Some(time_raw), None) = (halves.next(), halves.next(), halves.next()) else {
            return Err("expected YYYY-MM-DDTHH:MM:SSZ".into());
        };
        // Digits-only fields: one refused part refuses the whole string.
        fn fields(half: &str, sep: char) -> Option<Vec<i64>> {
            half.split(sep)
                .map(|p| if almide_rt_iso_digits(p) { p.parse::<i64>().ok() } else { None })
                .collect()
        }
        let (time_z, had_z) = match time_raw.strip_suffix('Z') {
            Some(x) => (x, true),
            None => (time_raw, false),
        };
        let sign_at = time_z.find(['+', '-']);
        // `Z` and an offset are exclusive spellings of the same thing.
        if had_z && sign_at.is_some() { return Err("invalid datetime format".into()); }
        let hms = sign_at.map_or(time_z, |i| &time_z[..i]);
        let (Some(d), Some(tm)) = (fields(date, '-'), fields(hms, ':')) else {
            return Err("invalid datetime format".into());
        };
        if d.len() != 3 || tm.len() != 3 { return Err("invalid datetime format".into()); }
        let offset = match sign_at {
            None => 0,
            Some(i) => {
                let raw = &time_z[i..];
                match fields(&time_z[i + 1..], ':').as_deref() {
                    Some([oh, om]) if (0..=23).contains(oh) && (0..=59).contains(om) => {
                        let secs = oh * 3600 + om * 60;
                        if raw.starts_with('-') { -secs } else { secs }
                    }
                    _ => return Err(format!("invalid offset: {}", almide_rt_errtext_escape(raw))),
                }
            }
        };
        let (y, mo, dd) = (d[0], d[1], d[2]);
        let (h, mi, sec) = (tm[0], tm[1], tm[2]);
        Ok(almide_rt_datetime_civil_fields(y, mo, dd, h, mi, sec)? - offset)
    })();
    __almide_r.map_err(Into::into)
}

// `string.is_digit`'s rule (runtime/rs/src/string.rs): non-empty, ASCII digits only.
fn almide_rt_iso_digits(p: &str) -> bool { !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) }

fn almide_rt_days_in_month(y: i64, m: i64) -> i64 {
    // Proleptic Gregorian; `%` on an exact multiple is 0 for either sign.
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    if m == 2 { if leap { 29 } else { 28 } }
    else if m == 4 || m == 6 || m == 9 || m == 11 { 30 }
    else { 31 }
}

pub fn almide_rt_datetime_format(ts: i64, pattern: &str) -> AlmideString {
    let __almide_out: String = {
        let (y, m, d) = almide_rt_civil_from_epoch(ts);
        let h = almide_rt_datetime_hour(ts);
        let mi = almide_rt_datetime_minute(ts);
        let s = almide_rt_datetime_second(ts);
        // strftime-style specifiers, substituted sequentially. The wasm + self-hosted
        // backends run the identical sequence, so datetime.format is byte-identical
        // across targets. `%` is special only before one of these specifiers; there is
        // no `%%` escape (see docs/stdlib/datetime.md).
        pattern
            .replace("%Y", &format!("{:04}", y))
            .replace("%m", &format!("{:02}", m))
            .replace("%d", &format!("{:02}", d))
            .replace("%H", &format!("{:02}", h))
            .replace("%M", &format!("{:02}", mi))
            .replace("%S", &format!("{:02}", s))
    };
    __almide_out.into()
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

// The day number of a timestamp, FLOORED (#2488): `ts / 86400` truncates toward
// zero, so every pre-epoch second that is not on a day boundary landed on the
// day AFTER its own — `to_iso(-1)` was `1970-01-01T23:59:59Z`, `weekday(-1)`
// was Thursday. The time-of-day extractors always floor-modded (`(x % m + m)
// % m` = rem_euclid); the day number must floor the same way or the two halves
// describe different days. `div_euclid` floors for a positive divisor and
// cannot overflow (only a divisor of -1 can). The self-host twin is
// `__c_days` in stdlib/datetime_calendar.almd.
fn almide_rt_days_from_epoch(ts: i64) -> i64 { ts.div_euclid(86400) }

// Civil date ↔ epoch conversion (Howard Hinnant's algorithm)
fn almide_rt_civil_from_epoch(ts: i64) -> (i64, i64, i64) {
    let z = almide_rt_days_from_epoch(ts) + 719468;
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

// Hinnant's civil-from-days, in i64 THROUGHOUT. The `as u64` casts this replaces were
// safe for a valid 1..=12 month — yoe is 0..399 there — but they are an UNSIGNED step in
// the middle of a signed calendar, and `from_parts(2020, -1, 1, …)` turned the month into
// 18446744073709551615: `153 * (m - 3)` then wrapped, and this leg answered 1540840320
// where the self-host, which had always been i64, answered 1572566400. Neither number
// means anything for month -1; the point is that they must be the SAME nothing.
fn almide_rt_epoch_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146097 + doe - 719468) * 86400
}
