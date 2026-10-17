//! The typed term generator — builds an Almide expression of a *given*
//! goal type, well-typed by construction.
//!
//! ## Why a statement builder, not pure expressions
//!
//! Several Almide values are *ambiguous in bare expression position* and
//! only type-check with a binding annotation: `[]`, `[:]`, `none`, and
//! the `ok`/`err`/`some` constructors all need a `let v: T = …` context
//! for the checker to fix their element/error type. Emitting them inline
//! inside a larger expression fails `almide check`.
//!
//! So the generator threads a [`Builder`] that can *hoist* such a value
//! into a fresh, annotated `let` statement and return a reference to it.
//! The whole generated `main` body becomes a sequence of typed `let`
//! bindings (each able to reference earlier ones, building real
//! data-flow) terminated by `println` observables. This is both more
//! robust and a more realistic program shape than deeply nested
//! one-liners.
//!
//! ## Fuel
//!
//! Generation is fuel-bounded: `Builder::fuel` decrements on each
//! structural step, and at zero fuel the generator falls back to the
//! cheapest leaf for the goal type. This bounds program size and
//! guarantees termination without a magic depth cap.

use std::collections::HashMap;

use crate::rng::SplitMix64;

use super::catalogue::Signature;
use super::pools;
use super::sig_type::SigType;
use super::types::GenType;

/// One variable currently in lexical scope.
#[derive(Clone)]
pub struct ScopeVar {
    pub name: String,
    pub ty: GenType,
}

/// Accumulates the statements of the program body as the term generator
/// runs, and owns all shared generation state.
pub struct Builder<'a> {
    pub rng: &'a mut SplitMix64,
    /// Catalogued stdlib signatures (immutable for the whole campaign).
    pub catalogue: &'a [Signature],
    /// Variables in scope, innermost last.
    pub scope: Vec<ScopeVar>,
    /// Statements emitted so far (rendered Almide source lines, no
    /// trailing newline). The driver wraps these in `fn main`.
    pub stmts: Vec<String>,
    /// Remaining structural fuel; productions that recurse spend it.
    pub fuel: i32,
    /// Monotonic counter for fresh binding names (`v0`, `v1`, …).
    pub fresh: u32,
    /// Set once an effect function is called: `main` must be an `effect fn`.
    pub effect: bool,
}

/// The maximum nesting fuel a single top-level expression starts with.
/// Chosen so terms reach interesting depth (a HOF over a mapped list)
/// without exploding compile time.
pub const START_FUEL: i32 = 6;

/// Goal-type recursion cap for the random goal types the driver picks,
/// so we never aim for e.g. `List[Map[String, Option[List[Int]]]]`.
pub const MAX_GOAL_DEPTH: u32 = 3;

impl<'a> Builder<'a> {
    /// Mint a fresh, unique variable name from a random stem.
    fn fresh_name(&mut self) -> String {
        let stem = self.rng.pick(pools::VAR_STEMS);
        let name = format!("{stem}{}", self.fresh);
        self.fresh += 1;
        name
    }

    /// In-scope variables whose type exactly equals `goal`.
    fn vars_of_type(&self, goal: &GenType) -> Vec<usize> {
        self.scope
            .iter()
            .enumerate()
            .filter(|(_, v)| &v.ty == goal)
            .map(|(i, _)| i)
            .collect()
    }

    /// Hoist `value_src` into a fresh annotated `let v: ty = value_src`
    /// statement, register the binding in scope, and return the name.
    /// Annotation is what makes ambiguous literals (`[]`, `none`, …)
    /// type-check.
    fn hoist(&mut self, ty: &GenType, value_src: String) -> String {
        let name = self.fresh_name();
        self.stmts
            .push(format!("let {name}: {} = {value_src}", ty.render()));
        self.scope.push(ScopeVar {
            name: name.clone(),
            ty: ty.clone(),
        });
        name
    }

    /// Hoist `value_src` into a fresh `var v: ty = value_src` for a `mut`
    /// parameter, and return the name. The var is in scope afterwards like a
    /// `let` (reading it later observes the mutation).
    fn hoist_var(&mut self, ty: &GenType, value_src: String) -> String {
        let name = self.fresh_name();
        self.stmts.push(format!("var {name}: {} = {value_src}", ty.render()));
        self.scope.push(ScopeVar { name: name.clone(), ty: ty.clone() });
        name
    }
}

