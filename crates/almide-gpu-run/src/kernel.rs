//! IR → WGSL for a pure `(Int) -> Int` function, applied element-wise.
//!
//! The subset is exactly what the device reproduces bit-for-bit:
//!   - types: `Int` (carried as `vec2<u32>`, low word first) and `Bool`;
//!   - `Int` literals, `+ - *` (wrapping), unary `-`, the six comparisons;
//!     `and` / `or` / `not` on `Bool`;
//!   - `if`/`else`, blocks with immutable `let`;
//!   - calls to other user functions in the subset, never recursive (WGSL
//!     forbids recursion).
//!
//! Everything else is a [`Refusal`] that names the construct. `/` and `%` are
//! refused on purpose: on the CPU legs a zero divisor and `MIN / -1` abort,
//! and the device has no abort yet — an emitter that let them through would
//! answer where the other legs stop.

use almide_ir::{BinOp, CallTarget, IrExpr, IrExprKind, IrFunction, IrProgram, IrStmtKind, Mutability, UnOp, VarId};
use almide_lang::types::Ty;
use std::collections::BTreeSet;

/// The kernel's entry point name, for the pipeline descriptor.
pub const ENTRY_POINT: &str = "almide_main";
/// Invocations per workgroup; the runner dispatches `ceil(n / WORKGROUP_SIZE)`.
pub const WORKGROUP_SIZE: u32 = 64;

/// Why a function cannot run on the GPU leg. The message names the construct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal(pub String);

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn refuse<T>(msg: impl Into<String>) -> Result<T, Refusal> {
    Err(Refusal(msg.into()))
}

/// 64-bit two's-complement arithmetic over `vec2<u32>` (x = low, y = high).
/// `+ - *` keep the low 64 bits of the true result, which is the wrapping
/// semantics of Almide's `Int` (native `wrapping_*`, wasm `i64.*`).
const PRELUDE: &str = r#"fn i64_add(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
  let lo = a.x + b.x;
  return vec2<u32>(lo, a.y + b.y + select(0u, 1u, lo < a.x));
}
fn i64_sub(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
  return vec2<u32>(a.x - b.x, a.y - b.y - select(0u, 1u, a.x < b.x));
}
fn mul32_wide(u: u32, v: u32) -> vec2<u32> {
  let u0 = u & 0xffffu;
  let u1 = u >> 16u;
  let v0 = v & 0xffffu;
  let v1 = v >> 16u;
  let p00 = u0 * v0;
  let p01 = u0 * v1;
  let p10 = u1 * v0;
  let p11 = u1 * v1;
  let mid = (p00 >> 16u) + (p01 & 0xffffu) + (p10 & 0xffffu);
  return vec2<u32>((p00 & 0xffffu) | (mid << 16u), p11 + (p01 >> 16u) + (p10 >> 16u) + (mid >> 16u));
}
fn i64_mul(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
  let p = mul32_wide(a.x, b.x);
  return vec2<u32>(p.x, p.y + a.x * b.y + a.y * b.x);
}
fn i64_neg(a: vec2<u32>) -> vec2<u32> {
  return i64_sub(vec2<u32>(0u, 0u), a);
}
fn i64_eq(a: vec2<u32>, b: vec2<u32>) -> bool {
  return a.x == b.x && a.y == b.y;
}
fn i64_lt(a: vec2<u32>, b: vec2<u32>) -> bool {
  if (a.y != b.y) {
    return bitcast<i32>(a.y) < bitcast<i32>(b.y);
  }
  return a.x < b.x;
}
"#;

/// Emit the WGSL kernel that applies `entry` to every element of a list of
/// `Int`. `entry` must be a pure `(Int) -> Int` function of `program`.
pub fn int_map_kernel(program: &IrProgram, entry: &str) -> Result<String, Refusal> {
    let root = find_fn(program, entry)?;
    if root.params.len() != 1 || root.params[0].ty != Ty::Int || root.ret_ty != Ty::Int {
        return refuse(format!("`{entry}` is not `(Int) -> Int`"));
    }
    let mut emitted = Vec::new();
    let mut done = BTreeSet::new();
    emit_fn_tree(program, root, &mut Vec::new(), &mut done, &mut emitted)?;
    let mut out = String::from(PRELUDE);
    for f in emitted {
        out.push_str(&f);
    }
    out.push_str(&format!(
        "@group(0) @binding(0) var<storage, read> xs: array<vec2<u32>>;\n\
         @group(0) @binding(1) var<storage, read_write> ys: array<vec2<u32>>;\n\
         @compute @workgroup_size({WORKGROUP_SIZE})\n\
         fn {ENTRY_POINT}(@builtin(global_invocation_id) gid: vec3<u32>) {{\n\
         \x20 let i = gid.x;\n\
         \x20 if (i >= arrayLength(&xs)) {{\n\
         \x20   return;\n\
         \x20 }}\n\
         \x20 ys[i] = {}(xs[i]);\n\
         }}\n",
        fn_ident(entry)
    ));
    Ok(out)
}

