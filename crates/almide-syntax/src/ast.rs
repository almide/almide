use serde::{Deserialize, Serialize};
use crate::intern::Sym;

// Almide AST types — mirrors src/ast.ts

/// Unique expression identifier. Eliminates span-collision bugs in type lookups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct ExprId(pub u32);

/// Generator for fresh ExprIds. Used by parser and sub-parsers.
#[derive(Debug, Clone)]
pub struct ExprIdGen {
    next: u32,
}
impl ExprIdGen {
    pub fn new() -> Self { ExprIdGen { next: 0 } }
    pub fn from(start: u32) -> Self { ExprIdGen { next: start } }
    pub fn next(&mut self) -> ExprId { let id = ExprId(self.next); self.next += 1; id }
    pub fn current(&self) -> u32 { self.next }
}

pub use almide_base::span::Span;

/// Simplified type tag resolved by the checker.
/// Emitters use this for correct codegen (e.g. Float vs Int arithmetic).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedType {
    Int,
    Float,
    String,
    Bool,
    Unit,
    Bytes,
    Matrix,
    List,
    Map,
    Set,
    Option,
    Result,
    Fn,
    Record,
    Tuple,
    Variant,
    Named,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypeExpr {
    Simple { name: Sym },
    Generic { name: Sym, args: Vec<TypeExpr> },
    Record { fields: Vec<FieldType> },
    OpenRecord { fields: Vec<FieldType> },
    Fn { params: Vec<TypeExpr>, ret: Box<TypeExpr>, is_effect: bool },
    Tuple { elements: Vec<TypeExpr> },
    Variant { cases: Vec<VariantCase> },
    Union { members: Vec<TypeExpr> },
    /// Compile-time literal value in type argument position (e.g., `Array[Float, 128]`).
    ConstLit { value: i64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VariantCase {
    Unit { name: Sym },
    Tuple { name: Sym, fields: Vec<TypeExpr> },
    Record { name: Sym, fields: Vec<FieldType> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldType {
    pub name: Sym,
    #[serde(rename = "type")]
    pub ty: TypeExpr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Expr>,
    /// Serialization alias: `name as "external_key": Type`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<Sym>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<Attribute>,
    /// Comments written above this field inside the record body. Carried so
    /// the formatter can put them back: a field's comment is usually the only
    /// record of its unit or invariant, and dropping it is unrecoverable.
    #[serde(skip)]
    pub comments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolMethod {
    pub name: Sym,
    pub params: Vec<Param>,
    pub return_type: TypeExpr,
    #[serde(default)]
    pub effect: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericParam {
    pub name: Sym,
    pub bounds: Option<Vec<Sym>>,
    /// Structural type constraint (e.g., `T: { name: String, .. }`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structural_bound: Option<TypeExpr>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Pattern {
    Wildcard,
    Ident { name: Sym },
    Literal { value: Box<Expr> },
    Constructor { name: Sym, args: Vec<Pattern> },
    RecordPattern { name: Sym, fields: Vec<FieldPattern>, rest: bool },
    Tuple { elements: Vec<Pattern> },
    Some { inner: Box<Pattern> },
    None,
    Ok { inner: Box<Pattern> },
    Err { inner: Box<Pattern> },
    /// `rest` (#1461 list-rest): `[a, b, ..t]` binds the tail past the
    /// prefix, `[a, ..]` ignores it — either way the pattern matches any
    /// list of length >= elements.len(). `None` = the exact-length form.
    List {
        elements: Vec<Pattern>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rest: Option<Option<Sym>>,
    },
    /// As-pattern (#1461): `name @ pattern` — binds the WHOLE value at
    /// this position while the inner pattern destructures/tests it.
    As { name: Sym, inner: Box<Pattern> },
    /// Or-pattern (#1461): `a | b | c => body` — the arm matches when ANY
    /// alternative matches. Alternatives are binder-free (checker rule);
    /// lowering desugars the arm into one IR arm per alternative.
    Or { alts: Vec<Pattern> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldPattern {
    pub name: Sym,
    pub pattern: Option<Pattern>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StringPart {
    Lit { value: String },
    Expr { expr: Box<Expr> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expr {
    #[serde(skip)]
    pub id: ExprId,
    #[serde(skip)]
    pub span: Option<Span>,
    #[serde(flatten)]
    pub kind: ExprKind,
}

impl Expr {
    pub fn new(id: ExprId, span: Option<Span>, kind: ExprKind) -> Self {
        Expr { id, span, kind }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExprKind {
    Int { value: serde_json::Value, raw: String },
    /// `raw` is the literal's VERBATIM source spelling (`1e10`, `1_000.25`,
    /// `1e999`) when the node came from a parse. `fmt` prints it instead of
    /// reprinting `value`, which normalized `1e10` to `10000000000.0` and
    /// `1e999` to the UNPARSEABLE `inf.0` (#1261). `None` on synthesized
    /// nodes (and after `strip_literal_raw`), where `fmt` falls back to
    /// rendering the value. NOT serialized: two ASTs are the same program
    /// when their VALUES match, which is exactly what fmt's post-format
    /// verifier compares.
    Float { value: f64, #[serde(skip)] raw: Option<String> },
    /// `raw` is the whole literal INCLUDING its delimiters, so the quote
    /// style (`'…'` vs `"…"`), the heredoc form (`"""…"""`), the raw-string
    /// form (`r"…"`) and every escape spelling (`\u{3042}`, `\x41`) survive
    /// `fmt` (#1263). Same `None`/serde contract as `Float::raw`.
    String { value: String, #[serde(skip)] raw: Option<String> },
    InterpolatedString { parts: Vec<StringPart>, #[serde(skip)] raw: Option<String> },
    Bool { value: bool },
    Ident { name: Sym },
    TypeName { name: Sym },
    List { elements: Vec<Expr> },
    MapLiteral { entries: Vec<(Expr, Expr)> },
    EmptyMap,
    Record { name: Option<Sym>, fields: Vec<FieldInit> },
    SpreadRecord { base: Box<Expr>, fields: Vec<FieldInit> },
    Call { callee: Box<Expr>, args: Vec<Expr>, #[serde(default, skip_serializing_if = "Vec::is_empty")] named_args: Vec<(Sym, Expr)>, #[serde(default)] type_args: Option<Vec<TypeExpr>> },
    Member { object: Box<Expr>, field: Sym },
    TupleIndex { object: Box<Expr>, index: usize },
    IndexAccess { object: Box<Expr>, index: Box<Expr> },
    Pipe { left: Box<Expr>, right: Box<Expr> },
    Compose { left: Box<Expr>, right: Box<Expr> },
    If { cond: Box<Expr>, then: Box<Expr>, else_: Box<Expr> },
    /// `if let name = scrutinee { then } else { else_ }` — Swift-style implicit unwrap:
    /// `name` binds the value INSIDE the scrutinee's Option/Result (the frontend desugars
    /// to `match scrutinee { Some(name)|Ok(name) => then, _ => else_ }` once the scrutinee
    /// type is known). Kept as a distinct node so the formatter preserves the surface form.
    IfLet { name: Sym, scrutinee: Box<Expr>, then: Box<Expr>, else_: Box<Expr> },
    Match { subject: Box<Expr>, arms: Vec<MatchArm> },
    Block { stmts: Vec<Stmt>, expr: Option<Box<Expr>> },
    Fan { exprs: Vec<Expr> },
    /// `fan.bounded(budget) { body }` — deterministic computation budget
    /// (Stage 2 v1: body is a single call expression; budget is a `Compute`).
    FanBounded { budget: Box<Expr>, body: Box<Expr> },
    /// `fan.race(budget?) { arm; arm; … }` — deterministic race: the winner is
    /// the (spend, index)-lexicographic minimum completion. The optional budget
    /// is a per-branch divergence guard (Stage 3 v1: arms are single calls).
    FanRace { budget: Option<Box<Expr>>, arms: Vec<Expr> },
    /// `fan.race(xs, f)` / `fan.race(budget, xs, f)` — the MAPPER form: one
    /// pure 1-param lambda raced over a dynamic list, winner = the
    /// (spend, index) lexicographic minimum among successes (the mapper
    /// returns Result — Err self-disqualifies, matching the block form's
    /// Result-arm rule). The budget is per-element (per-branch semantics).
    FanRaceMap { budget: Option<Box<Expr>>, list: Box<Expr>, mapper: Box<Expr> },
    /// `fan.timeout(deadline) { body }` — the ORACLE-tier deadline (Stage 4):
    /// the body runs under a WALL-CLOCK deadline checked cooperatively at
    /// charge sites (the Go-context cancellation model). The verdict is
    /// ω-relative (ADR-0001 S8): which site the deadline hits depends on the
    /// host — record/replay (T5-2) makes an observed ω reproducible.
    FanTimeout { deadline: Box<Expr>, body: Box<Expr> },
    /// `fan.settle { arm; arm; … }` — collect EVERYTHING: each arm settles to
    /// its own `Result` slot, heterogeneous arm types allowed. The value is a
    /// TUPLE `(Result[A, String], Result[B, String], …)` in arm order (T2-4).
    FanSettle { arms: Vec<Expr> },
    ForIn { var: Sym, var_tuple: Option<Vec<Sym>>, iterable: Box<Expr>, body: Vec<Stmt> },
    While { cond: Box<Expr>, body: Vec<Stmt> },
    Lambda { params: Vec<LambdaParam>, body: Box<Expr> },
    Hole,
    Todo { message: String },
    Try { expr: Box<Expr> },
    Unwrap { expr: Box<Expr> },
    UnwrapOr { expr: Box<Expr>, fallback: Box<Expr> },
    ToOption { expr: Box<Expr> },
    OptionalChain { expr: Box<Expr>, field: Sym },
    Binary { op: Sym, left: Box<Expr>, right: Box<Expr> },
    Unary { op: Sym, operand: Box<Expr> },
    Paren { expr: Box<Expr> },
    Tuple { elements: Vec<Expr> },
    Range { start: Box<Expr>, end: Box<Expr>, inclusive: bool },
    Break,
    Continue,
    Placeholder,
    Unit,
    None,
    Some { expr: Box<Expr> },
    Ok { expr: Box<Expr> },
    Err { expr: Box<Expr> },
    /// Type ascription: `expr: Type` (e.g. `[]: List[Int]` in call args).
    TypeAscription { expr: Box<Expr>, #[serde(rename = "type")] ty: TypeExpr },
    /// Placeholder for a parse error — allows partial AST construction.
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldInit {
    pub name: Sym,
    pub value: Expr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
    /// Leading comments before this arm
    #[serde(skip)]
    pub comments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LambdaParam {
    pub name: Sym,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tuple_names: Option<Vec<Sym>>,
    #[serde(rename = "type")]
    pub ty: Option<TypeExpr>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Stmt {
    Let { name: Sym, #[serde(rename = "type")] ty: Option<TypeExpr>, value: Expr, #[serde(skip)] span: Option<Span> },
    LetDestructure { pattern: Pattern, value: Expr, #[serde(skip)] span: Option<Span> },
    Var { name: Sym, #[serde(rename = "type")] ty: Option<TypeExpr>, value: Expr, #[serde(skip)] span: Option<Span> },
    Assign { name: Sym, value: Expr, #[serde(skip)] span: Option<Span> },
    IndexAssign { target: Sym, index: Box<Expr>, value: Expr, #[serde(skip)] span: Option<Span> },
    FieldAssign { target: Sym, field: Sym, value: Expr, #[serde(skip)] span: Option<Span> },
    Guard { cond: Expr, else_: Expr, #[serde(skip)] span: Option<Span> },
    /// `guard let name = scrutinee else { else_ }` — Swift-style: `name` binds the value
    /// inside the scrutinee's Option/Result and stays in scope for the REST of the block;
    /// the else branch must diverge. The frontend desugars the block tail into the Some/Ok
    /// arm of a match on the scrutinee.
    GuardLet { name: Sym, scrutinee: Expr, else_: Expr, #[serde(skip)] span: Option<Span> },
    Expr { expr: Expr, #[serde(skip)] span: Option<Span> },
    Comment { text: String },
    /// Placeholder for a parse error — allows partial AST construction.
    Error { #[serde(skip)] span: Option<Span> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    pub name: Sym,
    #[serde(rename = "type")]
    pub ty: TypeExpr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Box<Expr>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<Attribute>,
    /// `mut` parameter modifier — the function may mutate this argument in place.
    /// Caller must pass a `var` binding (not `let` or temporary).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_mut: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,   // default — anyone can access
    Mod,      // same project only, not external packages
    Local,    // this file only
}

/// @extern(target, "module", "function") annotation for FFI declarations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternAttr {
    pub target: Sym,     // "rust" or "ts"
    pub module: Sym,     // e.g., "fast_lib"
    pub function: Sym,   // e.g., "reverse"
}

/// @export(c, "symbol") annotation — export function with C ABI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportAttr {
    pub target: Sym,     // "c"
    pub symbol: Sym,     // e.g., "bridge_add"
}

/// Generic `@name(args)` attribute on a declaration.
///
/// Hosts the stdlib unification attributes (`@inline_rust`,
/// `@wasm_intrinsic`, `@pure`, `@schedule`, `@rewrite`) and any
/// future metadata. The more rigid `@extern` / `@export` shapes are
/// still parsed into `ExternAttr` / `ExportAttr` for backward
/// compatibility; new attributes live here.
///
/// `args` preserves the source order of positional and named
/// arguments so that formatter round-trip matches input byte-for-byte.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attribute {
    pub name: Sym,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<AttrArg>,
    #[serde(skip)]
    pub span: Option<Span>,
}

/// One argument inside `@name(...)`. `name` is `None` for positional
/// arguments and `Some(sym)` for `name=value` pairs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttrArg {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<Sym>,
    pub value: AttrValue,
}

/// Literal kinds allowed inside attribute argument positions. The
/// enum is intentionally narrow: attributes describe compile-time
/// metadata, not arbitrary expressions, so we avoid pulling in the
/// full `Expr` grammar and its recursive dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AttrValue {
    /// `"literal"` — for code templates (`@inline_rust`) and
    /// arbitrary string payloads.
    String { value: String },
    /// `42`, `-1`, `0xff` — for numeric tuning values.
    Int { value: i64 },
    /// `true` / `false` — for boolean flags.
    Bool { value: bool },
    /// Unquoted identifier, e.g. `gpu` in `@schedule(device=gpu)`.
    /// Parsers should not interpret this as a reference to a variable;
    /// it is an attribute-level enum tag.
    Ident { name: Sym },
}

impl Default for Visibility {
    fn default() -> Self { Visibility::Public }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Decl {
    Module { path: Vec<Sym>, #[serde(skip)] span: Option<Span> },
    Import { path: Vec<Sym>, names: Option<Vec<Sym>>, alias: Option<Sym>, #[serde(skip)] span: Option<Span> },
    Type { name: Sym, #[serde(rename = "type")] ty: TypeExpr, deriving: Option<Vec<Sym>>, #[serde(default)] visibility: Visibility, #[serde(default)] generics: Option<Vec<GenericParam>>, #[serde(skip)] span: Option<Span> },
    Fn {
        name: Sym,
        #[serde(default)] effect: Option<bool>,
        #[serde(default)] visibility: Visibility,
        #[serde(default)] extern_attrs: Vec<ExternAttr>,
        #[serde(default)] export_attrs: Vec<ExportAttr>,
        /// Generic `@name(args)` attributes that are not `@extern` or
        /// `@export`. Stdlib unification attributes live here.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attrs: Vec<Attribute>,
        #[serde(default)] generics: Option<Vec<GenericParam>>,
        params: Vec<Param>,
        #[serde(rename = "returnType")] return_type: TypeExpr,
        body: Option<Expr>,
        #[serde(skip)] span: Option<Span>,
    },
    TopLet { name: Sym, #[serde(rename = "type")] ty: Option<TypeExpr>, value: Expr, #[serde(default)] mutable: bool, #[serde(default)] visibility: Visibility, #[serde(skip)] span: Option<Span> },
    Protocol { name: Sym, #[serde(default)] generics: Option<Vec<GenericParam>>, methods: Vec<ProtocolMethod>, #[serde(skip)] span: Option<Span> },
    Test { name: String, body: Expr, #[serde(default)] where_clauses: Vec<TestWhere>, #[serde(skip)] span: Option<Span> },
    /// `local test where { ... }` — file-scoped test environment
    TestWhereDef { scope: TestWhereScope, clauses: Vec<TestWhere>, #[serde(skip)] span: Option<Span> },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TestWhereScope { Local, Module }

/// A `where` clause in a test declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestWhere {
    /// `where name = expr` — value binding
    Bind { name: Sym, value: Expr },
    /// `where path.to.name = expr` — reference override
    Override { path: Vec<Sym>, value: Expr },
    /// `where target(args...) => expr` — call pattern response
    CallResponse { target: Vec<Sym>, params: Vec<Pattern>, response: Expr },
    /// `where "case name" { bindings... }` — table-driven test case
    Case { name: String, bindings: Vec<TestWhere> },
}

/// Comments attached to one EXPRESSION (#1404 / #1326).
///
/// The attachment rule, ruled 2026-08-14: **a comment binds to the node it is
/// adjacent to, on the side it was written**.
///
/// ```text
/// foo(/* why */ a, b)      // LEADING on `a`  — travels with `a` if it moves
/// f(1 /* x */, 2)          // TRAILING on `1` — does NOT cross the comma
/// let y = 1 + // why
///   2                      // TRAILING on `1` — the operand it follows
/// ```
///
/// The leading half is the ruling as asked; the trailing half is its mirror,
/// because taking "attach to the FOLLOWING node" literally would move
/// `/* x */` across the comma and onto `2`, annotating a value its author
/// never wrote it against. rustfmt and prettier bind the same way.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExprComments {
    /// Written before the node, on the same line.
    pub leading: Vec<String>,
    /// Written after the node — an inline `/* */` or an end-of-line `//` whose
    /// expression continues on the next line.
    pub trailing: Vec<String>,
}

/// A file's DIALECT STAMP — `@dialect(N)`, written above everything else.
///
/// `N` is the language-dialect epoch the file was last verified against, NOT
/// a compiler release: the epoch advances only when the language surface
/// changes in a way that can break already-written code, so a file's stamp
/// stays put across the many releases that change nothing a writer can see.
///
/// The stamp exists because Almide's users are code generators. A model
/// writes against whatever dialect it learned; recording that dialect in the
/// file is what lets the compiler say "this was written for epoch 2, and
/// here is what moved since" instead of reporting an unexplained error — and
/// what lets modification-survival rate be measured per dialect rather than
/// in aggregate. `almide fmt` advances the stamp, forward only, and only
/// after the file checks clean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialectStamp {
    pub epoch: u32,
    #[serde(skip)]
    pub span: Option<Span>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Program {
    /// `@dialect(N)` if the file carries one. Absent is legal and silent —
    /// every file written before the stamp existed is unstamped, and
    /// demanding one would be the breaking change the stamp exists to avoid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialect: Option<DialectStamp>,
    pub module: Option<Decl>,
    pub imports: Vec<Decl>,
    pub decls: Vec<Decl>,
    /// Leading comments for each section:
    /// - index 0: comments before module/first import
    /// - index 1..=imports.len(): comments before each import (1-indexed)
    /// - remaining: comments before each decl
    #[serde(skip)]
    pub comment_map: Vec<Vec<String>>,
    /// Doc comments (`///`) for each decl (parallel to `decls`).
    #[serde(skip)]
    pub doc_map: Vec<Option<String>>,
    /// Number of blank lines before each decl (parallel to `decls`).
    #[serde(skip)]
    pub blank_lines_map: Vec<u32>,
    /// Names of fn declarations whose body parse failed. Lets the checker
    /// suppress cascading "undefined function" diagnostics from call sites.
    #[serde(skip)]
    pub failed_fn_names: std::collections::HashSet<String>,
    /// True when the parser reported any error: recovery dropped source text
    /// (a declaration, a fn body, a statement), so usage-based verdicts over
    /// this file — the E060 unused-import pass — are judged on an incomplete
    /// picture and stay silent; the parse error is already the diagnosis
    /// (#1783's gauntlet `s3` cell, where the refused `ports.Store` bound
    /// takes the only uses of `ports` with it).
    #[serde(skip)]
    pub parse_recovered: bool,
    /// #1404: comments bound to an EXPRESSION, keyed by `ExprId`.
    ///
    /// A SIDE TABLE rather than a field on `Expr`, deliberately: `Expr` is
    /// constructed in hundreds of places across the parser and rebuilt by every
    /// desugar, and a field would have to be threaded (and preserved) through
    /// all of them — a comment silently dropped by one rebuild is exactly the
    /// bug class the fmt conservation verifier exists to catch. Keyed by an id
    /// the node already carries, nothing downstream has to know this exists.
    #[serde(skip)]
    pub expr_comments: std::collections::HashMap<ExprId, ExprComments>,
}

// ── Generic AST visitor ──

/// Apply `f` to every `Expr` node reachable from a `Program`.
pub fn visit_exprs_mut(program: &mut Program, f: &mut impl FnMut(&mut Expr)) {
    for decl in program.decls.iter_mut() { visit_decl_exprs_mut(decl, f); }
}

/// Drop every literal's cached source spelling, so `fmt` renders the tree from
/// its VALUES again.
///
/// Mandatory for any tool that MUTATES a parsed AST and then re-renders it
/// (`almide fix`'s rewrites, the differential fuzzer's mutators): an
/// `InterpolatedString`'s `raw` spans its `${…}` holes, so a rewrite landing
/// inside a hole would be silently dropped by a verbatim reprint. Stripping is
/// the conservative direction — the worst case is that literals come back
/// normalized, never that an edit disappears.
pub fn strip_literal_raw(program: &mut Program) {
    visit_exprs_mut(program, &mut |e: &mut Expr| match &mut e.kind {
        ExprKind::Float { raw, .. }
        | ExprKind::String { raw, .. }
        | ExprKind::InterpolatedString { raw, .. } => *raw = None,
        _ => {}
    });
}

pub fn visit_decl_exprs_mut(decl: &mut Decl, f: &mut impl FnMut(&mut Expr)) {
    match decl {
        Decl::Fn { params, body, .. } => {
            for p in params.iter_mut() {
                if let Some(ref mut default) = p.default { visit_expr_mut(default, f); }
            }
            if let Some(b) = body { visit_expr_mut(b, f); }
        }
        Decl::TopLet { value, .. } => visit_expr_mut(value, f),
        Decl::Test { body, where_clauses, .. } => {
            for wc in where_clauses.iter_mut() { visit_test_where_exprs_mut(wc, f); }
            visit_expr_mut(body, f);
        }
        Decl::TestWhereDef { clauses, .. } => {
            for wc in clauses.iter_mut() { visit_test_where_exprs_mut(wc, f); }
        }
        Decl::Module { .. } | Decl::Import { .. } | Decl::Type { .. } |
        Decl::Protocol { .. } => {}
    }
}

fn visit_test_where_exprs_mut(wc: &mut TestWhere, f: &mut impl FnMut(&mut Expr)) {
    match wc {
        TestWhere::Bind { value, .. } | TestWhere::Override { value, .. } => visit_expr_mut(value, f),
        TestWhere::CallResponse { response, .. } => visit_expr_mut(response, f),
        TestWhere::Case { bindings, .. } => { for b in bindings.iter_mut() { visit_test_where_exprs_mut(b, f); } }
    }
}

fn visit_stmt_exprs_mut(stmt: &mut Stmt, f: &mut impl FnMut(&mut Expr)) {
    match stmt {
        Stmt::Let { value, .. } | Stmt::Var { value, .. } | Stmt::Assign { value, .. } => visit_expr_mut(value, f),
        Stmt::LetDestructure { pattern, value, .. } => {
            visit_pattern_exprs_mut(pattern, f);
            visit_expr_mut(value, f);
        }
        Stmt::IndexAssign { index, value, .. } => { visit_expr_mut(index, f); visit_expr_mut(value, f); }
        Stmt::FieldAssign { value, .. } => visit_expr_mut(value, f),
        Stmt::Guard { cond, else_, .. } => { visit_expr_mut(cond, f); visit_expr_mut(else_, f); }
        Stmt::GuardLet { scrutinee, else_, .. } => { visit_expr_mut(scrutinee, f); visit_expr_mut(else_, f); }
        Stmt::Expr { expr, .. } => visit_expr_mut(expr, f),
        Stmt::Comment { .. } | Stmt::Error { .. } => {}
    }
}

fn visit_pattern_exprs_mut(pat: &mut Pattern, f: &mut impl FnMut(&mut Expr)) {
    match pat {
        Pattern::Literal { value } => visit_expr_mut(value, f),
        Pattern::Constructor { args, .. } => { for a in args.iter_mut() { visit_pattern_exprs_mut(a, f); } }
        Pattern::RecordPattern { fields, .. } => {
            for fp in fields.iter_mut() { if let Some(ref mut p) = fp.pattern { visit_pattern_exprs_mut(p, f); } }
        }
        Pattern::Tuple { elements } | Pattern::List { elements, .. } => { for e in elements.iter_mut() { visit_pattern_exprs_mut(e, f); } }
        Pattern::As { inner, .. } => visit_pattern_exprs_mut(inner, f),
        Pattern::Some { inner } | Pattern::Ok { inner } | Pattern::Err { inner } => visit_pattern_exprs_mut(inner, f),
        Pattern::Or { alts } => { for a in alts.iter_mut() { visit_pattern_exprs_mut(a, f); } }
        Pattern::Wildcard | Pattern::Ident { .. } | Pattern::None => {}
    }
}

/// Visits each element of an expression slice (List/Tuple/Fan payloads).
fn visit_exprs_slice_mut(exprs: &mut [Expr], f: &mut impl FnMut(&mut Expr)) {
    for e in exprs.iter_mut() { visit_expr_mut(e, f); }
}

/// Visits each key/value pair of a map literal.
fn visit_map_entries_mut(entries: &mut [(Expr, Expr)], f: &mut impl FnMut(&mut Expr)) {
    for (k, v) in entries.iter_mut() { visit_expr_mut(k, f); visit_expr_mut(v, f); }
}

/// Visits each field value of a record literal (Record/SpreadRecord payloads).
fn visit_field_inits_mut(fields: &mut [FieldInit], f: &mut impl FnMut(&mut Expr)) {
    for fi in fields.iter_mut() { visit_expr_mut(&mut fi.value, f); }
}

/// Visits pattern/guard/body of each match arm.
fn visit_match_arms_mut(arms: &mut [MatchArm], f: &mut impl FnMut(&mut Expr)) {
    for arm in arms.iter_mut() {
        visit_pattern_exprs_mut(&mut arm.pattern, f);
        if let Some(ref mut g) = arm.guard { visit_expr_mut(g, f); }
        visit_expr_mut(&mut arm.body, f);
    }
}

/// Visits a statement list (Block/ForIn/While bodies).
fn visit_stmts_mut(stmts: &mut [Stmt], f: &mut impl FnMut(&mut Expr)) {
    for s in stmts.iter_mut() { visit_stmt_exprs_mut(s, f); }
}

/// Visits the embedded expressions of an interpolated string.
fn visit_string_parts_mut(parts: &mut [StringPart], f: &mut impl FnMut(&mut Expr)) {
    for part in parts.iter_mut() {
        if let StringPart::Expr { expr: e } = part { visit_expr_mut(e, f); }
    }
}

/// Apply `f` to `expr` and then to every child expression, grouped by CHILD
/// SHAPE (one / two / three / sequence / name-tagged / bespoke). Exhaustive with
/// no wildcard, so a new `ExprKind` fails to compile here until its shape is
/// declared.
pub fn visit_expr_mut(expr: &mut Expr, f: &mut impl FnMut(&mut Expr)) {
    f(expr);
    match &mut expr.kind {
        // ── One child ──
        ExprKind::Member { object: e, .. } | ExprKind::TupleIndex { object: e, .. }
        | ExprKind::Unary { operand: e, .. } | ExprKind::Lambda { body: e, .. }
        | ExprKind::Try { expr: e } | ExprKind::Unwrap { expr: e }
        | ExprKind::ToOption { expr: e } | ExprKind::Paren { expr: e }
        | ExprKind::Some { expr: e } | ExprKind::Ok { expr: e } | ExprKind::Err { expr: e }
        | ExprKind::OptionalChain { expr: e, .. }
        | ExprKind::TypeAscription { expr: e, .. } => visit_expr_mut(e, f),

        // ── Two children, left to right ──
        ExprKind::Binary { left: a, right: b, .. } | ExprKind::Pipe { left: a, right: b }
        | ExprKind::Compose { left: a, right: b }
        | ExprKind::UnwrapOr { expr: a, fallback: b }
        | ExprKind::IndexAccess { object: a, index: b }
        | ExprKind::Range { start: a, end: b, .. } => {
            visit_expr_mut(a, f);
            visit_expr_mut(b, f);
        }

        // ── Three children ──
        ExprKind::If { cond: a, then: b, else_: c }
        | ExprKind::IfLet { scrutinee: a, then: b, else_: c, .. } => {
            visit_expr_mut(a, f);
            visit_expr_mut(b, f);
            visit_expr_mut(c, f);
        }

        // ── A flat sequence of children ──
        ExprKind::List { elements: xs } | ExprKind::Tuple { elements: xs }
        | ExprKind::Fan { exprs: xs } | ExprKind::FanSettle { arms: xs } => {
            visit_exprs_slice_mut(xs, f)
        }

        // ── Name-tagged children ──
        ExprKind::MapLiteral { entries } => visit_map_entries_mut(entries, f),
        ExprKind::Record { fields, .. } => visit_field_inits_mut(fields, f),
        ExprKind::SpreadRecord { base, fields } => {
            visit_expr_mut(base, f);
            visit_field_inits_mut(fields, f);
        }

        // ── The fan family: `f` is applied SHALLOWLY to the budget and the
        //    body/arms (no recursion into their subtrees) — the pre-existing
        //    contract, kept verbatim.
        ExprKind::FanBounded { budget, body } => {
            f(budget);
            f(body);
        }
        ExprKind::FanRace { budget, arms } => {
            visit_opt_shallow_mut(budget, f);
            visit_exprs_slice_mut(arms, f);
        }
        ExprKind::FanRaceMap { budget, list, mapper } => {
            visit_opt_shallow_mut(budget, f);
            f(list);
            f(mapper);
        }
        ExprKind::FanTimeout { deadline, body } => {
            f(deadline);
            f(body);
        }

        // ── Shapes with their own traversal order ──
        ExprKind::Call { callee, args, named_args, .. } => {
            visit_expr_mut(callee, f);
            visit_exprs_slice_mut(args, f);
            visit_named_args_mut(named_args, f);
        }
        ExprKind::Match { subject, arms } => {
            visit_expr_mut(subject, f);
            visit_match_arms_mut(arms, f);
        }
        ExprKind::Block { stmts, expr: tail } => visit_block_mut(stmts, tail, f),
        ExprKind::ForIn { iterable: lead, body, .. }
        | ExprKind::While { cond: lead, body } => {
            visit_expr_mut(lead, f);
            visit_stmts_mut(body, f);
        }
        ExprKind::InterpolatedString { parts, .. } => visit_string_parts_mut(parts, f),

        // ── No children ──
        ExprKind::Int { .. } | ExprKind::Float { .. } | ExprKind::String { .. } |
        ExprKind::Bool { .. } | ExprKind::Ident { .. } | ExprKind::TypeName { .. } |
        ExprKind::EmptyMap | ExprKind::Hole | ExprKind::Todo { .. } |
        ExprKind::Break | ExprKind::Continue | ExprKind::Placeholder |
        ExprKind::Unit | ExprKind::None | ExprKind::Error => {}
    }
}

/// An optional child the fan family applies `f` to WITHOUT recursing.
fn visit_opt_shallow_mut(e: &mut Option<Box<Expr>>, f: &mut impl FnMut(&mut Expr)) {
    if let Some(b) = e {
        f(b);
    }
}

/// A call's named arguments — the name plays no part in the walk.
fn visit_named_args_mut(args: &mut [(Sym, Expr)], f: &mut impl FnMut(&mut Expr)) {
    for (_, a) in args.iter_mut() {
        visit_expr_mut(a, f);
    }
}

/// A block: every statement, then the tail expression if there is one.
fn visit_block_mut(
    stmts: &mut [Stmt],
    tail: &mut Option<Box<Expr>>,
    f: &mut impl FnMut(&mut Expr),
) {
    visit_stmts_mut(stmts, f);
    if let Some(e) = tail {
        visit_expr_mut(e, f);
    }
}

// ── Read-only AST visitor (#1231) ──
//
// Mirror of `visit_expr_mut` for callers that only OBSERVE the tree (counting
// uses, scanning for a node kind) — before this existed, every such caller had
// to clone the subtree just to run the mutable walker. Identical traversal
// order and identical shallow-visit contracts; keep the two families in
// lockstep. Exhaustive with no wildcard, so a new `ExprKind` fails to compile
// here until its shape is declared.

fn visit_stmt_exprs(stmt: &Stmt, f: &mut impl FnMut(&Expr)) {
    match stmt {
        Stmt::Let { value, .. } | Stmt::Var { value, .. } | Stmt::Assign { value, .. } => visit_expr(value, f),
        Stmt::LetDestructure { pattern, value, .. } => {
            visit_pattern_exprs(pattern, f);
            visit_expr(value, f);
        }
        Stmt::IndexAssign { index, value, .. } => { visit_expr(index, f); visit_expr(value, f); }
        Stmt::FieldAssign { value, .. } => visit_expr(value, f),
        Stmt::Guard { cond, else_, .. } => { visit_expr(cond, f); visit_expr(else_, f); }
        Stmt::GuardLet { scrutinee, else_, .. } => { visit_expr(scrutinee, f); visit_expr(else_, f); }
        Stmt::Expr { expr, .. } => visit_expr(expr, f),
        Stmt::Comment { .. } | Stmt::Error { .. } => {}
    }
}

fn visit_pattern_exprs(pat: &Pattern, f: &mut impl FnMut(&Expr)) {
    match pat {
        Pattern::Literal { value } => visit_expr(value, f),
        Pattern::Constructor { args, .. } => { for a in args.iter() { visit_pattern_exprs(a, f); } }
        Pattern::RecordPattern { fields, .. } => {
            for fp in fields.iter() { if let Some(ref p) = fp.pattern { visit_pattern_exprs(p, f); } }
        }
        Pattern::Tuple { elements } | Pattern::List { elements, .. } => { for e in elements.iter() { visit_pattern_exprs(e, f); } }
        Pattern::As { inner, .. } => visit_pattern_exprs(inner, f),
        Pattern::Some { inner } | Pattern::Ok { inner } | Pattern::Err { inner } => visit_pattern_exprs(inner, f),
        Pattern::Or { alts } => { for a in alts.iter() { visit_pattern_exprs(a, f); } }
        Pattern::Wildcard | Pattern::Ident { .. } | Pattern::None => {}
    }
}

/// Visits each element of an expression slice (List/Tuple/Fan payloads).
fn visit_exprs_slice(exprs: &[Expr], f: &mut impl FnMut(&Expr)) {
    for e in exprs.iter() { visit_expr(e, f); }
}

/// Visits each key/value pair of a map literal.
fn visit_map_entries(entries: &[(Expr, Expr)], f: &mut impl FnMut(&Expr)) {
    for (k, v) in entries.iter() { visit_expr(k, f); visit_expr(v, f); }
}

/// Visits each field value of a record literal (Record/SpreadRecord payloads).
fn visit_field_inits(fields: &[FieldInit], f: &mut impl FnMut(&Expr)) {
    for fi in fields.iter() { visit_expr(&fi.value, f); }
}

/// Visits pattern/guard/body of each match arm.
fn visit_match_arms(arms: &[MatchArm], f: &mut impl FnMut(&Expr)) {
    for arm in arms.iter() {
        visit_pattern_exprs(&arm.pattern, f);
        if let Some(ref g) = arm.guard { visit_expr(g, f); }
        visit_expr(&arm.body, f);
    }
}

/// Visits a statement list (Block/ForIn/While bodies).
fn visit_stmts(stmts: &[Stmt], f: &mut impl FnMut(&Expr)) {
    for s in stmts.iter() { visit_stmt_exprs(s, f); }
}

/// Visits the embedded expressions of an interpolated string.
fn visit_string_parts(parts: &[StringPart], f: &mut impl FnMut(&Expr)) {
    for part in parts.iter() {
        if let StringPart::Expr { expr: e } = part { visit_expr(e, f); }
    }
}

/// Apply `f` to `expr` and then to every child expression — the read-only
/// mirror of `visit_expr_mut`, same order, same fan-family shallow contract.
pub fn visit_expr(expr: &Expr, f: &mut impl FnMut(&Expr)) {
    f(expr);
    match &expr.kind {
        // ── One child ──
        ExprKind::Member { object: e, .. } | ExprKind::TupleIndex { object: e, .. }
        | ExprKind::Unary { operand: e, .. } | ExprKind::Lambda { body: e, .. }
        | ExprKind::Try { expr: e } | ExprKind::Unwrap { expr: e }
        | ExprKind::ToOption { expr: e } | ExprKind::Paren { expr: e }
        | ExprKind::Some { expr: e } | ExprKind::Ok { expr: e } | ExprKind::Err { expr: e }
        | ExprKind::OptionalChain { expr: e, .. }
        | ExprKind::TypeAscription { expr: e, .. } => visit_expr(e, f),

        // ── Two children, left to right ──
        ExprKind::Binary { left: a, right: b, .. } | ExprKind::Pipe { left: a, right: b }
        | ExprKind::Compose { left: a, right: b }
        | ExprKind::UnwrapOr { expr: a, fallback: b }
        | ExprKind::IndexAccess { object: a, index: b }
        | ExprKind::Range { start: a, end: b, .. } => {
            visit_expr(a, f);
            visit_expr(b, f);
        }

        // ── Three children ──
        ExprKind::If { cond: a, then: b, else_: c }
        | ExprKind::IfLet { scrutinee: a, then: b, else_: c, .. } => {
            visit_expr(a, f);
            visit_expr(b, f);
            visit_expr(c, f);
        }

        // ── A flat sequence of children ──
        ExprKind::List { elements: xs } | ExprKind::Tuple { elements: xs }
        | ExprKind::Fan { exprs: xs } | ExprKind::FanSettle { arms: xs } => {
            visit_exprs_slice(xs, f)
        }

        // ── Name-tagged children ──
        ExprKind::MapLiteral { entries } => visit_map_entries(entries, f),
        ExprKind::Record { fields, .. } => visit_field_inits(fields, f),
        ExprKind::SpreadRecord { base, fields } => {
            visit_expr(base, f);
            visit_field_inits(fields, f);
        }

        // ── The fan family: `f` is applied SHALLOWLY to the budget and the
        //    body/arms (no recursion into their subtrees) — the pre-existing
        //    contract, kept verbatim.
        ExprKind::FanBounded { budget, body } => {
            f(budget);
            f(body);
        }
        ExprKind::FanRace { budget, arms } => {
            visit_opt_shallow(budget, f);
            visit_exprs_slice(arms, f);
        }
        ExprKind::FanRaceMap { budget, list, mapper } => {
            visit_opt_shallow(budget, f);
            f(list);
            f(mapper);
        }
        ExprKind::FanTimeout { deadline, body } => {
            f(deadline);
            f(body);
        }

        // ── Shapes with their own traversal order ──
        ExprKind::Call { callee, args, named_args, .. } => {
            visit_expr(callee, f);
            visit_exprs_slice(args, f);
            visit_named_args(named_args, f);
        }
        ExprKind::Match { subject, arms } => {
            visit_expr(subject, f);
            visit_match_arms(arms, f);
        }
        ExprKind::Block { stmts, expr: tail } => visit_block(stmts, tail, f),
        ExprKind::ForIn { iterable: lead, body, .. }
        | ExprKind::While { cond: lead, body } => {
            visit_expr(lead, f);
            visit_stmts(body, f);
        }
        ExprKind::InterpolatedString { parts, .. } => visit_string_parts(parts, f),

        // ── No children ──
        ExprKind::Int { .. } | ExprKind::Float { .. } | ExprKind::String { .. } |
        ExprKind::Bool { .. } | ExprKind::Ident { .. } | ExprKind::TypeName { .. } |
        ExprKind::EmptyMap | ExprKind::Hole | ExprKind::Todo { .. } |
        ExprKind::Break | ExprKind::Continue | ExprKind::Placeholder |
        ExprKind::Unit | ExprKind::None | ExprKind::Error => {}
    }
}

/// An optional child the fan family applies `f` to WITHOUT recursing.
fn visit_opt_shallow(e: &Option<Box<Expr>>, f: &mut impl FnMut(&Expr)) {
    if let Some(b) = e {
        f(b);
    }
}

/// A call's named arguments — the name plays no part in the walk.
fn visit_named_args(args: &[(Sym, Expr)], f: &mut impl FnMut(&Expr)) {
    for (_, a) in args.iter() {
        visit_expr(a, f);
    }
}

/// A block: every statement, then the tail expression if there is one.
fn visit_block(
    stmts: &[Stmt],
    tail: &Option<Box<Expr>>,
    f: &mut impl FnMut(&Expr),
) {
    visit_stmts(stmts, f);
    if let Some(e) = tail {
        visit_expr(e, f);
    }
}
