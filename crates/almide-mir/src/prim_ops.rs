//! The primitive floor's machine ops, read from their declarations (#4006).
//!
//! Every fn of `stdlib/prim.almd` declares what it IS with exactly one
//! `@prim_op(op, key = value…)`: `@prim_op(load, width=4)` on `load32`,
//! `@prim_op(alloc, block=owned_slots)` on `alloc_list_str`. The lowering
//! looks a prim call up here and dispatches on the decoded [`PrimOp`]; no
//! prim fn's name appears in the compiler, so a prim is added or renamed in
//! prim.almd alone and the op vocabulary below is the only thing MIR knows.
//!
//! The table is parsed once from the compiler's embedded copy of prim.almd
//! with the real parser. A malformed declaration is not skipped: the table
//! is `Err` and every prim call walls naming it, and
//! `crates/almide-mir/tests/prim_op_declared.rs` fails the build's tests on
//! any fn without exactly one decodable `@prim_op`.

use std::collections::HashMap;
use std::sync::OnceLock;

use almide_lang::ast::{AttrValue, Attribute, Decl};

/// The block an `alloc` op creates (its `block = …` key).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AllocBlock {
    /// A byte block (`String` / `Bytes`): `[rc][len][cap][bytes…]`.
    Bytes,
    /// Scalar i64 slots, no element ownership.
    Slots,
    /// Slots that own their heap elements (a recursive drop frees them).
    OwnedSlots,
}

/// One prim fn's declared machine op.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimOp {
    /// A heap value's block address as an `Int`.
    Addr,
    /// Write the message block to stderr and exit 1.
    Abort,
    /// Load `width` bytes, zero-extended.
    Load { width: u8 },
    /// Load a 4-byte handle, keeping its pointer repr (a borrow of the slot).
    LoadRef,
    /// Store the low `width` bytes.
    Store { width: u8 },
    /// Store a heap value's handle into a slot, moving the value in.
    StoreOwned,
    /// A scalar integer bit operation.
    Int(crate::IntOp),
    /// Allocate a runtime-sized block.
    Alloc(AllocBlock),
    /// A raw reference-count step (`true` = increment).
    Rc { inc: bool },
    /// A pointer/integer reinterpretation.
    Ptr,
    /// A host call (I/O, clock, randomness) not described further yet.
    Host,
    /// A float-floor op that maps one-to-one onto a `PrimKind`.
    Direct(crate::PrimKind),
}

fn ident(v: &AttrValue) -> Option<&str> {
    match v {
        AttrValue::Ident { name } => Some(name.as_str()),
        _ => None,
    }
}

/// Decode one `@prim_op(…)`. The first positional argument names the op;
/// named arguments refine it.
pub fn decode(attr: &Attribute) -> Result<PrimOp, String> {
    let mut positional = attr.args.iter().filter(|a| a.name.is_none());
    let op = positional
        .next()
        .and_then(|a| ident(&a.value))
        .ok_or("@prim_op needs the op as its first argument")?;
    if positional.next().is_some() {
        return Err(format!("@prim_op({op}) takes one positional argument"));
    }
    let d = Decoder { attr, op };
    if let Some(decoded) = d.machine_op()? {
        return Ok(decoded);
    }
    if let Some(decoded) = d.float_op()? {
        return Ok(decoded);
    }
    Err(format!("@prim_op({op}) is not an op"))
}

/// One `@prim_op(op, …)` being decoded: the attribute and its op name.
struct Decoder<'a> {
    attr: &'a Attribute,
    op: &'a str,
}

