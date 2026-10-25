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
    records_inner(emitted, shipped, allow, None).map(|(w, _)| w)
}

/// [`records`] plus the ANCHOR file of the shipped pass's bounds and init
/// witnesses (#4011 (d)), the input of `almide-anchor-check`:
///
/// ```text
/// almide-anchors 1
/// alloc <function index of $alloc in the shipped bytes>
/// init <one token per init event: position:ordinal | ->
/// bounds <one token per bounds token: position:ordinal | ->
/// ```
///
/// laid out exactly as the witness texts (`;` between sites / stretches,
/// one token per event). `position` is the defined function's position in
/// the shipped module, `ordinal` the instruction's index in its body; `-`
/// is an event no instruction carries (a kill) or one in a body that did not
/// ship. `fmap` is `to_wasi_mapped`'s map: per defined function of
/// `emitted`, its position in `shipped` (`None`: pruned).
pub fn records_with_anchors(
    emitted: &[u8],
    shipped: &[u8],
    allow: Option<&[String]>,
    fmap: &[Option<u32>],
) -> Result<(Witnesses, String), String> {
    records_inner(emitted, shipped, allow, Some(fmap))
}

fn records_inner(
    emitted: &[u8],
    shipped: &[u8],
    allow: Option<&[String]>,
    fmap: Option<&[Option<u32>]>,
) -> Result<(Witnesses, String), String> {
    let (pass, frames) = super::take_shipped().ok_or("no shipped pass recorded")?;
    let modes = super::take_modes();
    let bounds = super::take_bounds();
    let init = super::take_init();
    let bounds_anchors = super::bounds::take_anchors();
    let init_anchors = super::init::take_anchors();
    let seals = super::anchor::take();
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
    let pick = |v: Vec<(usize, String)>| v.into_iter().find(|(p, _)| *p == pass).map(|(_, w)| w).unwrap_or_default();
    let anchor_file = match fmap {
        None => String::new(),
        Some(fmap) => {
    let locate = Locator::new(emitted, shipped, seals, fmap)?;
    format!(
        "almide-anchors 1\nalloc {}\ninit {}\nbounds {}\n",
        {
            // `$alloc`'s slot, through the same map (the prune renumbers).
            let d = (crate::F_ALLOC - cp::function_imports(emitted).map_err(|e| format!("imports: {e}"))?) as usize;
            match fmap.get(d).copied().flatten() {
                Some(j) => (j + cp::function_imports(shipped).map_err(|e| format!("imports: {e}"))?).to_string(),
                None => "-".to_string(),
            }
        },
        locate.text(&pick(init_anchors))?,
        locate.text(&pick(bounds_anchors))?,
    )
        }
    };
    let b = bounds.into_iter().find(|(p, _)| *p == pass).map(|(_, w)| w).unwrap_or_default();
    records.push(Record { property: "bounds", function: "(program)".into(), bytes: b });
    let i = init.into_iter().find(|(p, _)| *p == pass).map(|(_, w)| w).unwrap_or_default();
    records.push(Record { property: "init", function: "(program)".into(), bytes: i });
    use crate::cert_project as cp;
    // `to_wasi` keeps every defined function's order and moves it by the
    // change in the import count — up for the imports it adds, down for the
    // uncalled ones it prunes (#3114) — so a declared function keeps its
    // name in the stock-WASI bytes; any other function is named by index.
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
    Ok(((records, uncertified), anchor_file))
}

/// Turns the sinks' `serial@offset` anchors into `position:ordinal`.
struct Locator {
    /// serial -> defined position in the emitted module.
    pos: std::collections::HashMap<u64, usize>,
    /// Per emitted defined position, each instruction's end offset in its
    /// body, and the position it ships at.
    ends: Vec<Vec<usize>>,
    ships_at: Vec<Option<u32>>,
    /// Per located emitted body, each instruction's index in the shipped
    /// body (`to_wasi` puts `i32.const 1; call $exit` before every
    /// `unreachable`).
    ords: std::collections::HashMap<usize, Vec<usize>>,
}