/// Render a call of `sig` at generated arguments, as an expression of its
/// [`Signature::effective_ret`]. A `mut` argument is hoisted into a fresh
/// `var` (from a generated value, so the original value it was built from
/// must stay unchanged: value semantics is part of what is tested); a
/// `Unit`-returning mutator runs as a statement and the expression is the
/// mutated var.
fn render_call(b: &mut Builder, sig: &Signature, subst: &HashMap<String, GenType>) -> Option<String> {
    let mut args = Vec::with_capacity(sig.params.len());
    for (i, p) in sig.params.iter().enumerate() {
        let name = sig.param_names.get(i).map(String::as_str).unwrap_or("");
        if let Some(a) = net_int_arg(sig, name).filter(|_| *p == SigType::Int) {
            args.push(a);
            continue;
        }
        // cat with no file arguments reads its whole stdin: with arguments
        // naming missing files it exits before reading, and whether the
        // input write then sees EPIPE is timing, on every leg.
        if sig.module == "process" && sig.func == "exec_with_stdin" && name == "args" {
            args.push("[]".into());
            continue;
        }
        match sandbox_arg(b, sig, p, name) {
            Some(a) => args.push(a),
            None => args.push(gen_arg(b, p, name, subst)?),
        }
    }
    if let Some(v) = sig.verdict {
        return render_verdict_call(b, sig, v, args, subst);
    }
    if sig.effect {
        // Bound to an annotated `let`: an effect call's Result is then a
        // value (an un-annotated bind is E041), and its error path is
        // observed instead of propagated.
        b.effect = true;
        let ty = sig.effective_ret().instantiate(subst)?;
        let call = format!("{}.{}({})", sig.module, sig.func, args.join(", "));
        // An encoder whose bytes differ by leg is observed decoded.
        let call = match sig.observed_through {
            Some(dec) => format!("match {call} {{ ok(zc) => {dec}(zc), err(ze) => err(ze) }}"),
            None => call,
        };
        // A `Unit` effect (`env.set`, `io.print`) is a statement; its value is `()`.
        if ty == GenType::Unit {
            b.stmts.push(call);
            return Some("()".to_string());
        }
        return Some(b.hoist(&ty, call));
    }
    let Some(m) = sig.mut_param else {
        return Some(format!("{}.{}({})", sig.module, sig.func, args.join(", ")));
    };
    let ty = sig.params[m].instantiate(subst)?;
    let var = b.hoist_var(&ty, args[m].clone());
    args[m] = var.clone();
    let call = format!("{}.{}({})", sig.module, sig.func, args.join(", "));
    if sig.ret == SigType::Unit {
        b.stmts.push(call);
        Some(var)
    } else {
        Some(call)
    }
}

/// A host-decided call (ADR-0027 D3, `verdicts.rs`): every argument is bound
/// once (the checks read it again), brought into the function's domain, and
/// the result is checked against its properties, then replaced by the fixed
/// value every leg agrees on. A leg whose result breaks a property panics.
fn render_verdict_call(
    b: &mut Builder,
    sig: &Signature,
    v: &super::verdicts::Verdict,
    args: Vec<String>,
    subst: &HashMap<String, GenType>,
) -> Option<String> {
    let mut bound: Vec<(String, String)> = Vec::new();
    for ((name, p), a) in sig.param_names.iter().zip(&sig.params).zip(args) {
        let ty = p.instantiate(subst)?;
        let a = match v.domain.iter().find(|(d, _)| d == name) {
            Some((_, tmpl)) => super::verdicts::fill(tmpl, &[(name, &a)]),
            None => a,
        };
        // Out of scope like the raw result: a domain operand can be a host
        // choice too (`process.kill`'s pid is a fresh child's).
        let v = b.fresh_name();
        b.stmts.push(format!("let {v}: {} = {a}", ty.render()));
        bound.push((name.clone(), v));
    }
    let names: Vec<(&str, &str)> = bound.iter().map(|(p, e)| (p.as_str(), e.as_str())).collect();
    let call = format!("{}.{}({})", sig.module, sig.func, bound.iter().map(|(_, e)| e.as_str()).collect::<Vec<_>>().join(", "));
    let ty = sig.effective_ret().instantiate(subst)?;
    if sig.effect {
        b.effect = true;
    }
    if ty == GenType::Unit {
        b.stmts.push(call);
        return Some("()".to_string());
    }
    // Bound out of scope: the raw value is read only by its check, never by
    // a later term (it differs per leg and per run).
    let r = b.fresh_name();
    b.stmts.push(format!("let {r}: {} = {call}", ty.render()));
    let holds = super::verdicts::fill(v.holds, &names);
    let canon = super::verdicts::fill(v.canon, &names);
    let broke = format!("panic(\"verdict: {}.{} broke its declared property\")", sig.module, sig.func);
    let checked = match ty {
        // The error names what the host chose too (a temp path, its clock
        // suffix), so only that the call failed is compared.
        GenType::Result(_) => format!("match {r} {{ ok(v) => if {holds} then ok({canon}) else {broke}, err(_) => err(\"{}.{} failed\") }}", sig.module, sig.func),
        _ => format!("match {r} {{ v => if {holds} then {canon} else {broke} }}"),
    };
    Some(b.hoist(&ty, checked))
}

