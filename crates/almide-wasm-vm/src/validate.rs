//! One pass over a function body that validates it (the standard wasm
//! validation algorithm, restricted to the closed set) and translates it into
//! the flat IR (REQ-VM-3). An opcode outside the set, a multi-value block, an
//! f32 block type, an ill-typed operand or a body with values left over is a
//! `LoadError`: nothing this VM cannot justify reaches the interpreter.

use crate::error::LoadError;
use crate::ir::{Call, Instr, LOADS, STORES};
use crate::numeric::{self, Semantics};
use crate::reader::Reader;
use crate::types::{FuncType, ValType};

/// A function body as loaded.
pub struct Body {
    /// How many locals the frame holds: params first, then the declared ones.
    pub locals: u32,
    pub params: u32,
    pub code: Vec<Instr>,
    /// The highest operand-stack height any point of the body reaches.
    pub max_height: u32,
}

/// What a body may refer to.
pub struct Context<'m> {
    pub types: &'m [FuncType],
    /// The type index of every function in the index space (imports first).
    pub func_types: &'m [u32],
    /// (type, mutable) of every global.
    pub globals: &'m [(ValType, bool)],
    pub has_table: bool,
    pub has_memory: bool,
}

/// A body that declares more locals than this is refused. The emitter walls a
/// function past 50,000 locals itself, so nothing it produces comes close.
pub const MAX_LOCALS: usize = 50_000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Func,
    Block,
    Loop,
    If,
    Else,
}

struct Ctrl {
    kind: Kind,
    result: Option<ValType>,
    height: usize,
    unreachable: bool,
    /// Where a branch to a loop label lands.
    start: u32,
    /// Branches (and the else arm's jump) waiting for this block's end.
    pending: Vec<usize>,
    /// The `BrUnless` an `if` emitted, until its else or end is known.
    if_jump: Option<usize>,
}

type R<T> = Result<T, LoadError>;

pub fn translate(ctx: &Context, ty: &FuncType, body: &mut Reader) -> R<Body> {
    let locals = Locals::read(ty, body)?;
    let count = locals.count();
    let mut tr = Tr {
        ctx,
        locals,
        result: ty.results.first().copied(),
        stack: Vec::new(),
        ctrls: Vec::new(),
        code: Vec::new(),
        max: 0,
    };
    tr.ctrls.push(Ctrl::new(Kind::Func, tr.result, 0, 0));
    while !tr.ctrls.is_empty() {
        let op = body.byte()?;
        tr.op(op, body)?;
    }
    if !body.at_end() {
        return body.fail("bytes follow the function's final `end`");
    }
    Ok(Body { params: ty.params.len() as u32, locals: count, code: tr.code, max_height: tr.max as u32 })
}

/// A body's local types as runs — `(end, type)`, `end` exclusive — so a
/// declared count costs one entry however large it is, and the memory a
/// module takes to load stays proportional to its bytes.
struct Locals(Vec<(u32, ValType)>);

impl Locals {
    fn read(ty: &FuncType, body: &mut Reader) -> R<Locals> {
        let mut runs: Vec<(u32, ValType)> = Vec::new();
        let mut end = 0u32;
        for &p in &ty.params {
            end += 1;
            runs.push((end, p));
        }
        for _ in 0..body.vec_len()? {
            let count = body.u32()?;
            let at = body.offset();
            let t = ValType::declared(body.byte()?).ok_or_else(|| LoadError::new(at, "a local's type is outside i32/i64/f64"))?;
            end = match end.checked_add(count) {
                Some(e) if e as usize <= MAX_LOCALS => e,
                _ => return body.fail(format!("a body declares more than {MAX_LOCALS} locals")),
            };
            if count > 0 {
                runs.push((end, t));
            }
        }
        Ok(Locals(runs))
    }

    fn count(&self) -> u32 {
        self.0.last().map_or(0, |&(end, _)| end)
    }

