// uuid extern — the one piece of uuid that is not a pure function of its
// inputs: the process-wide v7 clock (RFC 9562 §6.2 method 1). Every other
// uuid function is Almide in stdlib/uuid.almd.

// The last stamp handed out, `ms * 4096 + counter`. A new millisecond restarts
// the counter at 0; a stamp at or below the last one (the same millisecond, or
// a wall clock stepped back) takes the last one plus 1, which carries into the
// next millisecond past counter 4095. So the stamps strictly increase over the
// whole process, whatever the clock does.
static ALMIDE_UUID_V7_LAST: std::sync::Mutex<i64> = std::sync::Mutex::new(i64::MIN);

pub fn almide_rt_uuid___v7_stamp(ms: i64) -> i64 {
    let fresh = ms.max(0).min((1i64 << 48) - 1).saturating_mul(4096);
    let mut last = ALMIDE_UUID_V7_LAST.lock().unwrap_or_else(|p| p.into_inner());
    let next = if fresh > *last { fresh } else { last.saturating_add(1) };
    *last = next;
    next
}