fn find_fn<'a>(program: &'a IrProgram, name: &str) -> Result<&'a IrFunction, Refusal> {
    match program.functions.iter().find(|f| f.name.as_str() == name) {
        Some(f) => Ok(f),
        None => refuse(format!("`{name}` is not a function of this program")),
    }
}

fn fn_ident(name: &str) -> String {
    format!("u_{name}")
}

fn var_ident(id: VarId) -> String {
    format!("v{}", id.0)
}

/// Emit `func` after every callee it reaches (WGSL needs no forward
/// declarations, but a stable callee-first order keeps the text reviewable).
/// `stack` is the current call path: meeting a function already on it is
/// recursion, which WGSL forbids.
fn emit_fn_tree(
    program: &IrProgram,
    func: &IrFunction,
    stack: &mut Vec<String>,
    done: &mut BTreeSet<String>,
    out: &mut Vec<String>,
) -> Result<(), Refusal> {
    let name = func.name.as_str().to_string();
    if stack.contains(&name) {
        return refuse(format!("`{name}` is recursive; WGSL has no recursion"));
    }
    if done.contains(&name) {
        return Ok(());
    }
    if func.is_effect {
        return refuse(format!("`{name}` is an effect fn"));
    }
    if func.generics.as_ref().is_some_and(|g| !g.is_empty()) {
        return refuse(format!("`{name}` is generic"));
    }
    stack.push(name.clone());
    let mut body = FnGen { program, lines: Vec::new(), temps: 0, scope: BTreeSet::new(), callees: Vec::new() };
    let mut params = Vec::new();
    for p in &func.params {
        let ty = wgsl_type(&p.ty).ok_or_else(|| Refusal(format!("`{name}`: parameter `{}` has type {:?}", p.name.as_str(), p.ty)))?;
        body.scope.insert(p.var);
        params.push(format!("{}: {ty}", var_ident(p.var)));
    }
    let ret = wgsl_type(&func.ret_ty).ok_or_else(|| Refusal(format!("`{name}`: return type {:?}", func.ret_ty)))?;
    let value = body.expr(&func.body)?;
    let FnGen { lines, callees, .. } = body;
    for callee in callees {
        let callee_fn = find_fn(program, &callee)?;
        emit_fn_tree(program, callee_fn, stack, done, out)?;
    }
    stack.pop();
    let mut text = format!("fn {}({}) -> {ret} {{\n", fn_ident(&name), params.join(", "));
    for l in lines {
        text.push_str(&l);
        text.push('\n');
    }
    text.push_str(&format!("  return {value};\n}}\n"));
    done.insert(name);
    out.push(text);
    Ok(())
}

fn wgsl_type(ty: &Ty) -> Option<&'static str> {
    match ty {
        Ty::Int => Some("vec2<u32>"),
        Ty::Bool => Some("bool"),
        _ => None,
    }
}

/// One function body being emitted. Statements accumulate in `lines`; every
/// expression method returns the WGSL expression for its value.
struct FnGen<'a> {
    program: &'a IrProgram,
    lines: Vec<String>,
    temps: u32,
    /// The `VarId`s bound so far (params and `let`s). A `Var` outside it is a
    /// capture or a global, which the kernel cannot see.
    scope: BTreeSet<VarId>,
    /// User functions this body calls, in first-call order.
    callees: Vec<String>,
}