const HTTP_PATHS: &[&str] = &["/echo", "/status/404", "/status/500", "/bytes/16", "/chunked/3", "/sse/2", "/missing"];
const HTTP_METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE"];

/// `http://<the fixture>/<path>`, read from the run's env.
fn fixture_url(path: &str) -> String {
    format!("\"http://\" + (env.get(\"XF_HTTP\") ?? \"\") + {}", render_string_literal(path))
}

/// An Int argument of a net function that names the fixture's port, a
/// listener's own port (0) or a live handle: a generated Int would dial or
/// close an arbitrary socket of the host.
fn net_int_arg(sig: &Signature, name: &str) -> Option<String> {
    let tcp_port = "(int.parse(env.get(\"XF_TCP_PORT\") ?? \"0\") ?? 0)";
    // A stream to the echo fixture with 11 bytes on their way back, and a
    // listener with a connection already pending: a read or an accept
    // against nothing would wait forever.
    let stream = format!(
        "(match net.tcp_connect(\"127.0.0.1\", {tcp_port}) {{ ok(s) => {{ let _w: Result[Unit, String] = net.tcp_write(s, bytes.from_string(\"hello world\"))\n s }}, err(_) => -1 }})"
    );
    let listener = "(match net.tcp_listen(\"127.0.0.1\", 0) { ok(l) => { let _c: Result[Int, String] = net.tcp_connect(\"127.0.0.1\", net.tcp_local_port(l) ?? 0)\n l }, err(_) => -1 })".to_string();
    match (sig.module, sig.func.as_str(), name) {
        ("net", "tcp_listen", "port") => Some("0".into()),
        ("net", _, "port") => Some(tcp_port.into()),
        ("net", _, "stream") => Some(stream),
        ("net", _, "listener") => Some(listener),
        _ => None,
    }
}

/// A String argument of a sandboxed effect function names something the
/// sandbox holds (`oracle/sandbox.rs`), or misses it: a path in the seeded
/// tree, an env var of the fixed env, an argv flag. `None`: generate as usual.
fn sandbox_arg(b: &mut Builder, sig: &Signature, param: &SigType, name: &str) -> Option<String> {
    const PATHS: &[&str] = &[
        "data.txt", "empty.txt", "bin.dat", "sub/nested.txt", "sub", "sub/deeper", "empty_dir",
        "missing.txt", "out.txt", "sub/new.txt", "made/inner", "*.txt", "sub/*",
    ];
    const ENV_NAMES: &[&str] = &["XF_GREETING", "XF_EMPTY", "XF_UNSET"];
    const ARG_NAMES: &[&str] = &["flag", "opt", "alpha", "beta", "missing"];
    // ADR-0027 D4: a fixed command set, present at these paths on linux and
    // macos, and no shell: a generated argument list is data, never code.
    const COMMANDS: &[&str] = &["/bin/echo", "/bin/cat", "/usr/bin/true", "/usr/bin/false", "/no/such/command"];
    // A spawned child shares the program's stdout and runs beside it: what it
    // prints lands wherever the scheduler puts it, so spawn draws commands
    // that print nothing.
    const SILENT: &[&str] = &["/usr/bin/true", "/usr/bin/false", "/no/such/command"];
    const DIRS: &[&str] = &["sub", "empty_dir", ".", "missing_dir"];
    if !sig.effect || *param != SigType::String {
        return None;
    }
    let pool = match (sig.module, name) {
        ("fs", _) => PATHS,
        ("env", _) => ENV_NAMES,
        ("args", _) => ARG_NAMES,
        ("process", "cmd") if sig.func == "spawn" => SILENT,
        // A child that exits before reading its input races the write
        // (EPIPE or not, by timing, on every leg): the input goes to one that
        // reads it all.
        ("process", "cmd") if sig.func == "exec_with_stdin" => &["/bin/cat", "/no/such/command"],
        ("process", "cmd") => COMMANDS,
        ("process", "dir") => DIRS,
        // ADR-0027 D5: the loopback fixtures (`oracle/fixture.rs`), never
        // another host.
        ("http", "url") => return Some(fixture_url(b.rng.pick(HTTP_PATHS))),
        ("http", "base_url") => return Some(fixture_url("/sse/2")),
        ("http", "method") => HTTP_METHODS,
        ("net", "host") => &["127.0.0.1"],
        _ => return None,
    };
    Some(render_string_literal(b.rng.pick(pool)))
}

