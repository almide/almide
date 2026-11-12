// Included in render_native_rec.rs: the release of a recursive type's value
// on a bounded stack (#4676).
//
// A derived drop releases a value's fields by calling itself: one native
// stack frame per level. `type T = { v: Int, kids: List[T] }` built 200,000
// deep through its list, and every other value that reaches itself through a
// list, an `Option` or a boxed variant, overflowed the stack when it was
// released.
//
// Here every struct and enum of such a cycle implements `Drop`: before its
// fields are released it DETACHES the values of its own type cycle it holds
// — a non-empty list of them, an `Option`'s payload, a boxed variant other
// than its field-less placeholder case — moving each onto one work list,
// whose values are detached in turn, in a loop. Each value is then released
// with nothing of the cycle left in it, so its own release recurses no
// deeper than one level. A field reached through a box that cannot be emptied
// (a struct, or an enum with no field-less case) is not moved: its own fields
// are detached in place. The values released and the order of the program's
// effects are unchanged (a release has none); only the stack depth is.
//
// A cycle is left to the derived drop when one of its fields reaches it back
// through any other holder (a map, a set, a tuple, a `Result`), when it holds
// a region twin, or when a cycle of it runs through in-place edges only.
//
// A struct with `Drop` cannot be moved out of field by field, so a field read
// that moves the value out of such a struct takes it instead, leaving the
// field's `Default` ([`has_deep_drop`]).

/// How a field of a cycle's type reaches the next value of the cycle.
#[derive(Clone, Copy, PartialEq, Debug)]
enum DeepEdge {
    /// A list of them: detached whole when not empty.
    List(u16),
    /// An `Option` of one: its payload detached.
    Opt(u16),
    /// One in a box, an enum with a field-less case: detached, the case left.
    Boxed(u16, u32),
    /// One in a box that cannot be emptied: its own fields detached in place.
    InPlace(u16),
}

/// A struct or an enum (not an alias of a builtin or a carrier).
fn deep_node(d: &RecDecl) -> bool {
    d.alias.is_none() && d.cell_of.is_none() && d.map_of.is_none() && d.option_of.is_none() && d.set_of.is_none()
        && d.result_of.is_none() && d.list_of.is_none() && d.closure_of.is_none()
        && (d.cases.is_some() || !d.fields.is_empty())
}

/// Every generated type `d` holds directly, through any holder.
fn deep_parts(d: &RecDecl) -> Vec<u16> {
    let mut ks: Vec<NTy> = d.fields.clone();
    if let Some(cs) = &d.cases {
        ks.extend(cs.iter().flat_map(|(_, fs)| fs.iter().copied()));
    }
    ks.extend(d.option_of);
    ks.extend(d.list_of);
    ks.extend(d.set_of);
    ks.extend(d.cell_of);
    if let Some((a, b)) = d.result_of.or(d.map_of) {
        ks.extend([a, b]);
    }
    ks.into_iter().filter_map(|t| match t {
        NTy::Rec(x) => Some(x),
        _ => None,
    }).collect()
}

/// The types reachable from `from` (itself excluded unless on a cycle).
fn deep_reach(r: &[RecDecl], from: u16) -> std::collections::BTreeSet<u16> {
    let mut seen = std::collections::BTreeSet::new();
    let mut stack: Vec<u16> = r.get(from as usize).map(deep_parts).unwrap_or_default();
    while let Some(k) = stack.pop() {
        if seen.insert(k) {
            stack.extend(r.get(k as usize).map(deep_parts).unwrap_or_default());
        }
    }
    seen
}

/// The edge field `f` of type `i` makes into the cycle `scc`, `Err(())` when
/// the field reaches the cycle another way, `Ok(None)` when it does not.
fn deep_edge(r: &[RecDecl], scc: &std::collections::BTreeSet<u16>, i: u16, f: NTy) -> Result<Option<DeepEdge>, ()> {
    let NTy::Rec(j) = f else { return Ok(None) };
    let into = |k: u16| scc.contains(&k);
    let reaches = into(j) || deep_reach(r, j).iter().any(|k| into(*k));
    if !reaches {
        return Ok(None);
    }
    let d = r.get(j as usize).ok_or(())?;
    let boxed = enum_field_boxed(i, f);
    if into(j) {
        if !boxed {
            return Err(());
        }
        let placeholder = d.cases.as_ref().and_then(|cs| cs.iter().find(|(_, fs)| fs.is_empty()).map(|(t, _)| *t));
        return Ok(Some(placeholder.map_or(DeepEdge::InPlace(j), |t| DeepEdge::Boxed(j, t))));
    }
    match (d.option_of, d.list_of) {
        (Some(NTy::Rec(k)), _) if into(k) => Ok(Some(DeepEdge::Opt(k))),
        (_, Some(NTy::Rec(k))) if into(k) && !boxed => Ok(Some(DeepEdge::List(k))),
        _ => Err(()),
    }
}

thread_local! {
    /// The plan of the type table it was made from (keyed by the table's text).
    static DEEP_PLAN: RefCell<Option<(u64, std::rc::Rc<DeepPlan>)>> = const { RefCell::new(None) };
}

