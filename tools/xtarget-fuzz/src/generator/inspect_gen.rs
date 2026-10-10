//! Program text for `xtarget-fuzz inspect` (proofs/stdlib-inspection.toml):
//! from one catalogued signature, the timing programs of criterion 2, the
//! diamond project of criterion 5. Text only — the runs and verdicts live in
//! `src/inspect.rs`.
//!
//! Instantiation is fixed, not random: every free type variable is `Int`
//! (a pinned Map-key / Result-error slot is `String`), so the same function
//! always yields the same programs and two runs measure the same thing.

use std::collections::HashMap;
use std::fmt::Write;

use super::catalogue::Signature;
use super::sig_type::SigType;
use super::types::{render_print, GenType};
use super::Project;

/// One function, instantiated, with the parameter its size is measured in.
#[derive(Debug, Clone)]
pub struct Target {
    pub name: String,
    params: Vec<(String, SigType)>,
    subst: HashMap<String, GenType>,
    ret: GenType,
    /// The parameter scaled to `n` (the first List / Map / String one, else a
    /// size-like Int); `None`: the function has no size to scale.
    pub size_param: Option<usize>,
    /// A parameter of the return type: the function can rebind it in a loop,
    /// a tail call and a fold (criterion 2's use-shapes).
    pub rebind_param: Option<usize>,
}

const SIZE_LIKE_INT: &[&str] = &["n", "count", "times", "len", "size", "end", "width"];

/// Element-typed parameters a search compares against: given a value no
/// sized input holds, so the search scans to the end (its worst case).
const SEARCH_PARAMS: &[&str] = &["x", "value", "elem", "item", "needle", "target"];

/// Functions that stop at the first `true` from their predicate: given a
/// predicate that is always `false`, so they visit every element.
const STOPS_ON_TRUE: &[&str] = &["any", "find", "find_index", "position", "find_map", "index_where"];

impl Target {
    pub fn new(sig: &Signature) -> Result<Target, String> {
        let name = format!("{}.{}", sig.module, sig.func);
        let mut subst = sig.forced_bindings();
        let mut vars = Vec::new();
        for p in &sig.params {
            p.collect_vars(&mut vars);
        }
        sig.ret.collect_vars(&mut vars);
        for v in vars {
            subst.entry(v).or_insert(GenType::Int);
        }
        let ret = sig.ret.instantiate(&subst).ok_or_else(|| format!("{name}: return type has no value form"))?;
        let params: Vec<(String, SigType)> = sig.param_names.iter().cloned().zip(sig.params.iter().cloned()).collect();
        let concrete = |t: &SigType| t.instantiate(&subst);
        let size_param = params
            .iter()
            .position(|(_, t)| matches!(concrete(t), Some(GenType::List(_) | GenType::Map(_) | GenType::String)))
            .or_else(|| params.iter().position(|(n, t)| *t == SigType::Int && SIZE_LIKE_INT.contains(&n.as_str())));
        let rebind_param = params.iter().position(|(_, t)| concrete(t).as_ref() == Some(&ret));
        Ok(Target { name, params, subst, ret, size_param, rebind_param })
    }

    fn ty(&self, i: usize) -> Option<GenType> {
        self.params[i].1.instantiate(&self.subst)
    }

    /// The call, with `slot` overriding parameter `i`'s argument text.
    fn call(&self, n: u64, slot: Option<(usize, &str)>) -> String {
        let args: Vec<String> = (0..self.params.len())
            .map(|i| match slot {
                Some((j, text)) if j == i => text.to_string(),
                _ => self.arg(i, n),
            })
            .collect();
        format!("{}({})", self.name, args.join(", "))
    }

    fn arg(&self, i: usize, n: u64) -> String {
        let (pname, t) = &self.params[i];
        match t {
            SigType::Fn(ps, r) => self.lambda(ps, r),
            SigType::Int if Some(i) == self.size_param => n.to_string(),
            SigType::Int => small_int(pname).into(),
            _ if SEARCH_PARAMS.contains(&pname.as_str()) => self.ty(i).map(|g| absent(&g)).unwrap_or_else(|| "()".into()),
            _ => match self.ty(i) {
                Some(g) if Some(i) == self.size_param => sized(&g, n),
                Some(g) => small(&g),
                None => "()".into(),
            },
        }
    }

