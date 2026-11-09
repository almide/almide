//! The anchor file (`almide-anchors 1`) and the bundle's `rc-sig` records.

use crate::{bundle_records, parse_at, At};

/// #4278 B1: per signed runtime helper (function index), whether it consumes
/// each parameter — read from the bundle's `rc-sig` record the anchor file
/// names for it (the record the proven checker judges against the helper's
/// lines). A helper the anchor file signs must carry its own `rc` frame, so
/// its logged events are held to its shipped body here like any frame's;
/// a name with no record, or a signed helper with no frame, is refused.
pub(crate) fn helper_sigs(
    bundle: &[u8],
    imports: u32,
    frames: &[(usize, String)],
    helpers: &[(usize, String)],
) -> Result<std::collections::HashMap<u32, Vec<bool>>, String> {
    let records = bundle_records(bundle, "rc-sig")?;
    let mut sigs = std::collections::HashMap::new();
    for (pos, name) in helpers {
        let (_, rec) = records
            .iter()
            .find(|(f, _)| f == name)
            .ok_or_else(|| format!("rc: helper {name} at position {pos} has no rc-sig record in the bundle"))?;
        if !frames.iter().any(|(p, _)| p == pos) {
            return Err(format!("rc: signed helper {name} at position {pos} has no rc line"));
        }
        let modes = rec.split('|').next().unwrap_or("");
        let consume: Vec<bool> = modes.split_whitespace().map(|m| m != "0").collect();
        sigs.insert(imports + *pos as u32, consume);
    }
    Ok(sigs)
}

/// #4277's lines: the rc_inc routine, the release routines, per certified
/// frame its position and live RC events, the arm and routine windows, and
/// (#4278 B1) per signed runtime helper its position and the name of the
/// bundle's `rc-sig` record that declares it.
pub type RcLines = (
    Option<u32>,
    std::collections::HashSet<u32>,
    Vec<(usize, String)>,
    Vec<(usize, usize, usize)>,
    Vec<(usize, usize, usize)>,
    Vec<(usize, String)>,
);

/// The anchor file read by [`parse_anchors`]: `(alloc, init anchors, bounds
/// anchors, judge sites, rc lines)`.
pub type Anchors = (u32, String, String, Vec<At>, Option<RcLines>);

/// [`Anchors`] with the `holds` sites after the judges (#4280).
pub type AnchorsHeld = (u32, String, String, Vec<At>, Vec<At>, Option<RcLines>);

/// The anchor file: `(alloc, init anchors, bounds anchors, judge sites, rc
/// lines)`. The `judges` line is optional (a file without it locates no
/// judge, and an access a judge sits in front of is then refused); so are
/// the `rcfns` / `rc` pair (measurement-gated, #4277).
pub fn parse_anchors(text: &str) -> Result<Anchors, String> {
    parse_anchors_held(text).map(|(a, i, b, j, _, r)| (a, i, b, j, r))
}

