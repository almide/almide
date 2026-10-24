//! The structural leg's certificate BUNDLE: every witness one build's
//! shipped pass recorded, in the format `almide-verify bundle` reads
//! (crates/almide-verify/src/bundle.rs). One producer for both callers —
//! `almide verify` (src/cli/verify.rs) and the corpus sweep's
//! `examples/emit_structural_bundle.rs` — so the bundle a user verifies is
//! the bundle proofs/structural-wall.sh judges.
//!
//! The witnesses, for the pass that shipped:
//! - `ownership`, one per frame (witness.rs), and an `uncertified` record for
//!   every declined frame;
//! - `call-modes`, the program's call-mode witness (witness_modes.rs);
//! - `bounds`, every `xs[i]` / `b[i]` access with the check before it
//!   (witness_bounds.rs, #4005);
//! - `init`, every construction site the emitter fills itself — the
//!   allocation and the slot stores that initialize it (witness_init.rs,
//!   #4013);
//! - `names`, over the stock-WASI bytes the build ships: `(module:<space>)`
//!   per shared index space, `locals:<fn>` per function (cert_project.rs);
//! - `caps` per source-declared function and one `caps-transitive` call
//!   graph `(program)`, over the structural module (cert_project.rs).

/// One witness: its property, the function it is about, its bytes.
pub struct Record {
    pub property: &'static str,
    pub function: String,
    pub bytes: String,
}

/// The records and the uncertified frames `(function, reason)`.
pub type Witnesses = (Vec<Record>, Vec<(String, String)>);

/// The witnesses of the build the sinks just recorded. Arm the sinks with
/// [`super::start_collecting`] before the build; `emitted` is the module
/// the structural leg returned (importing `almide.*`), `shipped` its
/// stock-WASI form — the bytes the `names` witness is about. `allow` is an
/// `almide.toml [permissions].allow` list, which narrows what an `effect
/// fn` declares (cert_project.rs `caps_allowed`). Returns the records and
/// the uncertified frames `(function, reason)`.
pub fn records(
    emitted: &[u8],
    shipped: &[u8],
    allow: Option<&[String]>,
) -> Result<Witnesses, String> {
    let (pass, frames) = super::take_shipped().ok_or("no shipped pass recorded")?;
    let modes = super::take_modes();
    let bounds = super::take_bounds();
    let init = super::take_init();
    let decls = super::decls::take_for(emitted).ok_or("no declaration table for the shipped module")?;

    let mut records = Vec::new();
    let mut uncertified = Vec::new();
    for (name, cert) in frames {
        match cert.strip_prefix(super::DECLINE_PREFIX) {
            Some(reason) => uncertified.push((name, reason.trim().to_string())),
            None if cert.starts_with('!') => uncertified.push((name, cert.trim().to_string())),
            None => records.push(Record { property: "ownership", function: name, bytes: cert }),
        }
    }
    if let Some((_, w)) = modes.into_iter().find(|(p, _)| *p == pass) {
        records.push(Record { property: "call-modes", function: "(program)".into(), bytes: w });
    }
    // A pass with no index access records no stretch; its witness is the
    // empty program, which the checker accepts vacuously.
    let b = bounds.into_iter().find(|(p, _)| *p == pass).map(|(_, w)| w).unwrap_or_default();
    records.push(Record { property: "bounds", function: "(program)".into(), bytes: b });
    let i = init.into_iter().find(|(p, _)| *p == pass).map(|(_, w)| w).unwrap_or_default();
    records.push(Record { property: "init", function: "(program)".into(), bytes: i });
    // `to_wasi` keeps every defined function's order and moves it by the
    // change in the import count — up for the imports it adds, down for the
    // uncalled ones it prunes (#3114) — so a declared function keeps its
    // name in the stock-WASI bytes; any other function is named by index.
    use crate::cert_project as cp;
    let named = cp::decl_names(&decls).map_err(|e| format!("decls: {e}"))?;
    let (shipped_imports, emitted_imports) =
        (cp::function_imports(shipped).map_err(|e| format!("imports: {e}"))?, cp::function_imports(emitted).map_err(|e| format!("imports: {e}"))?);
    let label = |i: u32| match i.checked_sub(shipped_imports).and_then(|d| named.get(&(d + emitted_imports))) {
        Some(n) => format!("locals:{n}"),
        _ => format!("locals:func[{i}]"),
    };
    for (function, bytes) in cp::names(shipped, &label).map_err(|e| format!("names: {e}"))? {
        records.push(Record { property: "names", function, bytes });
    }
    let (flat, graph) = cp::caps_allowed(&decls, allow).map_err(|e| format!("caps: {e}"))?;
    for (function, bytes) in flat {
        records.push(Record { property: "caps", function, bytes });
    }
    records.push(Record { property: "caps-transitive", function: "(program)".into(), bytes: graph });
    Ok((records, uncertified))
}

/// One framing-safe line: a name or reason may not break the bundle's lines.
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The bundle text. `artifact` is `(sha256 hex, path)` of the shipped file
/// the witnesses describe: present, the bundle is version 2 and
/// almide-verify recomputes the digest from that file and rejects a
/// mismatch; absent, version 1.
pub fn render(
    producer: &str,
    source: &str,
    artifact: Option<(&str, &str)>,
    records: &[Record],
    uncertified: &[(String, String)],
) -> String {
    let mut out = match artifact {
        None => format!("almide-certificate-bundle 1\nproducer {}\nsource {}\n", one_line(producer), one_line(source)),
        Some((hex, path)) => format!(
            "almide-certificate-bundle 2\nproducer {}\nsource {}\nartifact sha256 {hex} {}\n",
            one_line(producer),
            one_line(source),
            one_line(path)
        ),
    };
    for r in records {
        out.push_str(&format!("witness {} {} {}\n{}\n", r.property, r.bytes.len(), one_line(&r.function), r.bytes));
    }
    for (f, reason) in uncertified {
        out.push_str(&format!("uncertified {} {}\n", one_line(f), one_line(reason)));
    }
    out
}