/// The plan of the current type table, made once per table.
fn current_deep_plan() -> std::rc::Rc<DeepPlan> {
    use std::hash::{Hash, Hasher};
    RECS.with(|r| {
        let r = r.borrow();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        format!("{:?}", *r).hash(&mut h);
        let key = h.finish();
        if let Some((k, plan)) = DEEP_PLAN.with(|p| p.borrow().clone()) {
            if k == key {
                return plan;
            }
        }
        let plan = std::rc::Rc::new(deep_plan(&r));
        DEEP_PLAN.with(|p| *p.borrow_mut() = Some((key, plan.clone())));
        plan
    })
}

/// Generated type `i` releases its cycle iteratively: a field must not be
/// moved out of it, only taken.
pub(crate) fn has_deep_drop(i: u16) -> bool {
    current_deep_plan().contains_key(&i)
}

/// Each type of a supported cycle, with its edges per case (`None`: a struct).
type DeepPlan = std::collections::BTreeMap<u16, Vec<(Option<u32>, Vec<(usize, DeepEdge)>)>>;

fn deep_plan(r: &[RecDecl]) -> DeepPlan {
    let mut plan = DeepPlan::new();
    let mut done = std::collections::BTreeSet::new();
    for (i, d) in r.iter().enumerate() {
        let i = i as u16;
        if done.contains(&i) || !deep_node(d) || !deep_reach(r, i).contains(&i) {
            continue;
        }
        // The cycle of `i`: the types it reaches that reach it back.
        let scc: std::collections::BTreeSet<u16> = deep_reach(r, i).into_iter()
            .filter(|k| r.get(*k as usize).is_some_and(deep_node) && deep_reach(r, *k).contains(&i))
            .collect();
        done.extend(scc.iter().copied());
        let mut part = DeepPlan::new();
        let ok = scc.iter().all(|&k| {
            let d = &r[k as usize];
            if is_region(k) {
                return false;
            }
            let cases: Vec<(Option<u32>, &[NTy])> = match &d.cases {
                Some(cs) => cs.iter().map(|(t, fs)| (Some(*t), fs.as_slice())).collect(),
                None => vec![(None, d.fields.as_slice())],
            };
            let mut rows = Vec::new();
            for (tag, fs) in cases {
                let mut edges = Vec::new();
                for (n, f) in fs.iter().enumerate() {
                    match deep_edge(r, &scc, k, *f) {
                        Ok(Some(e)) => edges.push((n, e)),
                        Ok(None) => {}
                        Err(()) => return false,
                    }
                }
                if !edges.is_empty() {
                    rows.push((tag, edges));
                }
            }
            part.insert(k, rows);
            true
        });
        // A cycle of in-place edges alone would never move anything.
        let in_place: std::collections::BTreeMap<u16, Vec<u16>> = part.iter().map(|(k, rows)| {
            (*k, rows.iter().flat_map(|(_, es)| es.iter().filter_map(|(_, e)| match e {
                DeepEdge::InPlace(j) => Some(*j),
                _ => None,
            })).collect())
        }).collect();
        let loops = in_place.keys().any(|&s| {
            let mut seen = std::collections::BTreeSet::new();
            let mut stack = in_place[&s].clone();
            while let Some(k) = stack.pop() {
                if k == s {
                    return true;
                }
                if seen.insert(k) {
                    stack.extend(in_place.get(&k).cloned().unwrap_or_default());
                }
            }
            false
        });
        if ok && !loops {
            plan.extend(part);
        }
    }
    plan
}

/// The statements detaching edge `e` of the field at the place `p`.
fn deep_detach(p: &str, e: DeepEdge) -> String {
    match e {
        DeepEdge::List(k) => format!("if !{p}.is_empty() {{ w.push(__AlmdDeep::V{k}(std::mem::take(&mut {p}))); }}"),
        DeepEdge::Opt(k) => format!("if let Some(x) = {p}.take() {{ w.push(__AlmdDeep::N{k}(x)); }}"),
        DeepEdge::Boxed(k, t) => {
            let n = struct_name(k);
            format!("if !matches!(*{p}, {n}::C{t}) {{ w.push(__AlmdDeep::N{k}(std::mem::replace(&mut *{p}, {n}::C{t}))); }}")
        }
        DeepEdge::InPlace(_) => format!("{p}.__almd_detach(w);"),
    }
}

/// Edge `e` of the field at the place `p` holds nothing of the cycle.
fn deep_bare(p: &str, e: DeepEdge) -> String {
    match e {
        DeepEdge::List(_) => format!("{p}.is_empty()"),
        DeepEdge::Opt(_) => format!("{p}.is_none()"),
        DeepEdge::Boxed(k, t) => format!("matches!(**{p}, {}::C{t})", struct_name(k)),
        DeepEdge::InPlace(_) => format!("{p}.__almd_bare()"),
    }
}

/// The work list's inline capacity: a value whose release detaches no more
/// than this many at once at any point is released without an allocation.
const DEEP_NEAR: usize = 8;

