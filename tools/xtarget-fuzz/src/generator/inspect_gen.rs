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
    /// The parameter scaled to `n` (the first List / Map / Set / String / Bytes / Matrix / Value one, else a
    /// size-like Int); `None`: the function has no size to scale.
    pub size_param: Option<usize>,
    /// A parameter of the return type: the function can rebind it in a loop,
    /// a tail call and a fold (criterion 2's use-shapes).
    pub rebind_param: Option<usize>,
    /// The `mut` parameter: bound with `var`, and in the diamond each path
    /// mutates its own copy and prints it.
    mut_param: Option<usize>,
    /// The function's [`SHAPE_RECIPES`] entry, if it has one.
    recipe: Option<&'static Recipe>,
}

const SIZE_LIKE_INT: &[&str] = &["n", "count", "times", "len", "size", "end", "width"];

/// Element-typed parameters a search compares against: given a value no
/// sized input holds, so the search scans to the end (its worst case).
const SEARCH_PARAMS: &[&str] = &["x", "value", "elem", "item", "needle", "target", "sub", "pattern", "from"];

/// The arguments of a function whose operands must agree in shape (#3637):
/// each named parameter takes its expression, `{n}` standing for the size,
/// and `size` is the parameter the size is measured in. The generic values
/// (`matrix.zeros(n, 2)`, 2×2 operands, 2 for every count) violate these
/// functions' preconditions, so every shape exited 1.
#[derive(Debug)]
struct Recipe {
    func: &'static str,
    size: &'static str,
    args: &'static [(&'static str, &'static str)],
}

/// Rope: 2 heads × head_dim 2 fill the 4 columns; n is the row (position)
/// count.
const ROPE: &[(&str, &str)] = &[("x", "matrix.zeros({n}, 4)"), ("n_heads", "2"), ("head_dim", "2")];