/// Produce an expression of `goal` type, valid in bare expression
/// position (i.e. never an un-annotated ambiguous literal — those are
/// hoisted to a `let` and referenced). Returns Almide source text.
pub fn gen_expr(b: &mut Builder, goal: &GenType) -> String {
    b.fuel -= 1;
    if b.fuel <= 0 {
        return gen_leaf(b, goal);
    }

    let has_var = !b.vars_of_type(goal).is_empty();

    // Production menu as a weighted table — named weights keep the
    // steering legible (call-heavy, divergence-prone terms favoured).
    const W_LEAF: u32 = 3;
    const W_VAR: u32 = 5;
    const W_CALL: u32 = 8;
    const W_IF: u32 = 3;
    const W_CONSTRUCT: u32 = 5;

    #[derive(Clone, Copy)]
    enum Prod {
        Leaf,
        Var,
        Call,
        If,
        Construct,
    }

    let mut menu: Vec<(u32, Prod)> = vec![
        (W_LEAF, Prod::Leaf),
        (W_CALL, Prod::Call),
        (W_IF, Prod::If),
        (W_CONSTRUCT, Prod::Construct),
    ];
    if has_var {
        menu.push((W_VAR, Prod::Var));
    }

    let weights: Vec<u32> = menu.iter().map(|(w, _)| *w).collect();
    let choice = menu[b.rng.pick_weighted(&weights)].1;

    match choice {
        Prod::Leaf => gen_leaf(b, goal),
        Prod::Var => gen_var(b, goal).unwrap_or_else(|| gen_leaf(b, goal)),
        Prod::Call => gen_call(b, goal).unwrap_or_else(|| gen_construct(b, goal)),
        Prod::If => gen_if(b, goal),
        Prod::Construct => gen_construct(b, goal),
    }
}

/// A reference to an in-scope variable of the goal type, if any.
fn gen_var(b: &mut Builder, goal: &GenType) -> Option<String> {
    let candidates = b.vars_of_type(goal);
    if candidates.is_empty() {
        return None;
    }
    let idx = candidates[b.rng.below(candidates.len() as u32) as usize];
    Some(b.scope[idx].name.clone())
}

/// The cheapest terminal expression for a goal type: a literal, an
/// in-scope variable, or a hoisted construction. Always succeeds and is
/// always valid in bare position.
fn gen_leaf(b: &mut Builder, goal: &GenType) -> String {
    // Prefer an in-scope variable half the time (cheap data reuse).
    if b.rng.chance(1, 2) {
        if let Some(v) = gen_var(b, goal) {
            return v;
        }
    }
    match goal {
        GenType::Int => format!("{}", b.rng.pick(pools::INT_POOL)),
        GenType::Float => render_float_literal(*b.rng.pick(pools::FLOAT_POOL)),
        GenType::String => render_string_literal(b.rng.pick(pools::STRING_POOL)),
        GenType::Bool => format!("{}", b.rng.pick(pools::BOOL_POOL)),
        GenType::Unit => "()".to_string(),
        GenType::Sized(_) | GenType::Bytes => sized_or_bytes(b, goal),
        GenType::Matrix => matrix_literal(b),
        GenType::Value => value_literal(b, 0),
        GenType::Endian => b.rng.pick(&["LittleEndian", "BigEndian"]).to_string(),
        GenType::Nominal(k) => nominal_literal(b, k),
        // Compound goals: construct (and hoist if ambiguous).
        GenType::List(_)
        | GenType::Option(_)
        | GenType::Result(_)
        | GenType::ResultNom(..)
        | GenType::Tuple2(_, _)
        | GenType::Tuple3(..)
        | GenType::Map(_)
        | GenType::Set(_) => gen_construct(b, goal),
    }
}

/// A fixed-width number through its `int.to_<width>` conversion (from a
/// non-negative Int, so every width holds it), or `Bytes` from a short list.
fn sized_or_bytes(b: &mut Builder, goal: &GenType) -> String {
    const SMALL: &[i64] = &[0, 1, 2, 7, 100];
    match goal {
        GenType::Sized(name) => GenType::sized_literal(name, &b.rng.pick(SMALL).to_string()),
        _ => {
            let n = *b.rng.pick(pools::SMALL_COUNT_POOL) as usize;
            let items: Vec<String> = (0..n).map(|_| b.rng.pick(SMALL).to_string()).collect();
            format!("bytes.from_list([{}])", items.join(", "))
        }
    }
}