    /// `(p0, ..) => body`: a parameter of the return type passes through,
    /// else a small value of it — the cheapest callback that keeps the types.
    fn lambda(&self, ps: &[SigType], r: &SigType) -> String {
        let names: Vec<String> = (0..ps.len()).map(|k| format!("p{k}")).collect();
        let want = r.instantiate(&self.subst);
        let body = ps
            .iter()
            .zip(&names)
            .find(|(p, _)| p.instantiate(&self.subst) == want)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| match want {
                // The worst case scans everything: a search that never hits,
                // a filter that is evaluated on every element.
                Some(GenType::Bool) if STOPS_ON_TRUE.contains(&self.func()) => "false".into(),
                Some(ref g) => small(g),
                None => "()".into(),
            });
        format!("({}) => {body}", names.join(", "))
    }

    /// Criterion 2, the plain call: `k` calls on one size-`n` input. With
    /// `with_call` false the same loop runs without the call, so the
    /// difference of the two timings is the calls' cost alone.
    pub fn direct_program(&self, n: u64, k: u64, with_call: bool) -> String {
        let mut s = String::from("effect fn main() -> Unit = {\n");
        // The input changes every iteration by an O(1) in-place step, run in
        // BOTH programs, so the call is never loop-invariant (a never-true
        // guard like `i < 0` was provable and hoisted through) and the step's
        // own cost cancels out of the difference. Alternating two inputs was
        // worse: native cloned the chosen list on every iteration (#3532).
        let slot = self.size_param.map(|i| {
            let _ = writeln!(s, "  var x: {} = {}", self.render_param(i), self.arg(i, n));
            (i, "x")
        });
        let _ = writeln!(s, "  var acc = 0\n  var i = 0\n  while i < {k} {{");
        if let Some((i, _)) = slot {
            let _ = writeln!(s, "    x = {}", vary("x", &self.ty(i).unwrap_or(GenType::Int), n));
        }
        if with_call {
            let _ = writeln!(s, "    let r: {} = {}", self.ret.render(), self.call(n, slot));
            let _ = writeln!(s, "    acc = acc + {}", digest("r", &self.ret));
        }
        let _ = writeln!(s, "    acc = acc + 1\n    i = i + 1\n  }}");
        if let Some(i) = self.size_param {
            let _ = writeln!(s, "  acc = acc + {}", digest("x", &self.ty(i).unwrap_or(GenType::Int)));
        }
        s.push_str("  println(\"${acc}\")\n}\n");
        s
    }

    /// Criterion 2, the use-shapes: `k` steps that rebind the return-typed
    /// parameter, starting from size `n` — the var's own rebind in a loop,
    /// a self tail call and a fold callback. Each must cost what one step
    /// of the in-place form costs, not a copy of the value.
    pub fn rebind_programs(&self, n: u64, k: u64) -> Vec<(&'static str, String)> {
        let Some(p) = self.rebind_param else { return Vec::new() };
        let ty = self.ret.render();
        let init = if Some(p) == self.size_param { self.arg(p, n) } else { sized(&self.ret, n) };
        let step = |v: &str| self.call(1, Some((p, v)));
        let d = digest("x", &self.ret);
        vec![
            (
                "loop",
                format!(
                    "effect fn main() -> Unit = {{\n  var x: {ty} = {init}\n  var i = 0\n  while i < {k} {{\n    x = {}\n    i = i + 1\n  }}\n  println(\"${{{d}}}\")\n}}\n",
                    step("x")
                ),
            ),
            (
                "tail",
                format!(
                    "fn go(x: {ty}, i: Int) -> {ty} = if i <= 0 then x else go({}, i - 1)\n\neffect fn main() -> Unit = {{\n  let x = go({init}, {k})\n  println(\"${{{d}}}\")\n}}\n",
                    step("x")
                ),
            ),
            (
                "fold",
                format!(
                    "effect fn main() -> Unit = {{\n  let x = list.range(0, {k}) |> list.fold({init}, (x, i) => {})\n  println(\"${{{d}}}\")\n}}\n",
                    step("x")
                ),
            ),
        ]
    }

    /// Criterion 5: `base` wraps the call, `left` and `right` each import
    /// `base` and wrap it again, and `main` imports both and also calls the
    /// function directly — three lines that must print the same value.
    pub fn diamond_project(&self, n: u64) -> Project {
        let sig: Vec<String> = (0..self.params.len()).map(|i| format!("a{i}: {}", self.render_param(i))).collect();
        let names: Vec<String> = (0..self.params.len()).map(|i| format!("a{i}")).collect();
        let ret = self.ret.render();
        let wrap = |inner: &str| format!("pub fn call({}) -> {ret} = {inner}({})\n", sig.join(", "), names.join(", "));
        let mut main = String::from("import self.left\nimport self.right\n\neffect fn main() -> Unit = {\n");
        for i in 0..self.params.len() {
            let _ = writeln!(main, "  let a{i}: {} = {}", self.render_param(i), self.arg(i, n));
        }
        let args = names.join(", ");
        let _ = writeln!(main, "  let d: {ret} = {}({args})\n  let l = left.call({args})\n  let r = right.call({args})", self.name);
        for v in ["d", "l", "r"] {
            let _ = writeln!(main, "  {}", render_print(v, &self.ret));
        }
        main.push_str("}\n");
        let files = vec![
            ("app/almide.toml".to_string(), "[package]\nname = \"app\"\nversion = \"0.1.0\"\n".to_string()),
            ("app/src/base.almd".to_string(), wrap(&self.name)),
            ("app/src/left.almd".to_string(), format!("import self.base\n\n{}", wrap("base.call"))),
            ("app/src/right.almd".to_string(), format!("import self.base\n\n{}", wrap("base.call"))),
            ("app/src/main.almd".to_string(), main),
        ];
        Project { files, entry: "app/src/main.almd".to_string() }
    }

    fn func(&self) -> &str {
        self.name.rsplit('.').next().unwrap_or("")
    }

    pub fn ret_is_observable(&self) -> bool {
        self.ret.is_observable()
    }

    fn render_param(&self, i: usize) -> String {
        render_sig(&self.params[i].1, &self.subst)
    }
}