    fn get(&self, index: u32) -> Option<ValType> {
        let i = self.0.partition_point(|&(end, _)| end <= index);
        self.0.get(i).map(|&(_, t)| t)
    }
}

impl Ctrl {
    fn new(kind: Kind, result: Option<ValType>, height: usize, start: u32) -> Self {
        Ctrl { kind, result, height, unreachable: false, start, pending: Vec::new(), if_jump: None }
    }

    /// The value a branch to this label carries: a loop label takes none.
    fn label_value(&self) -> Option<ValType> {
        if self.kind == Kind::Loop { None } else { self.result }
    }
}

struct Tr<'c, 'm> {
    ctx: &'c Context<'m>,
    locals: Locals,
    result: Option<ValType>,
    /// `None` is a value of unknown type (after an unconditional branch).
    stack: Vec<Option<ValType>>,
    ctrls: Vec<Ctrl>,
    code: Vec<Instr>,
    max: usize,
}

impl Tr<'_, '_> {
    fn top(&self) -> &Ctrl {
        self.ctrls.last().expect("the function frame is the bottom control")
    }

    fn push(&mut self, t: Option<ValType>) {
        self.stack.push(t);
        self.max = self.max.max(self.stack.len());
    }

    fn pop(&mut self, r: &Reader) -> R<Option<ValType>> {
        let c = self.top();
        if self.stack.len() == c.height {
            return if c.unreachable { Ok(None) } else { r.fail("an instruction pops an empty operand stack") };
        }
        Ok(self.stack.pop().flatten())
    }

    fn pop_expect(&mut self, want: ValType, r: &Reader) -> R<()> {
        match self.pop(r)? {
            Some(got) if got != want => r.fail(format!("an operand is {got:?} where {want:?} is required")),
            _ => Ok(()),
        }
    }

    fn pop_params(&mut self, ty: &FuncType, r: &Reader) -> R<()> {
        for &p in ty.params.iter().rev() {
            self.pop_expect(p, r)?;
        }
        Ok(())
    }

    fn set_unreachable(&mut self) {
        let height = self.top().height;
        self.stack.truncate(height);
        self.ctrls.last_mut().expect("a control frame").unreachable = true;
    }

    fn emit(&mut self, i: Instr) -> usize {
        self.code.push(i);
        self.code.len() - 1
    }

    fn here(&self) -> u32 {
        self.code.len() as u32
    }

    fn label(&self, depth: u32, r: &Reader) -> R<usize> {
        let depth = depth as usize;
        if depth >= self.ctrls.len() {
            return r.fail("a branch names a label that does not enclose it");
        }
        Ok(self.ctrls.len() - 1 - depth)
    }

    /// Emit a branch to the label `depth` levels out, patched later when it
    /// jumps forward.
    fn branch(&mut self, depth: u32, conditional: bool, r: &Reader) -> R<()> {
        let at = self.label(depth, r)?;
        let (kind, start, height, value) = {
            let c = &self.ctrls[at];
            (c.kind, c.start, c.height as u32, c.label_value())
        };
        let (target, keep) = (if kind == Kind::Loop { start } else { 0 }, value.is_some());
        let instr = if conditional { Instr::BrIf { target, height, keep } } else { Instr::Br { target, height, keep } };
        let idx = self.emit(instr);
        if kind != Kind::Loop {
            self.ctrls[at].pending.push(idx);
        }
        Ok(())
    }

    /// A block type: `0x40` (empty) or one value-type byte. A type-index
    /// block type (multi-value) and the f32 byte are outside the set.
    fn block_type(&mut self, r: &mut Reader) -> R<Option<ValType>> {
        let at = r.offset();
        match r.byte()? {
            0x40 => Ok(None),
            b => ValType::declared(b).map(Some).ok_or_else(|| {
                LoadError::new(at, "a block type outside empty/i32/i64/f64 (multi-value or f32) is not accepted")
            }),
        }
    }