/// A small non-empty rectangular `Matrix` from Float literals.
/// A random leaf of a nominal type, from its declared constructors.
fn nominal_literal(b: &mut Builder, k: &str) -> String {
    let leaves = super::nominal::get(k).map_or(&[][..], |n| n.leaves);
    b.rng.pick(leaves).to_string()
}

fn matrix_literal(b: &mut Builder) -> String {
    let (r, c) = (1 + *b.rng.pick(&[0usize, 1, 2]), 1 + *b.rng.pick(&[0usize, 1, 2]));
    let rows: Vec<String> = (0..r)
        .map(|_| {
            let cells: Vec<String> = (0..c).map(|_| render_float_literal(*b.rng.pick(pools::FLOAT_POOL))).collect();
            format!("[{}]", cells.join(", "))
        })
        .collect();
    format!("matrix.from_lists([{}])", rows.join(", "))
}

/// A `Value`: a scalar, or (above depth 2, never) an array or object of up
/// to two children. Object keys mix snake_case and camelCase, so `pick` /
/// `omit` hit and miss and the case converters have something to rewrite.
fn value_literal(b: &mut Builder, depth: u32) -> String {
    const KEYS: &[&str] = &["a", "k", "snake_case", "camelCase", "ünï"];
    let kinds: &[u8] = if depth >= 2 { &[0, 1, 2, 3, 4] } else { &[0, 1, 2, 3, 4, 5, 6] };
    match *b.rng.pick(kinds) {
        0 => format!("value.int({})", b.rng.pick(pools::INT_POOL)),
        1 => format!("value.float({})", render_float_literal(*b.rng.pick(pools::FLOAT_POOL))),
        2 => format!("value.str({})", render_string_literal(b.rng.pick(pools::STRING_POOL))),
        3 => format!("value.bool({})", b.rng.pick(pools::BOOL_POOL)),
        4 => "value.null()".to_string(),
        5 => {
            let n = *b.rng.pick(&[0usize, 1, 2]);
            let items: Vec<String> = (0..n).map(|_| value_literal(b, depth + 1)).collect();
            if n == 0 { "value.array(list.drop([value.null()], 1))".to_string() } else { format!("value.array([{}])", items.join(", ")) }
        }
        _ => {
            let n = *b.rng.pick(&[0usize, 1, 2]);
            let pairs: Vec<String> = (0..n)
                .map(|_| {
                    let k = render_string_literal(b.rng.pick(KEYS));
                    format!("({k}, {})", value_literal(b, depth + 1))
                })
                .collect();
            if n == 0 { "value.object(list.drop([(\"a\", value.null())], 1))".to_string() } else { format!("value.object([{}])", pairs.join(", ")) }
        }
    }
}

/// Construct a compound value. Ambiguous shapes (empty list/map, `none`,
/// `ok`/`err`) are hoisted into an annotated `let` so they type-check;
/// the function returns the *reference* to that binding. Unambiguous
/// shapes (non-empty list, tuple, `some(x)`) are returned inline.
fn gen_construct(b: &mut Builder, goal: &GenType) -> String {
    match goal {
        GenType::Int
        | GenType::Float
        | GenType::String
        | GenType::Bool
        | GenType::Unit
        | GenType::Sized(_)
        | GenType::Bytes
        | GenType::Matrix
        | GenType::Value => gen_leaf(b, goal),
        | GenType::Endian
        | GenType::Nominal(_) => gen_leaf(b, goal),
        GenType::Set(elem) => {
            let n = *b.rng.pick(pools::SMALL_COUNT_POOL) as usize;
            if n == 0 {
                return b.hoist(goal, "set.new()".to_string());
            }
            let items: Vec<String> = (0..n).map(|_| gen_expr(b, elem)).collect();
            format!("set.from_list([{}])", items.join(", "))
        }
        GenType::List(elem) => {
            let n = *b.rng.pick(pools::SMALL_COUNT_POOL) as usize;
            if n == 0 {
                // Empty list is ambiguous ⇒ hoist with annotation.
                return b.hoist(goal, "[]".to_string());
            }
            let items: Vec<String> = (0..n).map(|_| gen_expr(b, elem)).collect();
            format!("[{}]", items.join(", "))
        }
        GenType::Option(inner) => {
            if b.rng.chance(2, 3) {
                // `some(x)` is unambiguous when x's type is known.
                format!("some({})", gen_expr(b, inner))
            } else {
                b.hoist(goal, "none".to_string())
            }
        }
        GenType::Result(ok) => {
            // `ok(x)`/`err(s)` need the *other* arm's type ⇒ always hoist.
            if b.rng.chance(2, 3) {
                let v = gen_expr(b, ok);
                b.hoist(goal, format!("ok({v})"))
            } else {
                let s = render_string_literal(b.rng.pick(pools::STRING_POOL));
                b.hoist(goal, format!("err({s})"))
            }
        }
        // The error arm is a value of the nominal, from its own leaves.
        GenType::ResultNom(ok, k) => {
            if b.rng.chance(2, 3) {
                let v = gen_expr(b, ok);
                b.hoist(goal, format!("ok({v})"))
            } else {
                let e = nominal_literal(b, k);
                b.hoist(goal, format!("err({e})"))
            }
        }
        GenType::Tuple2(a, b2) => {
            // A tuple literal is unambiguous from its element exprs.
            format!("({}, {})", gen_expr(b, a), gen_expr(b, b2))
        }
        GenType::Tuple3(a, b2, c) => format!("({}, {}, {})", gen_expr(b, a), gen_expr(b, b2), gen_expr(b, c)),
        GenType::Map(v) => {
            let n = *b.rng.pick(pools::SMALL_COUNT_POOL) as usize;
            if n == 0 {
                return b.hoist(goal, "[:]".to_string());
            }
            let mut entries = Vec::with_capacity(n);
            for i in 0..n {
                let key = format!("\"k{i}\"");
                entries.push(format!("{}: {}", key, gen_expr(b, v)));
            }
            format!("[{}]", entries.join(", "))
        }
    }
}

