//! The `regex` surface (stdlib/regex.almd), a BUNDLED module: `surface.rs`'s
//! rule applied to it. Every public fn there takes a `Regex = { pattern:
//! String }` and forwards to a `regex.__*` engine fn, whose body is the
//! self-hosted engine (stdlib/regex_engine.almd, the registry source the wasm
//! leg links) — so the vote is still that engine's, run in the pool tier.
//! Each arm below is the bundled body, step for step; the three helpers
//! (`__unmark`, `__matches`, `__splice`) are restated over `Value`s with the
//! line they mirror. Without this every regex fixture abstained with
//! `regex.new: no lowered body` (#4419 brought the scan's fixture in).

use std::rc::Rc;

use almide_base::intern::sym;

use crate::value::Value;
use crate::{Flow, Interpreter};

/// A flow's value, or the flow itself to propagate (an abort, fuel, an
/// abstain).
fn value_of(flow: Flow) -> Result<Value, Flow> {
    match flow {
        Flow::Value(v) => Ok(v),
        other => Err(other),
    }
}

/// `{ pattern: p }`.
fn regex_record(pattern: Value) -> Value {
    Value::Record { name: Some(sym("Regex")), fields: Rc::new(vec![(sym("pattern"), pattern)]) }
}

/// `re.pattern`.
fn pattern_of(re: &Value) -> Option<Value> {
    let Value::Record { fields, .. } = re else { return None };
    fields.iter().find(|(k, _)| k.as_str() == "pattern").map(|(_, v)| v.clone())
}

/// `fn __unmark(c: String) -> String? = if string.starts_with(c, "1") then
/// some(string.drop(c, 1)) else none`.
fn unmark(c: &Value) -> Option<Value> {
    let Value::Str(s) = c else { return None };
    Some(Value::Option(s.strip_prefix('1').map(|t| Box::new(Value::str(t.to_string())))))
}

/// `m |> list.drop(1) |> list.map((c) => __unmark(c))`.
fn groups_of(m: &[Value]) -> Option<Value> {
    let gs: Option<Vec<Value>> = m.iter().skip(1).map(unmark).collect();
    Some(Value::list(gs?))
}

/// `__matches(scan)`: the engines' scan `[k, field, k marks, field, ...,
/// tail]` as one `[field, marks...]` list per match — `k` read with `?? 1`,
/// `n = (len(body) - 1) / (k + 1)` whole matches, chunked by `k + 1`.
fn matches(scan: &Value) -> Option<Vec<Vec<Value>>> {
    let items = scan.as_iter_slice()?;
    let k = match items.first() {
        Some(Value::Str(s)) => s.parse::<i64>().unwrap_or(1),
        _ => 1,
    };
    let body = items.get(1..).unwrap_or(&[]);
    let width = usize::try_from(k + 1).ok().filter(|w| *w > 0)?;
    let n = (body.len() as i64 - 1).div_euclid(k + 1).max(0) as usize;
    Some(body[..n * width].chunks(width).map(|c| c.to_vec()).collect())
}