    /// The values a block leaves must be exactly its result.
    fn check_block_end(&mut self, r: &Reader) -> R<()> {
        if let Some(t) = self.top().result {
            self.pop_expect(t, r)?;
        }
        if self.stack.len() != self.top().height {
            return r.fail("values remain on the operand stack at the end of a block");
        }
        Ok(())
    }

    fn patch(&mut self, at: usize, to: u32) {
        match &mut self.code[at] {
            Instr::Br { target, .. } | Instr::BrIf { target, .. } | Instr::BrUnless { target } => *target = to,
            _ => unreachable!("only branches are patched"),
        }
    }

    fn op(&mut self, op: u8, r: &mut Reader) -> R<()> {
        match op {
            0x00..=0x13 => self.control(op, r),
            0x1A => {
                self.pop(r)?;
                self.emit(Instr::Drop);
                Ok(())
            }
            0x1B => self.select(r),
            0x20..=0x24 => self.variable(op, r),
            0x3F | 0x40 => self.memory_size_or_grow(op, r),
            0x41 | 0x42 | 0x44 => self.constant(op, r),
            0xFC => self.prefixed(r),
            _ => self.memory_or_numeric(op, r),
        }
    }

    fn control(&mut self, op: u8, r: &mut Reader) -> R<()> {
        match op {
            0x00 => {
                self.emit(Instr::Unreachable);
                self.set_unreachable();
            }
            0x02 | 0x03 => {
                let bt = self.block_type(r)?;
                let kind = if op == 0x02 { Kind::Block } else { Kind::Loop };
                let (height, start) = (self.stack.len(), self.here());
                self.ctrls.push(Ctrl::new(kind, bt, height, start));
            }
            0x04 => {
                let bt = self.block_type(r)?;
                self.pop_expect(ValType::I32, r)?;
                let jump = self.emit(Instr::BrUnless { target: 0 });
                let mut c = Ctrl::new(Kind::If, bt, self.stack.len(), 0);
                c.if_jump = Some(jump);
                self.ctrls.push(c);
            }
            0x05 => self.else_arm(r)?,
            0x0B => self.end(r)?,
            0x0C => {
                let depth = r.u32()?;
                self.branch_value(depth, r)?;
                self.branch(depth, false, r)?;
                self.set_unreachable();
            }
            0x0D => self.br_if(r)?,
            0x0F => {
                if let Some(t) = self.result {
                    self.pop_expect(t, r)?;
                }
                self.emit(Instr::Call(Call::Return));
                self.set_unreachable();
            }
            0x10..=0x13 => self.call(op, r)?,
            _ => return r.fail(format!("opcode {op:#x} is outside the instruction set this VM accepts")),
        }
        Ok(())
    }

    fn br_if(&mut self, r: &mut Reader) -> R<()> {
        let depth = r.u32()?;
        self.pop_expect(ValType::I32, r)?;
        let value = self.ctrls[self.label(depth, r)?].label_value();
        if let Some(t) = value {
            self.pop_expect(t, r)?;
            self.push(Some(t));
        }
        self.branch(depth, true, r)
    }

    fn select(&mut self, r: &Reader) -> R<()> {
        self.pop_expect(ValType::I32, r)?;
        let (a, b) = (self.pop(r)?, self.pop(r)?);
        if let (Some(a), Some(b)) = (a, b)
            && a != b
        {
            return r.fail("select's operands differ in type");
        }
        self.push(a.or(b));
        self.emit(Instr::Select);
        Ok(())
    }

    fn memory_size_or_grow(&mut self, op: u8, r: &mut Reader) -> R<()> {
        self.zero_index(r)?;
        self.need_memory(r)?;
        if op == 0x40 {
            self.pop_expect(ValType::I32, r)?;
        }
        self.push(Some(ValType::I32));
        self.emit(if op == 0x3F { Instr::MemorySize } else { Instr::MemoryGrow });
        Ok(())
    }