/// `if cond then A else B`, both arms of the goal type.
fn gen_if(b: &mut Builder, goal: &GenType) -> String {
    let cond = gen_expr(b, &GenType::Bool);
    let then = gen_expr(b, goal);
    let els = gen_expr(b, goal);
    format!("(if {cond} then {then} else {els})")
}

/// A stdlib call whose return type unifies with `goal`. Returns `None`
/// if no catalogued signature fits — the caller falls back to
/// construction.
fn gen_call(b: &mut Builder, goal: &GenType) -> Option<String> {
    // Candidate signatures whose return type unifies with the goal,
    // plus the substitution that makes it so.
    //
    // Pinned-slot pre-seeding: a type variable that sits in a `Map` *key*
    // or `Result` *error* slot is fixed to `String` by the value grammar
    // (`instantiate` lowers both to String). Such a variable can also
    // appear in a non-pinned return position — e.g. `result.to_err_option`
    // returns `Option[E]` and `map.keys` returns `List[K]`, where `E`/`K`
    // are the very slots pinned to String. If unification were allowed to
    // bind them freely from the goal (E = Float for an `Option[Float]`
    // goal), the generator would build a `Result[_, String]` value yet
    // call a function it believes returns `Option[Float]` — an ill-typed
    // emission the checker rejects (the dominant rung-a reject class).
    //
    // Seeding the forced `String` bindings *before* unification makes the
    // `Var` case in `unify_with_goal` require those slots to equal String:
    // a candidate whose pinned variable the goal demands be non-String is
    // correctly pruned, instead of producing a mismatch downstream.
    let mut candidates: Vec<(usize, HashMap<String, GenType>, u32)> = Vec::new();
    for (i, sig) in b.catalogue.iter().enumerate() {
        let mut subst = sig.forced_bindings();
        if sig.effective_ret().unify_with_goal(goal, &mut subst) {
            candidates.push((i, subst, sig.weight));
        }
    }
    if candidates.is_empty() {
        return None;
    }

    let weights: Vec<u32> = candidates.iter().map(|(_, _, w)| *w).collect();
    let pick = b.rng.pick_weighted(&weights);
    let (sig_idx, subst, _) = candidates[pick].clone();
    let sig = b.catalogue[sig_idx].clone();

    let mut subst = subst;
    bind_free_vars(b, &sig, &mut subst)?;
    render_call(b, &sig, &subst)
}

/// A direct call of catalogue entry `idx` at generated arguments, with the
/// concrete type it returns — the `--focus` binding every focused program
/// starts with. `None` when the instantiated return type cannot be printed.
pub fn gen_focus_call(b: &mut Builder, idx: usize) -> Option<(String, GenType)> {
    let sig = b.catalogue[idx].clone();
    let mut subst = sig.forced_bindings();
    bind_free_vars(b, &sig, &mut subst)?;
    let ret = sig.effective_ret().instantiate(&subst)?;
    if ret.observe("v", 0).is_none() {
        return None;
    }
    Some((render_call(b, &sig, &subst)?, ret))
}