/// [`parse_anchors`] and the optional `holds` line after `judges` (#4280).
pub fn parse_anchors_held(text: &str) -> Result<AnchorsHeld, String> {
    let mut lines = text.lines();
    if lines.next() != Some("almide-anchors 1") {
        return Err("anchors: not an `almide-anchors 1` file".into());
    }
    let mut field = |key: &str| -> Result<String, String> {
        let l = lines
            .next()
            .ok_or_else(|| format!("anchors: no `{key}` line"))?;
        if l == key {
            return Ok(String::new());
        }
        l.strip_prefix(&format!("{key} "))
            .map(str::to_string)
            .ok_or_else(|| format!("anchors: expected `{key}`, read `{l}`"))
    };
    // `-`: the module has no `$alloc` (nothing allocates), and no site can
    // name one.
    let alloc = match field("alloc")?.as_str() {
        "-" => u32::MAX,
        x => x
            .parse()
            .map_err(|_| "anchors: bad alloc index".to_string())?,
    };
    let (init, bounds) = (field("init")?, field("bounds")?);
    // The optional `fields` line (#4279) is read by `fields_anchors`.
    // So are the optional `prims`, `selfhost` and `routines` lines (#4005
    // A4), read by `prims_anchors` and lin.rs.
    let mut rest: Vec<&str> = lines
        .filter(|l| {
            !["fields", "prims", "selfhost", "routines"].iter().any(|k| *l == *k || l.strip_prefix(k).is_some_and(|r| r.starts_with(' ')))
        })
        .collect();
    let rc_at = rest.iter().position(|l| l.starts_with("rcfns"));
    let rc_lines = match rc_at {
        None => None,
        Some(i) => {
            let fns: Vec<&str> = rest[i].split_whitespace().skip(1).collect();
            let inc = fns.first().and_then(|t| t.parse().ok());
            let decs = fns.iter().skip(1).filter_map(|t| t.parse().ok()).collect();
            let frames = rest
                .get(i + 1)
                .and_then(|l| l.strip_prefix("rc"))
                .unwrap_or("")
                .split(';')
                .filter_map(|g| {
                    let (p, e) = g.trim().split_once('|')?;
                    Some((p.parse().ok()?, e.to_string()))
                })
                .collect();
            let windows = |line: Option<&&str>, key: &str| -> Vec<(usize, usize, usize)> {
                line.and_then(|l| l.strip_prefix(key))
                    .unwrap_or("")
                    .split_whitespace()
                    .filter_map(|t| {
                        let (p, r) = t.split_once(':')?;
                        let (a, b) = r.split_once('-')?;
                        Some((p.parse().ok()?, a.parse().ok()?, b.parse().ok()?))
                    })
                    .collect()
            };
            let arms = windows(rest.get(i + 2), "rcarms");
            let routines = windows(rest.get(i + 3), "rcroutines");
            let helpers = rest
                .get(i + 4)
                .and_then(|l| l.strip_prefix("rchelpers"))
                .unwrap_or("")
                .split_whitespace()
                .map(|t| {
                    let (p, n) = t.split_once(':').ok_or_else(|| format!("anchors: bad rchelpers token `{t}`"))?;
                    Ok((p.parse().map_err(|_| format!("anchors: bad rchelpers position `{t}`"))?, n.to_string()))
                })
                .collect::<Result<Vec<_>, String>>()?;
            rest.truncate(i);
            Some((inc, decs, frames, arms, routines, helpers))
        }
    };
    let mut lines = rest.into_iter();
    let judges = match lines.next() {
        None => Vec::new(),
        Some(l) => {
            let rest = if l == "judges" {
                ""
            } else {
                l.strip_prefix("judges ")
                    .ok_or_else(|| format!("anchors: expected `judges`, read `{l}`"))?
            };
            let mut v = Vec::new();
            for t in rest.split_whitespace() {
                if let Some(at) = parse_at(t)? {
                    v.push(at);
                }
            }
            v
        }
    };
    let holds = match lines.next() {
        None => Vec::new(),
        Some(l) => {
            let rest = if l == "holds" {
                ""
            } else {
                l.strip_prefix("holds ")
                    .ok_or_else(|| format!("anchors: expected `holds`, read `{l}`"))?
            };
            let mut v = Vec::new();
            for t in rest.split_whitespace() {
                if let Some(at) = parse_at(t)? {
                    v.push(at);
                }
            }
            v
        }
    };
    if let Some(l) = lines.next() {
        return Err(format!("anchors: unexpected line `{l}`"));
    }
    Ok((alloc, init, bounds, judges, holds, rc_lines))
}

/// The anchor file's `fields` line (#4279): one token per fields-witness
/// event, `;` between items; empty when the file has none.
pub fn fields_anchors(text: &str) -> String {
    text.lines()
        .find_map(|l| if l == "fields" { Some("") } else { l.strip_prefix("fields ") })
        .unwrap_or("")
        .to_string()
}

/// The anchor file's `prims` line (#4005 A4): one token per self-host prim
/// load or store the pass emitted; empty when the file has none.
pub fn prims_anchors(text: &str) -> String {
    text.lines()
        .find_map(|l| if l == "prims" { Some("") } else { l.strip_prefix("prims ") })
        .unwrap_or("")
        .to_string()
}
