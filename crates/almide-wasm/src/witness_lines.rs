//! The certificate-LINE text helpers of the structural witness (#2755,
//! split from witness_paths.rs for the file budget): a line's items, the
//! top-level net count, and the two rewrites that keep folded exit items
//! flat (format v5 arms do not nest) — flattening an exiting arm's nested
//! exits, and hoisting a path's exit items to the line start.

/// Record `c` on a path whose first event was `first` (#3259): a read `b`
/// only on a line born by `i`, an OWNED line. A borrowed param's or a
/// view's line, and a loop activation of a block born outside the loop, sit
/// at 0 while another holder keeps the block, so a read there is no probe.
pub(crate) fn record(events: &mut String, first: &mut Option<char>, c: char) {
    if c == 'b' {
        if *first == Some('i') {
            events.push(c);
        }
        return;
    }
    first.get_or_insert(c);
    events.push(c);
}

/// The net count a path's top-level events leave (a folded `{…|}` item's
/// arms are not the path's own: its surviving side is empty).
pub(crate) fn net(events: &str) -> i64 {
    let mut depth = 0;
    let mut n = 0;
    for c in events.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            'i' | 'a' if depth == 0 => n += 1,
            'd' | 'm' | 'r' if depth == 0 => n -= 1,
            _ => {}
        }
    }
    n
}

/// The items of a line: a folded `{…}` group or one event byte.
pub(crate) fn items(events: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    for (i, c) in events.char_indices() {
        match c {
            '{' => {
                if depth == 0 {
                    start = i;
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    out.push(&events[start..=i]);
                }
            }
            _ if depth == 0 => out.push(&events[i..i + c.len_utf8()]),
            _ => {}
        }
    }
    out
}

/// A folded path that ENDED (`aborted`: in an abort, else an exit), as the
/// flat exit paths it stands for: each folded `{<e><m>|}` item is the path
/// `<prefix><e>` ending in its own marker, and the path's own end is
/// `<prefix>` with the path's marker.
pub(crate) fn flat_exits(events: &str, aborted: bool) -> Vec<(String, bool)> {
    let mut prefix = String::new();
    let mut out = Vec::new();
    for it in items(events) {
        match it.strip_prefix('{').and_then(|x| x.strip_suffix("|}")) {
            Some(arm) => {
                let (body, mark) = arm.split_at(arm.len() - 1);
                out.push((format!("{prefix}{body}"), mark == "t"));
            }
            None => prefix.push_str(it),
        }
    }
    out.push((prefix, aborted));
    out
}

/// A folded path as (its exit items HOISTED to the line start, its flat
/// ops). A folded `{<e><m>|}` item after the plain ops `q` checks the exit
/// path `q e` from the count `q` leaves; its survivor is empty, so it moves
/// no count — rewritten `{<q><e><m>|}` and checked from the line's start, it
/// checks the same run. What remains of the path is flat.
pub(crate) fn hoist(events: &str) -> (Vec<String>, String) {
    let mut flat = String::new();
    let mut hoisted = Vec::new();
    for it in items(events) {
        match it.strip_prefix('{').and_then(|x| x.strip_suffix("|}")) {
            Some(arm) => hoisted.push(format!("{{{flat}{arm}|}}")),
            None => flat.push_str(it),
        }
    }
    (hoisted, flat)
}