/// The call of a function that never returns (catalogue entry `idx`, a
/// [`super::catalogue::NEVER_RETURNING`] row), at generated arguments
/// brought into its domain and bound first: the statement a focused program
/// ends with, after its prints.
pub fn gen_final_call(b: &mut Builder, idx: usize) -> Option<String> {
    let sig = b.catalogue[idx].clone();
    let domain = super::catalogue::never_domain(sig.module, &sig.func)?;
    let mut subst = sig.forced_bindings();
    bind_free_vars(b, &sig, &mut subst)?;
    let mut args = Vec::with_capacity(sig.params.len());
    for (name, p) in sig.param_names.iter().zip(&sig.params) {
        let a = gen_arg(b, p, name, &subst)?;
        let a = match domain.iter().find(|(d, _)| d == name) {
            Some((_, tmpl)) => super::verdicts::fill(tmpl, &[(name, &a)]),
            None => a,
        };
        let v = b.fresh_name();
        b.stmts.push(format!("let {v}: {} = {a}", p.instantiate(&subst)?.render()));
        args.push(v);
    }
    if sig.effect {
        b.effect = true;
    }
    Some(format!("{}.{}({})", sig.module, sig.func, args.join(", ")))
}

/// Parameter names that denote a count/size/index and so must receive a
/// SMALL value, never a boundary `INT_POOL` extreme: a `repeat`/`pad`
/// count of `u32::MAX` (or a negative one) would exhaust memory or panic
/// on the allocation, manufacturing a noise "hang"/"capacity overflow"
/// that buries real divergences. These are the count-like names the
/// stdlib actually uses.
const COUNT_LIKE_PARAM_NAMES: &[&str] = &[
    "n", "count", "len", "times", "size", "decimals", "width", "k", "i", "j", "index",
    "start", "end", "lo", "hi",
];

/// Concrete types a free (parameter-only) type variable may take. Kept
/// to printable scalars so any HOF over them stays observable and cheap.
const FREE_VAR_CHOICES: &[GenType] = &[GenType::Int, GenType::String, GenType::Bool];

/// Bind every type variable in `sig` not already bound by return-type
/// unification to a concrete scalar type.
fn bind_free_vars(
    b: &mut Builder,
    sig: &Signature,
    subst: &mut HashMap<String, GenType>,
) -> Option<()> {
    // First, force-bind variables pinned by Map-key / Result-error slots
    // to String (the type `instantiate` lowers those slots to), so the
    // closure parameter types the generator emits match the values it
    // actually constructs. `gen_call` already seeds these before goal
    // unification, but this path is also reached when no unification ran;
    // `or_insert` keeps it idempotent and never overwrites a binding the
    // (String-consistent) goal unification already chose.
    for (k, v) in sig.forced_bindings() {
        subst.entry(k).or_insert(v);
    }

    // Then choose any still-unbound variable freely.
    let mut vars = Vec::new();
    for p in &sig.params {
        p.collect_vars(&mut vars);
    }
    sig.ret.collect_vars(&mut vars);
    for v in vars {
        if !subst.contains_key(&v) {
            let chosen =
                FREE_VAR_CHOICES[b.rng.below(FREE_VAR_CHOICES.len() as u32) as usize].clone();
            subst.insert(v, chosen);
        }
    }
    Some(())
}

/// Generate an argument expression for a (possibly generic) parameter,
/// given its name and the binding substitution. HOF parameters become
/// inline lambdas; count-like `Int` parameters get a small, safe literal.
fn gen_arg(
    b: &mut Builder,
    param: &SigType,
    param_name: &str,
    subst: &HashMap<String, GenType>,
) -> Option<String> {
    match param {
        SigType::Fn(params, ret) => Some(gen_lambda(b, params, ret, subst)),
        // Count/size/index Int parameter ⇒ a small non-pathological value
        // (avoids the `u32::MAX`/negative allocation-bomb noise).
        SigType::Int if COUNT_LIKE_PARAM_NAMES.contains(&param_name) => {
            Some(format!("{}", b.rng.pick(pools::SMALL_COUNT_POOL)))
        }
        other => {
            let concrete = other.instantiate(subst)?;
            Some(gen_expr(b, &concrete))
        }
    }
}

/// Generate an inline lambda `(p0, p1) => body` for a HOF argument.
///
/// This is the `fan.map`-class divergence surface: the body frequently
/// wraps a parameter in `some`/`ok`/string interpolation, exactly the
/// shapes where closure boxing has historically broken.
///
/// Lambda bodies must stay *single-expression* (no hoisting into the
/// enclosing statement list mid-argument), so we generate the body with
/// a private sub-builder whose hoisted statements are discarded — the
/// body falls back to inline-only constructions. To keep that sound we
/// give the body fuel that only produces inline-valid shapes.
fn gen_lambda(
    b: &mut Builder,
    params: &[SigType],
    ret: &SigType,
    subst: &HashMap<String, GenType>,
) -> String {
    // Bind fresh lambda parameter names in scope.
    let mut names = Vec::with_capacity(params.len());
    let mut pushed = 0usize;
    for p in params {
        let cty = p.instantiate(subst).unwrap_or(GenType::Int);
        let name = b.fresh_name();
        b.scope.push(ScopeVar {
            name: name.clone(),
            ty: cty,
        });
        pushed += 1;
        names.push(name);
    }

    let ret_concrete = ret.instantiate(subst).unwrap_or(GenType::Int);
    let body = gen_lambda_body(b, &ret_concrete);

    for _ in 0..pushed {
        b.scope.pop();
    }

    format!("(({}) => {})", names.join(", "), body)
}