/// Each emitted instruction's index in the shipped body: the two walk in
/// step, the shipped one allowed to run ahead only into an `unreachable`
/// (the exit `to_wasi` injects before it).
fn align(emitted: &[u8], shipped: &[u8]) -> Result<Vec<usize>, String> {
    use std::mem::discriminant;
    fn ops(b: &[u8]) -> Result<Vec<wasmparser::Operator<'_>>, String> {
        let body = wasmparser::FunctionBody::new(wasmparser::BinaryReader::new(b, 0));
        body.get_operators_reader().map_err(|e| e.to_string())?.into_iter().collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }
    let (e, sh) = (ops(emitted)?, ops(shipped)?);
    let mut out = Vec::with_capacity(e.len());
    let mut j = 0;
    for op in &e {
        let unreachable = matches!(op, wasmparser::Operator::Unreachable);
        while j < sh.len() && discriminant(&sh[j]) != discriminant(op) && unreachable {
            j += 1;
        }
        if j >= sh.len() || discriminant(&sh[j]) != discriminant(op) {
            return Err(format!("anchors: an emitted instruction ({op:?}) has no counterpart in the shipped body"));
        }
        out.push(j);
        j += 1;
    }
    if j != sh.len() {
        return Err("anchors: the shipped body has instructions past the emitted one's".into());
    }
    Ok(out)
}

/// A defined body's bytes and its instructions' end offsets.
type Body = (Vec<u8>, Vec<usize>);

/// Each defined body's bytes and its instructions' end offsets.
fn bodies(bytes: &[u8]) -> Result<Vec<Body>, String> {
    let mut out = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        if let wasmparser::Payload::CodeSectionEntry(body) = payload.map_err(|e| e.to_string())? {
            let range = body.range();
            let (start, end) = (range.start as usize, range.end as usize);
            let mut r = body.get_operators_reader().map_err(|e| e.to_string())?;
            let mut ends = Vec::new();
            while !r.eof() {
                r.read().map_err(|e| e.to_string())?;
                ends.push(r.original_position() as usize - start);
            }
            out.push((bytes[start..end].to_vec(), ends));
        }
    }
    Ok(out)
}

impl Locator {
    fn new(
        emitted: &[u8],
        shipped: &[u8],
        seals: std::collections::HashMap<u64, u64>,
        fmap: &[Option<u32>],
    ) -> Result<Self, String> {
        let em = bodies(emitted)?;
        let sh = bodies(shipped)?;
        let mut by_digest = std::collections::HashMap::new();
        for (i, (raw, _)) in em.iter().enumerate() {
            by_digest.entry(super::anchor::body_digest(raw)).or_insert(i);
        }
        let pos: std::collections::HashMap<u64, usize> = seals.into_iter().filter_map(|(k, d)| by_digest.get(&d).map(|&i| (k, i))).collect();
        if fmap.len() != em.len() {
            return Err(format!("anchors: the function map has {} rows for {} emitted bodies", fmap.len(), em.len()));
        }
        let mut ords = std::collections::HashMap::new();
        for &i in pos.values() {
            if ords.contains_key(&i) {
                continue;
            }
            if let Some(j) = fmap[i] {
                let (sraw, _) = sh.get(j as usize).ok_or_else(|| format!("anchors: emitted body {i} ships at {j}, past the shipped bodies"))?;
                ords.insert(i, align(&em[i].0, sraw)?);
            }
        }
        Ok(Locator { pos, ends: em.into_iter().map(|(_, e)| e).collect(), ships_at: fmap.to_vec(), ords })
    }

    fn token(&self, t: &str) -> String {
        let Some((k, off)) = t.split_once('@') else { return "-".to_string() };
        let (Ok(k), Ok(off)) = (k.parse::<u64>(), off.parse::<usize>()) else { return "?".to_string() };
        let Some(&i) = self.pos.get(&k) else { return "-".to_string() };
        let Some(j) = self.ships_at[i] else { return "-".to_string() };
        match self.ends[i].iter().position(|&e| e == off) {
            Some(o) => format!("{j}:{}", self.ords[&i][o]),
            // A sealed body whose recorded offset is no instruction's end:
            // the checker refuses `?`.
            None => "?".to_string(),
        }
    }

    fn text(&self, w: &str) -> Result<String, String> {
        Ok(w.split(';')
            .map(|g| g.split_whitespace().map(|t| self.token(t)).collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
            .join(";"))
    }
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