const SHAPE_RECIPES: &[Recipe] = &[
    Recipe { func: "matrix.rope_rotate", size: "x", args: ROPE },
    Recipe { func: "matrix.rope_rotate_at", size: "x", args: ROPE },
    Recipe { func: "matrix.rope_rotate_neox_at", size: "x", args: ROPE },
    // Causal attention needs rows(q) <= rows(k), and k / v as wide as q:
    // n is the key/value length (a KV cache) against 2 query rows, 4 wide
    // over 2 heads. The rebound q keeps its 2 × 4 shape.
    Recipe {
        func: "matrix.masked_multi_head_attention",
        size: "k",
        args: &[("q", "matrix.zeros(2, 4)"), ("k", "matrix.zeros({n}, 4)"), ("v", "matrix.zeros({n}, 4)"), ("n_heads", "2")],
    },
    // A (T, in_ch) input, an (out_ch, in_ch × kernel) weight, one bias per
    // out_ch; kernel 3, stride 1, padding 1 give T output rows of out_ch =
    // in_ch, so the rebound input keeps its shape. n is T.
    Recipe {
        func: "matrix.conv1d",
        size: "input",
        args: &[
            ("input", "matrix.zeros({n}, 2)"),
            ("weight", "matrix.zeros(2, 6)"),
            ("bias", "[0.5, 1.5]"),
            ("kernel", "3"),
            ("stride", "1"),
            ("padding", "1"),
        ],
    },
];

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
        let recipe = SHAPE_RECIPES.iter().find(|r| r.func == name);
        let param_at = |p: &str| params.iter().position(|(n, _)| n == p);
        if let Some(r) = recipe {
            // A recipe naming a parameter the signature no longer has would
            // silently fall back to the generic values that violate it.
            if let Some((p, _)) = r.args.iter().find(|(p, _)| param_at(p).is_none()) {
                return Err(format!("{name}: shape recipe names `{p}`, which is not a parameter"));
            }
            if !r.args.iter().any(|(p, e)| *p == r.size && e.contains("{n}")) {
                return Err(format!("{name}: shape recipe's size `{}` does not take n", r.size));
            }
        }
        let size_param = match recipe {
            Some(r) => param_at(r.size),
            None => params
                .iter()
                .position(|(_, t)| matches!(concrete(t), Some(GenType::List(_) | GenType::Map(_) | GenType::Set(_) | GenType::String | GenType::Bytes | GenType::Matrix | GenType::Value)))
                .or_else(|| params.iter().position(|(n, t)| *t == SigType::Int && SIZE_LIKE_INT.contains(&n.as_str()))),
        };
        let rebind_param = params.iter().position(|(_, t)| concrete(t).as_ref() == Some(&ret));
        Ok(Target { name, params, subst, ret, size_param, rebind_param, mut_param: sig.mut_param, recipe })
    }

    /// The recipe's expression for parameter `i`, if it names it.
    fn recipe_arg(&self, i: usize) -> Option<&'static str> {
        let pname = &self.params[i].0;
        self.recipe?.args.iter().find(|(p, _)| p == pname).map(|(_, e)| *e)
    }

    /// Parameter `i`'s value grows with n.
    fn scales(&self, i: usize) -> bool {
        Some(i) == self.size_param || self.recipe_arg(i).is_some_and(|e| e.contains("{n}"))
    }

    fn ty(&self, i: usize) -> Option<GenType> {
        self.params[i].1.instantiate(&self.subst)
    }

    fn arg(&self, i: usize, n: u64) -> String {
        let (pname, t) = &self.params[i];
        if let Some(e) = self.recipe_arg(i) {
            return e.replace("{n}", &n.to_string());
        }
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
        // A string has no O(1) in-place step, and `x = x` is not free on
        // every leg: it is bound once and left alone.
        let slot = self.size_param.map(|i| {
            let varies = !matches!(self.ty(i), Some(GenType::String));
            let kw = if varies || Some(i) == self.mut_param { "var" } else { "let" };
            let _ = writeln!(s, "  {kw} x: {} = {}", self.render_param(i), self.arg(i, n));
            (i, varies)
        });
        // Every other value argument is bound to a typed `let` first: a bare
        // `ok(2)` leaves its error type unconstrained (E025).
        s.push_str(&self.arg_lets(n, self.size_param));
        let _ = writeln!(s, "  var acc = 0\n  var i = 0\n  while i < {k} {{");
        if let Some((i, true)) = slot {
            let _ = writeln!(s, "    x = {}", vary("x", &self.ty(i).unwrap_or(GenType::Int), n));
        }
        if with_call {
            let _ = writeln!(s, "    let r: {} = {}", self.ret.render(), self.bound_call(slot.map(|(i, _)| i)));
            let _ = writeln!(s, "    acc = acc + {}", digest("r", &self.ret));
        }
        let _ = writeln!(s, "    acc = acc + 1\n    i = i + 1\n  }}");
        if let Some(i) = self.size_param {
            let _ = writeln!(s, "  acc = acc + {}", digest("x", &self.ty(i).unwrap_or(GenType::Int)));
        }
        s.push_str("  println(\"${acc}\")\n}\n");
        format!("{}{s}", super::catalogue::imports_for(&s))
    }

    /// Criterion 2, the use-shapes: `k` steps that rebind the return-typed
    /// parameter, starting from size `n` — the var's own rebind in a loop,
    /// a self tail call and a fold callback. Each must cost what one step
    /// of the in-place form costs, not a copy of the value.
    pub fn rebind_programs(&self, n: u64, k: u64) -> Vec<(&'static str, String)> {
        let Some(p) = self.rebind_param else { return Vec::new() };
        let ty = self.ret.render();
        let init = if Some(p) == self.size_param || self.recipe_arg(p).is_some() { self.arg(p, n) } else { sized(&self.ret, n) };
        // The other value arguments come from typed top-level fns, which the
        // tail shape's `go` can see as well as `main` (an inline `ok(2)`
        // leaves its error type unconstrained, E025). They are small, except
        // a recipe's operand that must keep its shape against the others.
        let mut consts = String::new();
        for i in (0..self.params.len()).filter(|i| *i != p && !self.is_fn(*i)) {
            let m = if self.recipe_arg(i).is_some() && self.scales(i) { n } else { 1 };
            let _ = writeln!(consts, "fn a{i}() -> {} = {}\n", self.render_param(i), self.arg(i, m));
        }
        let step = |v: &str| {
            let args: Vec<String> = (0..self.params.len())
                .map(|i| match i {
                    _ if i == p => v.to_string(),
                    _ if self.is_fn(i) => self.arg(i, 1),
                    _ => format!("a{i}()"),
                })
                .collect();
            format!("{}({})", self.name, args.join(", "))
        };
        let d = digest("x", &self.ret);
        let programs = vec![
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
                    "fn go(x: {ty}, i: Int) -> {ty} = if i <= 0 then x else go({}, i - 1)\n\neffect fn main() -> Unit = {{\n  let x0: {ty} = {init}\n  let x: {ty} = go(x0, {k})\n  println(\"${{{d}}}\")\n}}\n",
                    step("x")
                ),
            ),
            (
                "fold",
                format!(
                    "effect fn main() -> Unit = {{\n  let x0: {ty} = {init}\n  let x: {ty} = list.range(0, {k}) |> list.fold(x0, (x, i) => {})\n  println(\"${{{d}}}\")\n}}\n",
                    step("x")
                ),
            ),
        ];
        programs
            .into_iter()
            .map(|(label, src)| {
                let body = format!("{consts}{src}");
                (label, format!("{}{body}", super::catalogue::imports_for(&body)))
            })
            .collect()
    }

    /// Criterion 5: `base` wraps the call, `left` and `right` each import
    /// `base` and wrap it again, and `main` imports both and also calls the
    /// function directly — three lines that must print the same value.
    pub fn diamond_project(&self, n: u64) -> Project {
        let sig: Vec<String> = (0..self.params.len())
            .map(|i| format!("{}a{i}: {}", if Some(i) == self.mut_param { "mut " } else { "" }, self.render_param(i)))
            .collect();
        let names: Vec<String> = (0..self.params.len()).map(|i| format!("a{i}")).collect();
        let ret = self.ret.render();
        let wrap = |inner: &str| format!("pub fn call({}) -> {ret} = {inner}({})\n", sig.join(", "), names.join(", "));
        let mut main = String::from("import self.left\nimport self.right\n\neffect fn main() -> Unit = {\n");
        for i in (0..self.params.len()).filter(|i| Some(*i) != self.mut_param) {
            let _ = writeln!(main, "  let a{i}: {} = {}", self.render_param(i), self.arg(i, n));
        }
        // Each path mutates its own var, so the three must agree on the
        // mutated value as well as on the return.
        let path_args = |path: &str| -> String {
            (0..self.params.len())
                .map(|i| if Some(i) == self.mut_param { format!("{path}m") } else { format!("a{i}") })
                .collect::<Vec<_>>()
                .join(", ")
        };
        for (v, callee) in [("d", self.name.as_str()), ("l", "left.call"), ("r", "right.call")] {
            if let Some(m) = self.mut_param {
                let _ = writeln!(main, "  var {v}m: {} = {}", self.render_param(m), self.arg(m, n));
            }
            let _ = writeln!(main, "  let {v}: {ret} = {callee}({})", path_args(v));
        }
        for v in ["d", "l", "r"] {
            // A Unit return has no string form (E089) and nothing to compare.
            if self.ret != GenType::Unit {
                let _ = writeln!(main, "  {}", render_print(v, &self.ret));
            }
            if let Some(m) = self.mut_param {
                let _ = writeln!(main, "  {}", render_print(&format!("{v}m"), &self.ty(m).unwrap_or(GenType::Unit)));
            }
        }
        main.push_str("}\n");
        let main = format!("{}{main}", super::catalogue::imports_for(&main));
        let base = wrap(&self.name);
        let files = vec![
            ("app/almide.toml".to_string(), "[package]\nname = \"app\"\nversion = \"0.1.0\"\n".to_string()),
            ("app/src/base.almd".to_string(), format!("{}{base}", super::catalogue::imports_for(&base))),
            ("app/src/left.almd".to_string(), format!("import self.base\n\n{}", wrap("base.call"))),
            ("app/src/right.almd".to_string(), format!("import self.base\n\n{}", wrap("base.call"))),
            ("app/src/main.almd".to_string(), main),
        ];
        Project { files, entry: "app/src/main.almd".to_string() }
    }

    /// `let a<i>: T = <arg>` for every value parameter other than `skip`
    /// (callbacks stay inline: they are not values a `let` can type here).
    fn arg_lets(&self, n: u64, skip: Option<usize>) -> String {
        let mut s = String::new();
        for i in (0..self.params.len()).filter(|i| Some(*i) != skip && !self.is_fn(*i)) {
            let kw = if Some(i) == self.mut_param { "var" } else { "let" };
            let _ = writeln!(s, "  {kw} a{i}: {} = {}", self.render_param(i), self.arg(i, n));
        }
        s
    }

    /// The call over the `let`-bound arguments of [`Self::arg_lets`], `x` in
    /// the `slot` position and callbacks inline.
    fn bound_call(&self, slot: Option<usize>) -> String {
        let args: Vec<String> = (0..self.params.len())
            .map(|i| match i {
                _ if Some(i) == slot => "x".to_string(),
                _ if self.is_fn(i) => self.arg(i, 0),
                _ => format!("a{i}"),
            })
            .collect();
        format!("{}({})", self.name, args.join(", "))
    }

    fn is_fn(&self, i: usize) -> bool {
        matches!(self.params[i].1, SigType::Fn(..))
    }

    fn func(&self) -> &str {
        self.name.rsplit('.').next().unwrap_or("")
    }

    /// The return value can be printed: an observable type, or a top-level
    /// `Bytes` (printed as its byte list, see [`render_print`]).
    pub fn ret_is_observable(&self) -> bool {
        self.ret.observe("v", 0).is_some()
    }

    /// The diamond prints something: a non-Unit return, or a `mut`
    /// parameter's mutated copy.
    pub fn diamond_prints(&self) -> bool {
        self.ret != GenType::Unit || self.mut_param.is_some()
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
        "i" | "j" | "index" | "start" | "lo" | "row" | "col" => "0",
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
        GenType::Sized(name) => GenType::sized_literal(name, "2"),
        GenType::Bytes => "bytes.from_list([1, 2])".into(),
        GenType::Matrix => "matrix.from_lists([[1.5, 2.0], [0.5, 3.0]])".into(),
        GenType::Value => "value.object([(\"a\", value.int(1)), (\"snake_case\", value.str(\"x\"))])".into(),
        GenType::Endian => "BigEndian".into(),
        GenType::Set(e) => format!("set.from_list([{}])", small(e)),
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
        GenType::String => "\"s${100000000 + i}\"".into(),
        GenType::Bool => "i % 2 == 0".into(),
        GenType::List(e) => format!("[{}]", elem(e)),
        GenType::Option(e) => format!("some({})", elem(e)),
        GenType::Tuple2(a, b) => format!("({}, {})", elem(a), elem(b)),
        GenType::Sized(name) => GenType::sized_literal(name, "i % 100"),
        GenType::Set(e) => format!("set.from_list([{}])", elem(e)),
        other => small(other),
    }
}