impl<'a> Decoder<'a> {
    fn named(&self, key: &str) -> Option<&'a AttrValue> {
        self.attr
            .args
            .iter()
            .find(|a| a.name.as_ref().is_some_and(|n| n.as_str() == key))
            .map(|a| &a.value)
    }

    fn width(&self) -> Result<u8, String> {
        match self.named("width") {
            Some(AttrValue::Int { value }) if matches!(*value, 1 | 4 | 8) => Ok(*value as u8),
            _ => Err(format!("@prim_op({}) needs `width = 1 | 4 | 8`", self.op)),
        }
    }

    fn sub(&self) -> Result<&'a str, String> {
        self.named("op")
            .and_then(ident)
            .ok_or(format!("@prim_op({}) needs `op = …`", self.op))
    }

    fn bad<T>(&self, s: &str) -> Result<T, String> {
        Err(format!("@prim_op({}, op = {s}) is not an op", self.op))
    }

    /// The memory, integer, allocation and host ops; `None` for any other name.
    fn machine_op(&self) -> Result<Option<PrimOp>, String> {
        Ok(Some(match self.op {
            "addr" => PrimOp::Addr,
            "abort" => PrimOp::Abort,
            "load" => PrimOp::Load { width: self.width()? },
            "load_ref" => PrimOp::LoadRef,
            "store" => PrimOp::Store { width: self.width()? },
            "store_owned" => PrimOp::StoreOwned,
            "int" => PrimOp::Int(self.int_op()?),
            "alloc" => PrimOp::Alloc(self.alloc_block()?),
            "rc" => PrimOp::Rc { inc: self.rc_inc()? },
            "ptr" => self.ptr_op()?,
            "host" => PrimOp::Host,
            _ => return Ok(None),
        }))
    }

    /// The float-floor ops; `None` for any other name.
    fn float_op(&self) -> Result<Option<PrimOp>, String> {
        use crate::PrimKind;
        Ok(Some(match self.op {
            "float_un" => PrimOp::Direct(PrimKind::FloatUn(self.float_un()?)),
            "float_bin" => PrimOp::Direct(PrimKind::FloatBin(self.float_bin()?)),
            "float_cmp" => PrimOp::Direct(PrimKind::FloatCmp(self.float_cmp()?)),
            "float_to_int" => PrimOp::Direct(PrimKind::FloatToInt),
            "int_to_float" => PrimOp::Direct(PrimKind::IntToFloat),
            "float_bits" => PrimOp::Direct(PrimKind::FloatBits),
            "f32_demote" => PrimOp::Direct(PrimKind::F32Demote),
            "f32_promote" => PrimOp::Direct(PrimKind::F32Promote),
            "int_to_f32" => PrimOp::Direct(PrimKind::IntToF32),
            "f32_bits" => PrimOp::Direct(PrimKind::F32Bits),
            _ => return Ok(None),
        }))
    }

    fn int_op(&self) -> Result<crate::IntOp, String> {
        use crate::IntOp;
        Ok(match self.sub()? {
            "bit_and" => IntOp::And,
            "bit_or" => IntOp::Or,
            "bit_xor" => IntOp::Xor,
            "shl" => IntOp::Shl,
            "shr" => IntOp::Shr,
            "shr_u" => IntOp::ShrU,
            s => return self.bad(s),
        })
    }

    fn alloc_block(&self) -> Result<AllocBlock, String> {
        Ok(match self.named("block").and_then(ident) {
            Some("bytes") => AllocBlock::Bytes,
            Some("slots") => AllocBlock::Slots,
            Some("owned_slots") => AllocBlock::OwnedSlots,
            _ => return Err("@prim_op(alloc) needs `block = bytes | slots | owned_slots`".into()),
        })
    }

    fn rc_inc(&self) -> Result<bool, String> {
        match self.sub()? {
            "inc" => Ok(true),
            "dec" => Ok(false),
            s => self.bad(s),
        }
    }

    fn ptr_op(&self) -> Result<PrimOp, String> {
        match self.sub()? {
            "to_int" => Ok(PrimOp::Ptr),
            "from_int" => Ok(PrimOp::Ptr),
            s => self.bad(s),
        }
    }

    fn float_un(&self) -> Result<crate::FUnOp, String> {
        use crate::FUnOp;
        Ok(match self.sub()? {
            "abs" => FUnOp::Abs,
            "sqrt" => FUnOp::Sqrt,
            "floor" => FUnOp::Floor,
            "ceil" => FUnOp::Ceil,
            "nearest" => FUnOp::Nearest,
            "neg" => FUnOp::Neg,
            s => return self.bad(s),
        })
    }

    fn float_bin(&self) -> Result<crate::FBinOp, String> {
        use crate::FBinOp;
        Ok(match self.sub()? {
            "add" => FBinOp::Add,
            "sub" => FBinOp::Sub,
            "mul" => FBinOp::Mul,
            "div" => FBinOp::Div,
            "min" => FBinOp::Min,
            "max" => FBinOp::Max,
            "copysign" => FBinOp::CopySign,
            s => return self.bad(s),
        })
    }

    fn float_cmp(&self) -> Result<crate::FCmpOp, String> {
        use crate::FCmpOp;
        Ok(match self.sub()? {
            "lt" => FCmpOp::Lt,
            "le" => FCmpOp::Le,
            "gt" => FCmpOp::Gt,
            "ge" => FCmpOp::Ge,
            "eq" => FCmpOp::Eq,
            "ne" => FCmpOp::Ne,
            s => return self.bad(s),
        })
    }
}

/// Every fn's name with one decode result per `@prim_op` it carries.
pub type PrimOpDeclarations = Vec<(String, Vec<Result<PrimOp, String>>)>;

/// Every fn declared in `source` with its `@prim_op` attributes decoded, in
/// source order: `(fn name, one result per @prim_op it carries)`.
pub fn declarations_in(source: &str) -> Result<PrimOpDeclarations, String> {
    let tokens = almide_lang::lexer::Lexer::tokenize(source);
    let mut parser = almide_lang::parser::Parser::new(tokens);
    let prog = parser.parse().map_err(|e| format!("prim.almd does not parse: {e:?}"))?;
    if !parser.errors.is_empty() {
        let messages: Vec<String> = parser.errors.iter().map(|d| d.display()).collect();
        return Err(format!("prim.almd does not parse: {}", messages.join("; ")));
    }
    Ok(prog
        .decls
        .iter()
        .filter_map(|d| match d {
            Decl::Fn { name, attrs, .. } => Some((
                name.as_str().to_string(),
                attrs.iter().filter(|a| a.name.as_str() == "prim_op").map(decode).collect(),
            )),
            _ => None,
        })
        .collect())
}

fn table() -> &'static Result<HashMap<String, PrimOp>, String> {
    static TABLE: OnceLock<Result<HashMap<String, PrimOp>, String>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let source = almide_lang::embedded::source_of("prim").ok_or("no embedded prim.almd")?;
        let mut out = HashMap::new();
        for (func, ops) in declarations_in(source)? {
            match &ops[..] {
                [Ok(op)] => {
                    out.insert(func, *op);
                }
                [Err(e)] => return Err(format!("prim.{func}: {e}")),
                _ => return Err(format!("prim.{func} carries {} @prim_op declarations, not one", ops.len())),
            }
        }
        Ok(out)
    })
}

/// The declared op of `prim.<func>`.
pub fn declared(func: &str) -> Result<PrimOp, String> {
    match table() {
        Ok(t) => t.get(func).copied().ok_or_else(|| format!("prim.{func} is not declared in prim.almd")),
        Err(e) => Err(e.clone()),
    }
}