impl FnGen<'_> {
    fn push(&mut self, line: String) {
        self.lines.push(format!("  {line}"));
    }

    fn temp(&mut self) -> String {
        self.temps += 1;
        format!("t{}", self.temps)
    }

    fn expr(&mut self, e: &IrExpr) -> Result<String, Refusal> {
        if wgsl_type(&e.ty).is_none() {
            return refuse(format!("a value of type {:?}", e.ty));
        }
        match &e.kind {
            IrExprKind::LitInt { value } => {
                let bits = *value as u64;
                Ok(format!("vec2<u32>({}u, {}u)", bits as u32, (bits >> 32) as u32))
            }
            IrExprKind::LitBool { value } => Ok(value.to_string()),
            IrExprKind::Var { id } => {
                if self.scope.contains(id) {
                    Ok(var_ident(*id))
                } else {
                    refuse("a variable bound outside the function (a capture or a global)")
                }
            }
            IrExprKind::BinOp { op, left, right } => self.binop(*op, left, right),
            IrExprKind::UnOp { op, operand } => {
                let v = self.expr(operand)?;
                match op {
                    UnOp::NegInt => Ok(format!("i64_neg({v})")),
                    UnOp::Not => Ok(format!("(!{v})")),
                    UnOp::NegFloat => refuse("Float negation"),
                }
            }
            IrExprKind::If { cond, then, else_ } => self.if_expr(&e.ty, cond, then, else_),
            IrExprKind::Block { stmts, expr } => {
                for s in stmts {
                    match &s.kind {
                        IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value } => {
                            if wgsl_type(ty).is_none() {
                                return refuse(format!("a `let` of type {ty:?}"));
                            }
                            let v = self.expr(value)?;
                            self.push(format!("let {} = {v};", var_ident(*var)));
                            self.scope.insert(*var);
                        }
                        IrStmtKind::Bind { mutability: Mutability::Var, .. } => return refuse("a `var` binding"),
                        _ => return refuse("a statement other than an immutable `let`"),
                    }
                }
                match expr {
                    Some(tail) => self.expr(tail),
                    None => refuse("a block with no value"),
                }
            }
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                let callee = name.as_str();
                if !self.program.functions.iter().any(|f| f.name.as_str() == callee) {
                    return refuse(format!("a call to `{callee}`, which is not a user function of this program"));
                }
                let mut vals = Vec::new();
                for a in args {
                    vals.push(self.expr(a)?);
                }
                if !self.callees.iter().any(|c| c == callee) {
                    self.callees.push(callee.to_string());
                }
                Ok(format!("{}({})", fn_ident(callee), vals.join(", ")))
            }
            IrExprKind::Call { .. } => refuse("a call that is not to a named user function"),
            IrExprKind::TailCall { .. } => refuse("a tail call (recursion)"),
            other => refuse(format!("the expression form {}", kind_name(other))),
        }
    }

    fn binop(&mut self, op: BinOp, left: &IrExpr, right: &IrExpr) -> Result<String, Refusal> {
        let l = self.expr(left)?;
        let r = self.expr(right)?;
        let int = left.ty == Ty::Int;
        match op {
            BinOp::AddInt => Ok(format!("i64_add({l}, {r})")),
            BinOp::SubInt => Ok(format!("i64_sub({l}, {r})")),
            BinOp::MulInt => Ok(format!("i64_mul({l}, {r})")),
            BinOp::Eq if int => Ok(format!("i64_eq({l}, {r})")),
            BinOp::Neq if int => Ok(format!("(!i64_eq({l}, {r}))")),
            BinOp::Lt if int => Ok(format!("i64_lt({l}, {r})")),
            BinOp::Gt if int => Ok(format!("i64_lt({r}, {l})")),
            BinOp::Lte if int => Ok(format!("(!i64_lt({r}, {l}))")),
            BinOp::Gte if int => Ok(format!("(!i64_lt({l}, {r}))")),
            BinOp::Eq => Ok(format!("({l} == {r})")),
            BinOp::Neq => Ok(format!("({l} != {r})")),
            BinOp::And => Ok(format!("({l} && {r})")),
            BinOp::Or => Ok(format!("({l} || {r})")),
            BinOp::DivInt | BinOp::ModInt => {
                refuse("`/` or `%` on Int: a zero divisor or MIN / -1 aborts on the CPU legs and the device cannot abort yet")
            }
            other => refuse(format!("the operator {other:?}")),
        }
    }

    /// `if` evaluates only the taken branch: each branch's statements land in
    /// its own WGSL block and assign one shared result variable.
    fn if_expr(&mut self, ty: &Ty, cond: &IrExpr, then: &IrExpr, else_: &IrExpr) -> Result<String, Refusal> {
        let c = self.expr(cond)?;
        let t = self.temp();
        let wty = wgsl_type(ty).ok_or_else(|| Refusal(format!("an `if` of type {ty:?}")))?;
        self.push(format!("var {t}: {wty};"));
        let outer = std::mem::take(&mut self.lines);
        let then_v = self.expr(then)?;
        let then_lines = std::mem::take(&mut self.lines);
        let else_v = self.expr(else_)?;
        let else_lines = std::mem::replace(&mut self.lines, outer);
        self.push(format!("if ({c}) {{"));
        for l in then_lines {
            self.lines.push(format!("  {l}"));
        }
        self.push(format!("  {t} = {then_v};"));
        self.push("} else {".to_string());
        for l in else_lines {
            self.lines.push(format!("  {l}"));
        }
        self.push(format!("  {t} = {else_v};"));
        self.push("}".to_string());
        Ok(t)
    }
}

/// A stable name for an IR expression form, for refusal messages.
fn kind_name(kind: &IrExprKind) -> String {
    let dbg = format!("{kind:?}");
    dbg.split(|c: char| !c.is_alphanumeric()).next().unwrap_or("?").to_string()
}