/// A value of `t` whose size is `n`.
fn sized(t: &GenType, n: u64) -> String {
    match t {
        GenType::Int => n.to_string(),
        GenType::String => format!("string.repeat(\"aé\", {n})"),
        GenType::List(e) => format!("list.range(0, {n}) |> list.map((i) => {})", elem(e)),
        GenType::Map(v) => format!("list.range(0, {n}) |> list.fold(map.new(), (m, i) => map.set(m, \"k${{100000000 + i}}\", {}))", elem(v)),
        GenType::Bytes => format!("bytes.new({n})"),
        GenType::Matrix => format!("matrix.zeros({n}, 2)"),
        // An object of n keys: what `field` / `keys` / `pick` / `omit` /
        // `merge` and the case converters walk. Keys and values are fixed
        // width, so its stringified length is a·n + b (#3636).
        GenType::Value => format!("value.object(list.range(0, {n}) |> list.map((i) => (\"k${{100000000 + i}}\", value.int(100000000 + i))))"),
        GenType::Set(e) => format!("list.range(0, {n}) |> list.map((i) => {}) |> set.from_list", elem(e)),
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
        GenType::Sized(name) if name.starts_with("Float") => {
            format!("(if float.from_{}({v}) > 0.0 then 1 else 0)", name.to_lowercase())
        }
        GenType::Sized(name) => format!("int.from_{}({v})", name.to_lowercase()),
        GenType::Bytes => format!("bytes.len({v})"),
        GenType::Matrix => format!("matrix.rows({v})"),
        GenType::Value => format!("string.len(value.stringify({v}))"),
        GenType::Endian => "1".into(),
        GenType::Set(_) => format!("set.len({v})"),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The catalogued function `name`, instantiated (for tests elsewhere too).
    pub(crate) fn target(name: &str) -> Target {
        let sig = crate::generator::catalogue::build()
            .into_iter()
            .find(|s| format!("{}.{}", s.module, s.func) == name)
            .unwrap_or_else(|| panic!("{name} not catalogued"));
        Target::new(&sig).expect("target")
    }

    /// #3636: the sized Value's stringified length must be a·n + b. With
    /// `value.int(i)` the digit width grew across the sweep (crossing 99999
    /// between 20k and 320k), so json.stringify / value.stringify /
    /// value.omit fit n at 2.9–3.8% rms against a declared n.
    #[test]
    fn the_sized_value_holds_fixed_width_ints() {
        let s = sized(&GenType::Value, 1000);
        assert!(s.contains("value.int(100000000 + i)"), "{s}");
        assert!(!s.contains("value.int(i)"), "{s}");
    }

    /// #3637: matrix functions with head or kernel geometry got
    /// `matrix.zeros(n, 2)` with every other operand 2×2 and 2 for every
    /// count, so heads × dim = 4 overran the 2 columns and the q/k/v and
    /// conv1d shapes did not line up — every shape exited 1. Each gets
    /// operands that agree, with n on an axis the function accepts, in the
    /// plain call, the rebind shapes and the diamond alike.
    #[test]
    fn head_and_kernel_geometry_functions_get_operands_that_agree() {
        for name in ["matrix.rope_rotate", "matrix.rope_rotate_at", "matrix.rope_rotate_neox_at"] {
            let t = target(name);
            let direct = t.direct_program(1000, 4, true);
            // 2 heads × head_dim 2 = the 4 columns.
            assert!(direct.contains("  var x: Matrix = matrix.zeros(1000, 4)"), "{direct}");
            assert!(direct.contains("  let a1: Int = 2\n  let a2: Int = 2\n"), "{direct}");
            let tail = &t.rebind_programs(1000, 4)[1].1;
            assert!(tail.contains("let x0: Matrix = matrix.zeros(1000, 4)") && tail.contains("fn a1() -> Int = 2"), "{tail}");
        }
        // Causal attention needs rows(q) <= rows(k): n is the key/value
        // length (a KV cache), q a fixed 2 rows, all 4 wide over 2 heads.
        let t = target("matrix.masked_multi_head_attention");
        let direct = t.direct_program(1000, 4, true);
        assert!(direct.contains("  var x: Matrix = matrix.zeros(1000, 4)"), "{direct}");
        assert!(direct.contains("  let a0: Matrix = matrix.zeros(2, 4)"), "{direct}");
        assert!(direct.contains("  let a2: Matrix = matrix.zeros(1000, 4)"), "{direct}");
        assert!(direct.contains("  let a3: Int = 2"), "{direct}");
        assert!(direct.contains("matrix.masked_multi_head_attention(a0, x, a2, a3)"), "{direct}");
        let fold = &t.rebind_programs(1000, 4)[2].1;
        assert!(fold.contains("let x0: Matrix = matrix.zeros(2, 4)"), "{fold}");
        assert!(fold.contains("fn a1() -> Matrix = matrix.zeros(1000, 4)") && fold.contains("fn a2() -> Matrix = matrix.zeros(1000, 4)"), "{fold}");
        // conv1d: (T, in_ch) input, (out_ch, in_ch × kernel) weight, one
        // bias per out_ch; kernel 3, stride 1, padding 1 keep T rows out.
        let t = target("matrix.conv1d");
        let direct = t.direct_program(1000, 4, true);
        for want in [
            "  var x: Matrix = matrix.zeros(1000, 2)",
            "  let a1: Matrix = matrix.zeros(2, 6)",
            "  let a2: List[Float] = [0.5, 1.5]",
            "  let a3: Int = 3",
            "  let a4: Int = 1",
            "  let a5: Int = 1",
        ] {
            assert!(direct.contains(want), "{want}: {direct}");
        }
        let project = t.diamond_project(64);
        let main = &project.files.iter().find(|(p, _)| p.ends_with("main.almd")).unwrap().1;
        assert!(main.contains("let a0: Matrix = matrix.zeros(64, 2)") && main.contains("let a1: Matrix = matrix.zeros(2, 6)"), "{main}");
    }

    /// #3666: the decoders' sized String was `string.repeat("aé", n)`, which
    /// none of them accepts, so every shape timed the early-error exit (flat,
    /// ×1.0 per doubling). Each gets valid input of size n — encoded bytes,
    /// 2n hex digits, a JSON array of n fixed-width ints — in the plain call
    /// and the diamond alike.
    #[test]
    fn decoders_are_sized_with_input_they_accept() {
        for (name, want) in [
            ("base64.decode", "base64.encode(bytes.from_list(list.range(0, 1000) |> list.map((i) => i % 256)))"),
            ("base64.decode_url", "base64.encode_url(bytes.from_list(list.range(0, 1000) |> list.map((i) => i % 256)))"),
            ("hex.decode", "hex.encode(bytes.from_list(list.range(0, 1000) |> list.map((i) => i % 256)))"),
            ("json.parse", "\"[\" + (list.range(0, 1000) |> list.map((i) => int.to_string(100000000 + i)) |> list.join(\",\")) + \"]\""),
        ] {
            let t = target(name);
            let direct = t.direct_program(1000, 4, true);
            assert!(direct.contains(&format!("  let x: String = {want}\n")), "{name}: {direct}");
            assert!(!direct.contains("string.repeat"), "{name}: {direct}");
            let project = t.diamond_project(1000);
            let main = &project.files.iter().find(|(p, _)| p.ends_with("main.almd")).unwrap().1;
            assert!(main.contains(&format!("  let a0: String = {want}\n")), "{name}: {main}");
        }
    }

    /// A recipe that drifted from its function's signature is an error, not
    /// a silent fall-back to the shapes that function refuses.
    #[test]
    fn every_shape_recipe_names_a_catalogued_function_and_its_parameters() {
        for r in SHAPE_RECIPES {
            let t = target(r.func);
            assert!(t.recipe.is_some() && t.size_param.is_some(), "{}", r.func);
        }
    }

    /// A `mut`-parameter mutator: its sized input is a `var` the loop calls
    /// it on, and each diamond path mutates its own `var` and prints it.
    #[test]
    fn a_mutator_is_called_on_a_var_and_each_diamond_path_prints_its_own_copy() {
        let t = target("bytes.push");
        let direct = t.direct_program(1000, 4, true);
        assert!(direct.contains("  var x: Bytes = bytes.new(1000)"), "{direct}");
        assert!(direct.contains("let r: Unit = bytes.push(x, "), "{direct}");
        let project = t.diamond_project(1000);
        let file = |suffix: &str| project.files.iter().find(|(p, _)| p.ends_with(suffix)).map(|(_, s)| s.clone()).unwrap();
        assert!(file("base.almd").contains("pub fn call(mut a0: Bytes, a1: Int) -> Unit = bytes.push(a0, a1)"), "{}", file("base.almd"));
        let main = file("main.almd");
        for v in ["d", "l", "r"] {
            assert!(main.contains(&format!("  var {v}m: Bytes = bytes.new(1000)")), "{main}");
            assert!(main.contains(&format!("let {v}m_o: List[Int] = bytes.to_list({v}m)\n  println(\"{v}m = ${{{v}m_o}}\")")), "{main}");
        }
        assert!(!main.contains("println(\"d = "), "a Unit return is not printed (E089): {main}");
        if let Ok(dir) = std::env::var("XF_DUMP") {
            std::fs::write(format!("{dir}/direct.almd"), &direct).unwrap();
            for (p, s) in &project.files {
                let path = std::path::Path::new(&dir).join(p);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, s).unwrap();
            }
        }
    }
}