fn render_sig(t: &SigType, subst: &HashMap<String, GenType>) -> String {
    match t {
        SigType::Fn(ps, r) => format!(
            "({}) -> {}",
            ps.iter().map(|p| render_sig(p, subst)).collect::<Vec<_>>().join(", "),
            render_sig(r, subst)
        ),
        other => other.instantiate(subst).map(|g| g.render()).unwrap_or_else(|| "Unit".into()),
    }
}

fn small_int(param: &str) -> &'static str {
    match param {
        "i" | "j" | "index" | "start" | "lo" => "0",
        _ => "2",
    }
}

/// A small literal of `t`.
fn small(t: &GenType) -> String {
    match t {
        GenType::Int => "2".into(),
        GenType::Float => "1.5".into(),
        GenType::String => "\"aé\"".into(),
        GenType::Bool => "true".into(),
        GenType::Unit => "()".into(),
        GenType::List(e) => format!("[{}, {}]", small(e), small(e)),
        GenType::Option(e) => format!("some({})", small(e)),
        GenType::Result(e) => format!("ok({})", small(e)),
        GenType::Tuple2(a, b) => format!("({}, {})", small(a), small(b)),
        GenType::Map(v) => format!("[\"k\": {}]", small(v)),
    }
}

/// A value of `t` no sized input holds (`elem` makes Ints >= 0, strings
/// `s<i>`): the miss that makes a search scan everything.
fn absent(t: &GenType) -> String {
    match t {
        GenType::Int => "-1".into(),
        GenType::String => "\"absent\"".into(),
        other => small(other),
    }
}

/// An element of `t` that varies with the loop index `i`.
fn elem(t: &GenType) -> String {
    match t {
        GenType::Int => "i".into(),
        GenType::Float => "int.to_float(i) * 0.5".into(),
        GenType::String => "\"s${i}\"".into(),
        GenType::Bool => "i % 2 == 0".into(),
        GenType::List(e) => format!("[{}]", elem(e)),
        GenType::Option(e) => format!("some({})", elem(e)),
        GenType::Tuple2(a, b) => format!("({}, {})", elem(a), elem(b)),
        other => small(other),
    }
}

/// A value of `t` whose size is `n`.
fn sized(t: &GenType, n: u64) -> String {
    match t {
        GenType::Int => n.to_string(),
        GenType::String => format!("string.repeat(\"aé\", {n})"),
        GenType::List(e) => format!("list.range(0, {n}) |> list.map((i) => {})", elem(e)),
        GenType::Map(v) => format!("list.range(0, {n}) |> list.fold(map.new(), (m, i) => map.set(m, \"k${{i}}\", {}))", elem(v)),
        other => small(other),
    }
}

/// An O(1) in-place change of the size-`n` value `v` that depends on the
/// loop index `i` and keeps its size.
fn vary(v: &str, t: &GenType, n: u64) -> String {
    match t {
        GenType::Int => format!("{n} + i % 2"),
        GenType::List(e) => format!("list.set({v}, 0, {})", elem(e)),
        GenType::Map(e) => format!("map.set({v}, \"k0\", {})", elem(e)),
        // A string has no O(1) in-place edit; the same value is rebound.
        _ => v.to_string(),
    }
}

/// An Int that depends on the whole of `v`, so no call is dead.
fn digest(v: &str, t: &GenType) -> String {
    match t {
        GenType::Int => v.into(),
        GenType::Float => format!("(if {v} > 0.0 then 1 else 0)"),
        GenType::String => format!("string.len({v})"),
        GenType::Bool => format!("(if {v} then 1 else 0)"),
        GenType::Unit => "0".into(),
        GenType::List(_) => format!("list.len({v})"),
        GenType::Map(_) => format!("map.len({v})"),
        GenType::Option(_) => format!("(if option.is_some({v}) then 1 else 0)"),
        GenType::Result(_) => format!("(if result.is_ok({v}) then 1 else 0)"),
        GenType::Tuple2(..) => "1".into(),
    }
}
