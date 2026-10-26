//! Every error text the Codec derive builds embeds its input through C-418's
//! escaper (#4263, security table row 22).
//!
//! `tests/runtime_err_text_operands_test.rs` holds C-418 over the runtime's
//! source; a text the compiler SYNTHESIZES during lowering is outside what it
//! reads. `T.decode_strict`'s `unknown key '<k>'` was one: the attacker's key
//! was concatenated raw, so ESC, a bidi override or a newline reached the error
//! and `http.decode_json`'s 400 body (C-436).
//!
//! This gate reads the derive's OUTPUT, not a list of messages: it lowers a
//! program of Codec types only (records, nested records, lists, options, a
//! variant with unit, tuple and record cases), so every function in the IR is
//! one the derive emitted, and walks every string the IR builds by `+` or by
//! interpolation. An operand passes when it is
//!
//! * a literal (the derive's own text: a type, field or case name),
//! * a call to `string.__err_text` (C-418's escaper), or
//! * a value computed from scalars only (`int.to_string(len)`): every argument
//!   is a number or a Bool, so no text of the input reaches it.
//!
//! Anything else (a variable, a field read, a call taking text or a `Value`)
//! is input text reaching an error raw, and fails the gate. An error handed
//! back unchanged (`err(e)`) builds no text and is gated where it was made.
//! The control `a_raw_operand_is_caught` runs the same predicate over
//! hand-built twins of the old and new shapes and requires the old one caught.

use almide::canonicalize;
use almide::check::Checker;
use almide::ir::visit::{walk_expr, IrVisitor};
use almide::ir::*;
use almide::lexer::Lexer;
use almide::lower::lower_program;
use almide::parser::Parser;
use almide::types::Ty;

fn lower(input: &str) -> IrProgram {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    let mut prog = parser.parse().expect("parse failed");
    let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
    let mut checker = Checker::from_env(canon.env);
    checker.diagnostics = canon.diagnostics;
    checker.infer_program(&mut prog);
    lower_program(&prog, &checker.env, &checker.type_map)
}

/// Every Codec shape the derive has a builder for.
const SRC: &str = "\
type Geo: Codec = { lat: Float, label: String }
type Address: Codec = { city: String, geo: Geo, tags: List[Geo], near: Option[Geo], spots: List[List[Geo]] }
type Command: Codec = | Rename { name: String, at: Geo } | Pin(Geo, Int) | Move(List[Geo]) | Delete
type Input: Decode = { name: String, address: Address?, commands: List[Command] = [] }
";

fn is_scalar(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Int | Ty::Float | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
            | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64 | Ty::Float32 | Ty::Float64 | Ty::Bool
    )
}

fn is_escaper(e: &IrExpr) -> bool {
    matches!(&e.kind, IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. }
        if module.as_str() == "string" && func.as_str() == "__err_text")
}

/// Is `e`, as one piece of a text being built, free of raw input text?
fn operand_ok(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::LitStr { .. } => true,
        // A nested `+` is walked as a node of its own.
        IrExprKind::BinOp { op: BinOp::ConcatStr, .. } | IrExprKind::StringInterp { .. } => true,
        IrExprKind::Call { args, .. } => is_escaper(e) || args.iter().all(|a| is_scalar(&a.ty)),
        _ => is_scalar(&e.ty),
    }
}

#[derive(Default)]
struct Texts {
    raw: Vec<String>,
    escaped: usize,
    built: usize,
}

impl Texts {
    fn piece(&mut self, f: &str, e: &IrExpr) {
        self.built += 1;
        if is_escaper(e) {
            self.escaped += 1;
        }
        if !operand_ok(e) {
            self.raw.push(format!("{f}: {:?}", e.kind));
        }
    }
}

struct Walk<'a> {
    f: &'a str,
    texts: &'a mut Texts,
}

impl IrVisitor for Walk<'_> {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::BinOp { op: BinOp::ConcatStr, left, right } => {
                self.texts.piece(self.f, left);
                self.texts.piece(self.f, right);
            }
            IrExprKind::StringInterp { parts } => {
                for p in parts {
                    if let IrStringPart::Expr { expr } = p {
                        self.texts.piece(self.f, expr);
                    }
                }
            }
            _ => {}
        }
        walk_expr(self, e);
    }
}

fn texts_of(ir: &IrProgram) -> Texts {
    let mut texts = Texts::default();
    for f in &ir.functions {
        let name = f.name.to_string();
        Walk { f: &name, texts: &mut texts }.visit_expr(&f.body);
    }
    texts
}

#[test]
fn every_derived_error_text_escapes_its_input() {
    let ir = lower(SRC);
    let names: Vec<String> = ir.functions.iter().map(|f| f.name.to_string()).collect();
    // The walk reaches the strict family: without it the gate is vacuous.
    for want in ["Address.__strict", "Command.__strict", "Input.decode_strict"] {
        assert!(names.iter().any(|n| n == want), "the derive did not emit `{want}`; emitted: {names:?}");
    }
    let texts = texts_of(&ir);
    assert!(
        texts.raw.is_empty(),
        "a derived error text embeds input text without C-418's escaper (#4263):\n{}",
        texts.raw.join("\n")
    );
    // Records and variant bodies each refuse an unknown key, escaped.
    assert!(texts.escaped >= 2, "expected the unknown-key texts to pass through string.__err_text; built {} pieces, {} escaped", texts.built, texts.escaped);
}

fn e(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}

fn module_call(m: &str, f: &str, args: Vec<IrExpr>) -> IrExpr {
    e(
        IrExprKind::Call {
            target: CallTarget::Module { module: almide::intern::sym(m), func: almide::intern::sym(f), def_id: None },
            args,
            type_args: vec![],
        },
        Ty::String,
    )
}

/// The old `unknown key '<k>'` (the key concatenated raw) is caught; the
/// escaped key, a literal and `int.to_string(n)` are not.
#[test]
fn a_raw_operand_is_caught() {
    let mut vt = VarTable::new();
    let k = vt.alloc(almide::intern::sym("_k"), Ty::String, Mutability::Let, None);
    let n = vt.alloc(almide::intern::sym("_n"), Ty::Int, Mutability::Let, None);
    let key = || e(IrExprKind::Var { id: k }, Ty::String);
    let lit = e(IrExprKind::LitStr { value: "unknown key '".into() }, Ty::String);
    assert!(!operand_ok(&key()), "a raw key must be caught");
    assert!(!operand_ok(&module_call("value", "as_string", vec![key()])), "a call over text must be caught");
    assert!(operand_ok(&lit));
    assert!(operand_ok(&module_call("string", "__err_text", vec![key()])));
    assert!(operand_ok(&module_call("int", "to_string", vec![e(IrExprKind::Var { id: n }, Ty::Int)])));
    let raw = e(IrExprKind::BinOp { op: BinOp::ConcatStr, left: Box::new(lit), right: Box::new(key()) }, Ty::String);
    let mut texts = Texts::default();
    Walk { f: "twin", texts: &mut texts }.visit_expr(&raw);
    assert_eq!(texts.raw.len(), 1, "the walk must report the raw twin");
}