/// The iterative `Drop` of every supported cycle of the program, or "" when
/// it has none (so a render without one is byte-identical to before).
pub(crate) fn deep_drop_impls() -> String {
    let plan = current_deep_plan();
    if plan.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let mut items = std::collections::BTreeSet::new();
    for (k, rows) in plan.iter() {
        let n = struct_name(*k);
        let (mut detach, mut bare) = (String::new(), String::new());
        for (tag, edges) in rows {
            for (_, e) in edges {
                match e {
                    DeepEdge::List(j) => items.insert(format!("V{j}(Vec<{}>)", struct_name(*j))),
                    DeepEdge::Opt(j) | DeepEdge::Boxed(j, _) => items.insert(format!("N{j}({})", struct_name(*j))),
                    DeepEdge::InPlace(_) => false,
                };
            }
            match tag {
                None => {
                    for (f, e) in edges {
                        detach.push_str(&format!("        {}\n", deep_detach(&format!("self.f{f}"), *e)));
                    }
                    let all = edges.iter().map(|(f, e)| deep_bare(&format!("(&self.f{f})"), *e)).collect::<Vec<_>>().join(" && ");
                    bare.push_str(&format!("        {all}\n"));
                }
                Some(t) => {
                    let width = r_case_width(*k, *t);
                    let binds = (0..width).map(|f| if edges.iter().any(|(g, _)| *g == f) { format!("f{f}") } else { "_".to_string() }).collect::<Vec<_>>().join(", ");
                    detach.push_str(&format!("        if let Self::C{t}({binds}) = self {{\n"));
                    for (f, e) in edges {
                        detach.push_str(&format!("            {}\n", deep_detach(&format!("(*f{f})"), *e)));
                    }
                    detach.push_str("        }\n");
                    let all = edges.iter().map(|(f, e)| deep_bare(&format!("f{f}"), *e)).collect::<Vec<_>>().join(" && ");
                    bare.push_str(&format!("            Self::C{t}({binds}) => {all},\n"));
                }
            }
        }
        let bare = if rows.iter().any(|(t, _)| t.is_some()) { format!("        match self {{\n{bare}            _ => true,\n        }}\n") } else { bare };
        out.push_str(&format!(
            "impl {n} {{\n    #[inline]\n    fn __almd_detach(&mut self, w: &mut __AlmdWork) {{\n{detach}    }}\n    #[inline]\n    #[allow(unreachable_patterns)]\n    fn __almd_bare(&self) -> bool {{\n{bare}    }}\n}}\n\
             impl Drop for {n} {{\n    #[inline]\n    fn drop(&mut self) {{\n        if !self.__almd_bare() {{\n            let mut w = __AlmdWork::new();\n            self.__almd_detach(&mut w);\n            __almd_deep_drain(&mut w);\n        }}\n    }}\n}}\n\n"
        ));
    }
    let arms = items.iter().map(|it| {
        let (v, _) = it.split_once('(').unwrap_or((it, ""));
        if v.starts_with('V') {
            format!("            __AlmdDeep::{v}(mut v) => for x in v.iter_mut() {{ x.__almd_detach(w) }},\n")
        } else {
            format!("            __AlmdDeep::{v}(mut x) => x.__almd_detach(w),\n")
        }
    }).collect::<String>();
    let variants = items.iter().map(|it| format!("    {it},\n")).collect::<String>();
    let nones = vec!["None"; DEEP_NEAR].join(", ");
    out.push_str(&format!(
        "/// A value detached from a recursive type's cycle, released in a loop (#4676).\nenum __AlmdDeep {{\n{variants}}}\n\n\
         /// The detached values not yet released: the first few inline, the rest in a list.\nstruct __AlmdWork {{\n    near: [Option<__AlmdDeep>; {DEEP_NEAR}],\n    n: usize,\n    far: Vec<__AlmdDeep>,\n}}\n\n\
         impl __AlmdWork {{\n    #[inline]\n    fn new() -> Self {{\n        Self {{ near: [{nones}], n: 0, far: Vec::new() }}\n    }}\n    #[inline]\n    fn push(&mut self, x: __AlmdDeep) {{\n        if self.n < {DEEP_NEAR} {{\n            self.near[self.n] = Some(x);\n            self.n += 1;\n        }} else {{\n            self.far.push(x);\n        }}\n    }}\n    #[inline]\n    fn pop(&mut self) -> Option<__AlmdDeep> {{\n        if let Some(x) = self.far.pop() {{\n            return Some(x);\n        }}\n        if self.n == 0 {{\n            return None;\n        }}\n        self.n -= 1;\n        self.near[self.n].take()\n    }}\n}}\n\n\
         #[inline(never)]\nfn __almd_deep_drain(w: &mut __AlmdWork) {{\n    while let Some(it) = w.pop() {{\n        match it {{\n{arms}        }}\n    }}\n}}\n\n"
    ));
    out
}

/// The number of fields of case `t` of enum `k`.
fn r_case_width(k: u16, t: u32) -> usize {
    enum_cases(k).and_then(|cs| cs.into_iter().find(|(c, _)| *c == t).map(|(_, fs)| fs.len())).unwrap_or(0)
}