    fn constant(&mut self, op: u8, r: &mut Reader) -> R<()> {
        let (t, bits) = match op {
            0x41 => (ValType::I32, u64::from(r.i32()? as u32)),
            0x42 => (ValType::I64, r.i64()? as u64),
            _ => (ValType::F64, r.f64_bits()?),
        };
        self.push(Some(t));
        self.emit(Instr::Const(bits));
        Ok(())
    }

    fn branch_value(&mut self, depth: u32, r: &Reader) -> R<()> {
        let value = self.ctrls[self.label(depth, r)?].label_value();
        if let Some(t) = value {
            self.pop_expect(t, r)?;
        }
        Ok(())
    }

    fn else_arm(&mut self, r: &Reader) -> R<()> {
        if self.top().kind != Kind::If {
            return r.fail("`else` without an enclosing `if`");
        }
        self.check_block_end(r)?;
        let (height, keep) = (self.top().height as u32, self.top().result.is_some());
        let jump = self.emit(Instr::Br { target: 0, height, keep });
        let here = self.here();
        let c = self.ctrls.last_mut().expect("the if frame");
        c.pending.push(jump);
        let if_jump = c.if_jump.take().expect("an if frame holds its jump");
        c.kind = Kind::Else;
        c.unreachable = false;
        self.patch(if_jump, here);
        self.stack.truncate(height as usize);
        Ok(())
    }

    fn end(&mut self, r: &Reader) -> R<()> {
        self.check_block_end(r)?;
        if self.top().kind == Kind::If && self.top().result.is_some() {
            return r.fail("an `if` without `else` cannot produce a value");
        }
        if self.top().kind == Kind::Func {
            // a branch to the function's own label returns
            let ret = self.here();
            let c = self.ctrls.pop().expect("the function frame");
            for at in c.pending {
                self.patch(at, ret);
            }
            if let Some(t) = c.result {
                self.push(Some(t));
            }
            self.emit(Instr::Call(Call::Return));
            return Ok(());
        }
        let here = self.here();
        let c = self.ctrls.pop().expect("a block frame");
        if let Some(at) = c.if_jump {
            self.patch(at, here);
        }
        for at in c.pending {
            self.patch(at, here);
        }
        if let Some(t) = c.result {
            self.push(Some(t));
        }
        Ok(())
    }

    fn func_type(&self, index: u32, r: &Reader) -> R<FuncType> {
        let ty = self.ctx.func_types.get(index as usize).and_then(|&t| self.ctx.types.get(t as usize));
        match ty {
            Some(t) => Ok(t.clone()),
            None => r.fail("a call names a function that does not exist"),
        }
    }

    fn call(&mut self, op: u8, r: &mut Reader) -> R<()> {
        let indirect = op == 0x11 || op == 0x13;
        let tail = op == 0x12 || op == 0x13;
        let index = r.u32()?;
        let ty = if indirect {
            self.zero_index(r)?; // table index 0
            if !self.ctx.has_table {
                return r.fail("call_indirect in a module without a table");
            }
            self.pop_expect(ValType::I32, r)?;
            match self.ctx.types.get(index as usize) {
                Some(t) => t.clone(),
                None => return r.fail("call_indirect names a type that does not exist"),
            }
        } else {
            self.func_type(index, r)?
        };
        self.pop_params(&ty, r)?;
        if tail {
            if ty.results.first().copied() != self.result {
                return r.fail("a tail call's result differs from the caller's");
            }
            self.emit(Instr::Call(if indirect { Call::ReturnCallIndirect(index) } else { Call::ReturnCall(index) }));
            self.set_unreachable();
            return Ok(());
        }
        if let Some(&t) = ty.results.first() {
            self.push(Some(t));
        }
        self.emit(Instr::Call(if indirect { Call::CallIndirect(index) } else { Call::Call(index) }));
        Ok(())
    }