/// Generate a lambda body — a single inline expression. Because a lambda
/// body cannot host `let` statements, this path never hoists: it draws
/// from variables in scope (incl. the lambda params) or inline-safe
/// constructions, recursing only through unambiguous shapes.
fn gen_lambda_body(b: &mut Builder, goal: &GenType) -> String {
    // Prefer an in-scope value (a lambda param or outer binding).
    if let Some(v) = gen_var(b, goal) {
        // Sometimes wrap a scalar in a small transformation to exercise
        // the closure body, otherwise return it directly.
        if b.rng.chance(1, 2) {
            return v;
        }
    }
    // Inline-safe construction for the goal type. For `Result`/`Option`
    // we use `some`/`ok` *only* when the inner type is known and inline,
    // since those constructors are valid inline when their payload fixes
    // the type. `none`/`err`/empty collections are NOT inline-safe, so
    // for those goal shapes we fall back to a scope var or a default.
    match goal {
        GenType::Int => format!("{}", b.rng.pick(pools::INT_POOL)),
        GenType::Float => render_float_literal(*b.rng.pick(pools::FLOAT_POOL)),
        GenType::String => render_string_literal(b.rng.pick(pools::STRING_POOL)),
        GenType::Bool => format!("{}", b.rng.pick(pools::BOOL_POOL)),
        GenType::Unit => "()".to_string(),
        GenType::Option(inner) => {
            // `some(x)` is inline-safe; build x inline.
            format!("some({})", gen_lambda_body(b, inner))
        }
        GenType::Result(ok) | GenType::ResultNom(ok, _) => {
            // `ok(x)` alone cannot fix E inline, so only safe to use when
            // the HOF pins E (e.g. result.map's closure returns B, and
            // the surrounding Result[B, E] context fixes E). It does, so
            // `ok(x)` here is fine.
            format!("ok({})", gen_lambda_body(b, ok))
        }
        GenType::List(elem) => {
            // Non-empty list is inline-safe; size 1 keeps it tiny.
            format!("[{}]", gen_lambda_body(b, elem))
        }
        GenType::Tuple2(a, c) => {
            format!("({}, {})", gen_lambda_body(b, a), gen_lambda_body(b, c))
        }
        GenType::Tuple3(a, c, d) => format!("({}, {}, {})", gen_lambda_body(b, a), gen_lambda_body(b, c), gen_lambda_body(b, d)),
        GenType::Map(v) => {
            // Single-entry map literal is inline-safe.
            format!("[\"k\": {}]", gen_lambda_body(b, v))
        }
        GenType::Sized(_) | GenType::Bytes => sized_or_bytes(b, goal),
        GenType::Matrix => matrix_literal(b),
        GenType::Value => value_literal(b, 0),
        GenType::Endian => b.rng.pick(&["LittleEndian", "BigEndian"]).to_string(),
        GenType::Nominal(k) => nominal_literal(b, k),
        GenType::Set(e) => format!("set.from_list([{}])", gen_lambda_body(b, e)),
    }
}

/// Render a float literal the Almide lexer accepts (it requires a
/// decimal point; `-0.0` and scientific forms pass through).
fn render_float_literal(f: f64) -> String {
    if f == 0.0 && f.is_sign_negative() {
        return "-0.0".to_string();
    }
    if f.fract() == 0.0 && f.is_finite() && f.abs() < 1e15 {
        return format!("{f:.1}");
    }
    let s = format!("{f}");
    if s.contains('.') || s.contains('e') || s.contains('E') {
        s
    } else {
        format!("{s}.0")
    }
}

/// Render a string literal with Almide-compatible escapes. Backslash,
/// double-quote, common control chars, and `$` (interpolation lead-in)
/// are escaped; all other bytes — including multibyte UTF-8 — pass
/// through literally, which is what we want for the Unicode pool.
fn render_string_literal(s: impl AsRef<str>) -> String {
    let s = s.as_ref();
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '$' => out.push_str("\\$"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}