impl<'a> Interpreter<'a> {
    /// `stdlib/regex.almd`'s public fns. `None` = not one of them, or an
    /// argument shape the bundled body does not take — the caller keeps the
    /// `no lowered body` abstain.
    pub(crate) fn surface_regex(&mut self, f: &str, args: &[Value]) -> Option<Flow> {
        // `fn new(pattern) -> Regex = { pattern: regex.__new(pattern) }`
        if f == "new" {
            let [p] = args else { return None };
            return Some(self.forward("regex", "__new", std::slice::from_ref(p)).map_value(regex_record));
        }
        // `fn compile(pattern)`: `ok({ pattern })` when `__check` answers "",
        // else `err(reason)`.
        if f == "compile" {
            let [p] = args else { return None };
            return Some(match value_of(self.forward("regex", "__check", std::slice::from_ref(p))) {
                Ok(Value::Str(why)) if why.is_empty() => Flow::val(Value::Result(Ok(Box::new(regex_record(p.clone()))))),
                Ok(why @ Value::Str(_)) => Flow::val(Value::Result(Err(Box::new(why)))),
                Ok(_) => return None,
                Err(flow) => flow,
            });
        }
        if f == "escape" {
            let [Value::Str(s)] = args else { return None };
            // `string.to_list(s) |> list.map((c) => if string.contains(META, c)
            // then "\\" + c else c) |> list.join("")`
            let out: String = s
                .chars()
                .flat_map(|c| {
                    let esc = "\\.+*?()|[]{}^$".contains(c);
                    esc.then_some('\\').into_iter().chain(std::iter::once(c))
                })
                .collect();
            return Some(Flow::val(Value::str(out)));
        }
        let (re, rest) = args.split_first()?;
        let mut call = vec![pattern_of(re)?];
        call.extend(rest.iter().cloned());
        // The thin wrappers: `fn <f>(re, ...) = regex.__<g>(re.pattern, ...)`.
        let thin = match (f, rest.len()) {
            ("is_match", 1) => Some("__is_match"),
            ("is_full_match", 1) => Some("__full_match"),
            // the deprecated spelling of is_full_match, with its own body
            ("full_match", 1) => Some("__full_match"),
            ("find", 1) => Some("__find"),
            ("find_all", 1) => Some("__find_all"),
            ("replace", 2) => Some("__replace"),
            ("replace_first", 2) => Some("__replace_first"),
            ("split", 1) => Some("__split"),
            _ => None,
        };
        if let Some(g) = thin {
            return Some(self.forward("regex", g, &call));
        }
        match (f, rest.len()) {
            // `let i = regex.__index_of(..); if i < 0 then none else some(i)`
            ("index_of", 1) => Some(match value_of(self.forward("regex", "__index_of", &call)) {
                Ok(Value::Int(i)) => Flow::val(Value::Option((i >= 0).then(|| Box::new(Value::Int(i))))),
                Ok(_) => return None,
                Err(flow) => flow,
            }),
            // `regex.__captures(..) |> option.map((cs) => cs |> list.map(__unmark))`
            ("captures", 1) => Some(match value_of(self.forward("regex", "__captures", &call)) {
                Ok(Value::Option(None)) => Flow::val(Value::Option(None)),
                Ok(Value::Option(Some(cs))) => {
                    let cs: Option<Vec<Value>> = cs.as_iter_slice()?.iter().map(unmark).collect();
                    Flow::val(Value::Option(Some(Box::new(Value::list(cs?)))))
                }
                Ok(_) => return None,
                Err(flow) => flow,
            }),
            // `__matches(regex.__scan(re.pattern, s, false)) |> list.map((m) =>
            // m |> list.drop(1) |> list.map((c) => __unmark(c)))`
            ("captures_all", 1) => {
                call.push(Value::Bool(false));
                Some(match value_of(self.forward("regex", "__scan", &call)) {
                    Ok(scan) => {
                        let all: Option<Vec<Value>> = matches(&scan)?.iter().map(|m| groups_of(m)).collect();
                        Flow::val(Value::list(all?))
                    }
                    Err(flow) => flow,
                })
            }
            // `__splice(regex.__scan(re.pattern, s, false), f)`
            ("replace_with", 2) => self.scan_splice(call, false),
            // `__splice(regex.__scan(re.pattern, s, true), f)`
            ("replace_first_with", 2) => self.scan_splice(call, true),
            _ => None,
        }
    }

    /// `__splice(regex.__scan(re.pattern, s, first), f)` for `call` =
    /// `[pattern, s, f]`.
    fn scan_splice(&mut self, mut call: Vec<Value>, first: bool) -> Option<Flow> {
        let Value::Closure(clo) = call.pop()? else { return None };
        call.push(Value::Bool(first));
        Some(match value_of(self.forward("regex", "__scan", &call)) {
            Ok(scan) => self.splice(&scan, &clo)?,
            Err(flow) => flow,
        })
    }

    /// `__splice(scan, f)`: each match's field followed by `f` of its
    /// groups, joined, then the scan's last item (the trailing field):
    /// `list.join(__matches(scan) |> list.map((m) => list.get(m, 0) ?? "" +
    /// f(...)), "") + list.get(scan, list.len(scan) - 1) ?? ""`.
    fn splice(&mut self, scan: &Value, clo: &Rc<crate::value::Closure>) -> Option<Flow> {
        let mut out = String::new();
        for m in matches(scan)? {
            let Some(Value::Str(field)) = m.first() else { return None };
            out.push_str(field);
            match value_of(self.apply_closure(clo, vec![groups_of(&m)?])) {
                Ok(Value::Str(r)) => out.push_str(&r),
                Ok(_) => return None,
                Err(flow) => return Some(flow),
            }
        }
        match scan.as_iter_slice()?.last() {
            Some(Value::Str(tail)) => out.push_str(tail),
            Some(_) => return None,
            None => {}
        }
        Some(Flow::val(Value::str(out)))
    }
}

trait MapValue {
    fn map_value(self, f: impl FnOnce(Value) -> Value) -> Flow;
}

impl MapValue for Flow {
    /// The flow with its value mapped; any other flow unchanged.
    fn map_value(self, f: impl FnOnce(Value) -> Value) -> Flow {
        match self {
            Flow::Value(v) => Flow::Value(f(v)),
            other => other,
        }
    }
}