    fn variable(&mut self, op: u8, r: &mut Reader) -> R<()> {
        let index = r.u32()?;
        if op <= 0x22 {
            let Some(t) = self.locals.get(index) else {
                return r.fail("a local index is out of range");
            };
            match op {
                0x20 => self.push(Some(t)),
                0x21 => self.pop_expect(t, r)?,
                _ => {
                    self.pop_expect(t, r)?;
                    self.push(Some(t));
                }
            }
            self.emit(match op {
                0x20 => Instr::LocalGet(index),
                0x21 => Instr::LocalSet(index),
                _ => Instr::LocalTee(index),
            });
            return Ok(());
        }
        let (t, mutable) = match self.ctx.globals.get(index as usize) {
            Some(&g) => g,
            None => return r.fail("a global index is out of range"),
        };
        if op == 0x23 {
            self.push(Some(t));
            self.emit(Instr::GlobalGet(index));
        } else {
            if !mutable {
                return r.fail("global.set on an immutable global");
            }
            self.pop_expect(t, r)?;
            self.emit(Instr::GlobalSet(index));
        }
        Ok(())
    }

    fn memory_or_numeric(&mut self, op: u8, r: &mut Reader) -> R<()> {
        if let Some(i) = LOADS.iter().position(|l| l.code == op) {
            let offset = self.memarg(LOADS[i].bytes, r)?;
            self.pop_expect(ValType::I32, r)?;
            self.push(Some(LOADS[i].result));
            self.emit(Instr::Load { op: i as u8, offset });
            return Ok(());
        }
        if let Some(i) = STORES.iter().position(|s| s.code == op) {
            let offset = self.memarg(STORES[i].bytes, r)?;
            self.pop_expect(STORES[i].value, r)?;
            self.pop_expect(ValType::I32, r)?;
            self.emit(Instr::Store { op: i as u8, offset });
            return Ok(());
        }
        self.numeric(u16::from(op), r)
    }

    fn numeric(&mut self, code: u16, r: &Reader) -> R<()> {
        let Some(op) = numeric::lookup(code) else {
            return r.fail(format!("opcode {code:#x} is outside the instruction set this VM accepts"));
        };
        match op.semantics {
            Semantics::Unary(f) => {
                self.pop_expect(op.operand, r)?;
                self.emit(Instr::Unary(f));
            }
            Semantics::Binary(f) => {
                self.pop_expect(op.operand, r)?;
                self.pop_expect(op.operand, r)?;
                self.emit(Instr::Binary(f));
            }
        }
        self.push(Some(op.result));
        Ok(())
    }

    fn prefixed(&mut self, r: &mut Reader) -> R<()> {
        let sub = r.u32()?;
        match sub {
            10 | 11 => {
                self.zero_index(r)?;
                if sub == 10 {
                    self.zero_index(r)?;
                }
                self.need_memory(r)?;
                for _ in 0..3 {
                    self.pop_expect(ValType::I32, r)?;
                }
                self.emit(if sub == 10 { Instr::MemoryCopy } else { Instr::MemoryFill });
                Ok(())
            }
            _ if sub <= 0xFF => self.numeric(0xFC00 | sub as u16, r),
            _ => r.fail(format!("opcode 0xfc {sub} is outside the instruction set this VM accepts")),
        }
    }

    fn memarg(&mut self, bytes: u8, r: &mut Reader) -> R<u32> {
        let align = r.u32()?;
        let offset = r.u32()?;
        self.need_memory(r)?;
        if align > bytes.trailing_zeros() {
            return r.fail("a memory access claims more alignment than its width");
        }
        Ok(offset)
    }

    fn need_memory(&self, r: &Reader) -> R<()> {
        if self.ctx.has_memory { Ok(()) } else { r.fail("a memory instruction in a module without a memory") }
    }

    /// A memory or table index that must be 0 (one memory, one table),
    /// in any valid LEB128 spelling of 0.
    fn zero_index(&mut self, r: &mut Reader) -> R<()> {
        if r.u32()? != 0 { r.fail("a memory or table index is not 0") } else { Ok(()) }
    }
}
