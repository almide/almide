//! The interpreter (REQ-VM-5..7). Every buffer an instance uses — linear
//! memory up to its cap, the operand/local stack, the frame stack — is
//! allocated when the instance is built; running allocates nothing. A call
//! checks the callee's whole stack need (locals + its validated maximum
//! operand height) before it enters, so no instruction inside a body can
//! overflow; every instruction spends one unit of fuel.

use std::collections::HashMap;

use crate::error::{LoadError, Trap};
use crate::ir::{widen, Call, Instr, LOADS, STORES};
use crate::module::{Module, MAX_PAGES, PAGE};
use crate::types::FuncType;
use crate::wasi::{self, Io, Stop};

/// The fixed bounds of one run.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Instructions the run may execute.
    pub fuel: u64,
    /// The most pages memory may ever hold (the declared maximum is also honoured).
    pub memory_pages: u32,
    /// 64-bit cells for every live frame's locals and operands together.
    pub stack_cells: u32,
    /// Frames that may be live at once, `_start`'s included.
    pub call_depth: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { fuel: u64::MAX, memory_pages: MAX_PAGES, stack_cells: 1 << 22, call_depth: 1 << 18 }
    }
}

/// How a run ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// `_start` returned.
    Finished,
    /// `proc_exit(code)`.
    Exit(i32),
    Trapped(Trap),
}

/// What a call needs to know about a function, imports included.
#[derive(Clone, Copy)]
struct Shape {
    params: u32,
    locals: u32,
    result: bool,
    max_height: u32,
}

/// A caller's saved registers.
#[derive(Clone, Copy)]
struct Frame {
    func: u32,
    pc: u32,
    fp: u32,
    ob: u32,
}

/// The running frame's registers: its function, the next instruction, the
/// frame pointer (its first local), the operand base and the stack top.
struct Regs {
    func: u32,
    pc: usize,
    fp: usize,
    ob: usize,
    sp: usize,
}

pub struct Instance<'m> {
    module: &'m Module,
    shapes: Vec<Shape>,
    /// Each type index mapped to the first structurally equal one, so an
    /// indirect call's signature check is one integer compare.
    canon: Vec<u32>,
    memory: Vec<u8>,
    pages: u32,
    max_pages: u32,
    globals: Vec<u64>,
    table: Vec<Option<u32>>,
    stack: Vec<u64>,
    frames: Vec<Frame>,
    depth_limit: usize,
    fuel: u64,
}

impl<'m> Instance<'m> {
    pub fn new(module: &'m Module, limits: Limits) -> Result<Self, LoadError> {
        let nimp = module.imports.len();
        let shapes = module
            .func_types
            .iter()
            .enumerate()
            .map(|(f, &t)| {
                let ty = &module.types[t as usize];
                let (locals, max_height) = match f.checked_sub(nimp) {
                    Some(d) => (module.bodies[d].locals, module.bodies[d].max_height),
                    None => (ty.params.len() as u32, 0),
                };
                Shape { params: ty.params.len() as u32, locals, result: !ty.results.is_empty(), max_height }
            })
            .collect();
        let mut first: HashMap<&FuncType, u32> = HashMap::new();
        let canon = module.types.iter().enumerate().map(|(i, t)| *first.entry(t).or_insert(i as u32)).collect();
        let (min, declared_max) = module.memory.unwrap_or((0, Some(0)));
        let max_pages = declared_max.unwrap_or(MAX_PAGES).min(limits.memory_pages);
        if min > max_pages {
            return Err(LoadError::whole(format!(
                "the initial memory ({min} pages) exceeds the memory limit ({max_pages} pages)"
            )));
        }
        let mut memory = vec![0u8; max_pages as usize * PAGE];
        for seg in &module.data {
            let at = seg.offset as usize;
            memory[at..at + seg.items.len()].copy_from_slice(&seg.items);
        }
        let mut table = vec![None; module.table.unwrap_or(0) as usize];
        for seg in &module.elements {
            for (k, &f) in seg.items.iter().enumerate() {
                table[seg.offset as usize + k] = Some(f);
            }
        }
        Ok(Instance {
            module,
            shapes,
            canon,
            memory,
            pages: min,
            max_pages,
            globals: module.globals.iter().map(|g| g.init).collect(),
            table,
            stack: vec![0u64; limits.stack_cells as usize],
            frames: Vec::with_capacity(limits.call_depth as usize),
            depth_limit: limits.call_depth as usize,
            fuel: limits.fuel,
        })
    }

    /// Run `_start` to completion, an exit, or a trap.
    pub fn run(&mut self, io: &mut Io) -> Outcome {
        match self.execute(io) {
            Ok(()) => Outcome::Finished,
            Err(Stop::Exit(code)) => Outcome::Exit(code),
            Err(Stop::Trap(t)) => Outcome::Trapped(t),
        }
    }

    /// Make room for a defined function whose arguments are the top `params`
    /// cells: zero its declared locals and check its whole stack need.
    /// Returns (frame pointer, operand base).
    fn enter(&mut self, f: u32, sp: usize) -> Result<(usize, usize), Trap> {
        let s = self.shapes[f as usize];
        let fp = sp - s.params as usize;
        let ob = fp + s.locals as usize;
        if ob + s.max_height as usize > self.stack.len() {
            return Err(Trap::CallStackExhausted);
        }
        self.stack[sp..ob].fill(0);
        Ok((fp, ob))
    }

    /// The function an indirect call reaches, with its signature checked.
    fn resolve(&self, index: u64, ty: u32) -> Result<u32, Trap> {
        let slot = *self.table.get(index as u32 as usize).ok_or(Trap::IndirectCallOutOfTable)?;
        let f = slot.ok_or(Trap::IndirectCallToNull)?;
        let callee = self.module.func_types[f as usize];
        if self.canon[callee as usize] != self.canon[ty as usize] {
            return Err(Trap::IndirectCallTypeMismatch);
        }
        Ok(f)
    }

    /// A host call whose arguments are the top cells; returns the new `sp`.
    fn host(&mut self, f: u32, sp: usize, io: &mut Io) -> Result<usize, Stop> {
        let s = self.shapes[f as usize];
        let base = sp - s.params as usize;
        let size = self.pages as usize * PAGE;
        let result = wasi::call(self.module.imports[f as usize], &self.stack[base..sp], &mut self.memory[..size], io)?;
        let mut sp = base;
        if let Some(v) = result {
            self.stack[sp] = v;
            sp += 1;
        }
        Ok(sp)
    }

    fn memory_range(&self, addr: u64, offset: u32, len: u64) -> Result<usize, Trap> {
        let start = (addr as u32 as u64) + u64::from(offset);
        if start + len > self.pages as u64 * PAGE as u64 {
            return Err(Trap::MemoryOutOfBounds);
        }
        Ok(start as usize)
    }

    fn execute(&mut self, io: &mut Io) -> Result<(), Stop> {
        let m = self.module;
        let nimp = m.imports.len() as u32;
        if m.entry < nimp {
            self.host(m.entry, 0, io)?;
            return Ok(());
        }
        // The running frame's registers live in locals, so the hot loop keeps
        // them in machine registers; a transfer packs them into `Regs`.
        let (mut fp, mut ob) = self.enter(m.entry, 0)?;
        let (mut func, mut pc, mut sp) = (m.entry, 0usize, ob);
        let mut code: &[Instr] = &m.bodies[(func - nimp) as usize].code;
        loop {
            if self.fuel == 0 {
                return Err(Trap::OutOfFuel.into());
            }
            self.fuel -= 1;
            let instr = code[pc];
            pc += 1;
            // ONE match over the flat form (see `Instr`): the dispatch is the
            // interpreter's cost, so it stays a single branch per instruction.
            match instr {
                Instr::LocalGet(i) => {
                    self.stack[sp] = self.stack[fp + i as usize];
                    sp += 1;
                }
                Instr::LocalSet(i) => {
                    sp -= 1;
                    self.stack[fp + i as usize] = self.stack[sp];
                }
                Instr::LocalTee(i) => self.stack[fp + i as usize] = self.stack[sp - 1],
                Instr::Const(v) => {
                    self.stack[sp] = v;
                    sp += 1;
                }
                Instr::Binary(f) => {
                    self.stack[sp - 2] = f(self.stack[sp - 2], self.stack[sp - 1])?;
                    sp -= 1;
                }
                Instr::Unary(f) => self.stack[sp - 1] = f(self.stack[sp - 1]),
                Instr::Br { target, height, keep } => {
                    sp = cut(&mut self.stack, sp, ob + height as usize, keep);
                    pc = target as usize;
                }
                Instr::BrIf { target, height, keep } => {
                    sp -= 1;
                    if self.stack[sp] as u32 != 0 {
                        sp = cut(&mut self.stack, sp, ob + height as usize, keep);
                        pc = target as usize;
                    }
                }
                Instr::BrUnless { target } => {
                    sp -= 1;
                    if self.stack[sp] as u32 == 0 {
                        pc = target as usize;
                    }
                }
                Instr::Load { op, offset } => self.load(op, offset, sp)?,
                Instr::Store { op, offset } => {
                    sp -= 2;
                    self.store(op, offset, sp)?;
                }
                Instr::GlobalGet(i) => {
                    self.stack[sp] = self.globals[i as usize];
                    sp += 1;
                }
                Instr::GlobalSet(i) => {
                    sp -= 1;
                    self.globals[i as usize] = self.stack[sp];
                }
                Instr::Drop => sp -= 1,
                Instr::Select => {
                    if self.stack[sp - 1] as u32 == 0 {
                        self.stack[sp - 3] = self.stack[sp - 2];
                    }
                    sp -= 2;
                }
                Instr::Call(op) => {
                    let mut r = Regs { func, pc, fp, ob, sp };
                    if !self.transfer(op, &mut r, io)? {
                        return Ok(());
                    }
                    Regs { func, pc, fp, ob, sp } = r;
                    code = &m.bodies[(func - nimp) as usize].code;
                }
                Instr::MemorySize => {
                    self.stack[sp] = u64::from(self.pages);
                    sp += 1;
                }
                Instr::MemoryGrow => self.stack[sp - 1] = self.grow(self.stack[sp - 1] as u32),
                Instr::MemoryCopy => {
                    sp -= 3;
                    self.copy(sp)?;
                }
                Instr::MemoryFill => {
                    sp -= 3;
                    self.fill(sp)?;
                }
                Instr::Unreachable => return Err(Trap::Unreachable.into()),
            }
        }
    }

    /// `load`: the address is the top cell, which the value replaces.
    #[inline(always)]
    fn load(&mut self, op: u8, offset: u32, sp: usize) -> Result<(), Trap> {
        let o = &LOADS[op as usize];
        let n = usize::from(o.bytes);
        let at = self.memory_range(self.stack[sp - 1], offset, n as u64)?;
        let mut raw = [0u8; 8];
        raw[..n].copy_from_slice(&self.memory[at..at + n]);
        self.stack[sp - 1] = widen(o, u64::from_le_bytes(raw));
        Ok(())
    }

    /// `store`: the address and the value are the two cells at `sp`, already popped.
    #[inline(always)]
    fn store(&mut self, op: u8, offset: u32, sp: usize) -> Result<(), Trap> {
        let n = usize::from(STORES[op as usize].bytes);
        let (addr, value) = (self.stack[sp], self.stack[sp + 1]);
        let at = self.memory_range(addr, offset, n as u64)?;
        self.memory[at..at + n].copy_from_slice(&value.to_le_bytes()[..n]);
        Ok(())
    }

    /// `memory.copy`: (dst, src, n) are the three cells at `sp`, already popped.
    fn copy(&mut self, sp: usize) -> Result<(), Trap> {
        let (dst, src, n) = (self.stack[sp], self.stack[sp + 1], self.stack[sp + 2] as u32);
        let d = self.memory_range(dst, 0, u64::from(n))?;
        let s = self.memory_range(src, 0, u64::from(n))?;
        self.memory.copy_within(s..s + n as usize, d);
        Ok(())
    }

    /// `memory.fill`: (dst, value, n) are the three cells at `sp`, already popped.
    fn fill(&mut self, sp: usize) -> Result<(), Trap> {
        let (dst, value, n) = (self.stack[sp], self.stack[sp + 1] as u8, self.stack[sp + 2] as u32);
        let d = self.memory_range(dst, 0, u64::from(n))?;
        self.memory[d..d + n as usize].fill(value);
        Ok(())
    }

    /// A call, a return or a tail call. `Ok(false)` when the entry frame has
    /// returned and the run is over; the caller reloads the code after any
    /// transfer.
    fn transfer(&mut self, op: Call, r: &mut Regs, io: &mut Io) -> Result<bool, Stop> {
        match op {
            Call::Return => {
                r.sp = self.leave(r.func, r.sp, r.fp);
                Ok(self.resume_caller(r))
            }
            Call::Call(f) => self.call(f, r, io).map(|()| true),
            Call::CallIndirect(ty) => {
                let f = self.pop_callee(ty, r)?;
                self.call(f, r, io).map(|()| true)
            }
            Call::ReturnCall(f) => self.tail_call(f, r, io),
            Call::ReturnCallIndirect(ty) => {
                let f = self.pop_callee(ty, r)?;
                self.tail_call(f, r, io)
            }
        }
    }

    fn pop_callee(&mut self, ty: u32, r: &mut Regs) -> Result<u32, Trap> {
        r.sp -= 1;
        self.resolve(self.stack[r.sp], ty)
    }

    /// Pop the caller's frame into the registers; `false` when there is none.
    fn resume_caller(&mut self, r: &mut Regs) -> bool {
        match self.frames.pop() {
            None => false,
            Some(c) => {
                (r.func, r.pc, r.fp, r.ob) = (c.func, c.pc as usize, c.fp as usize, c.ob as usize);
                true
            }
        }
    }

    fn call(&mut self, callee: u32, r: &mut Regs, io: &mut Io) -> Result<(), Stop> {
        if (callee as usize) < self.module.imports.len() {
            r.sp = self.host(callee, r.sp, io)?;
            return Ok(());
        }
        if self.frames.len() + 1 >= self.depth_limit {
            return Err(Trap::CallStackExhausted.into());
        }
        self.frames.push(Frame { func: r.func, pc: r.pc as u32, fp: r.fp as u32, ob: r.ob as u32 });
        (r.fp, r.ob) = self.enter(callee, r.sp)?;
        (r.func, r.pc, r.sp) = (callee, 0, r.ob);
        Ok(())
    }

    /// The callee's arguments take the place of this frame, and the callee
    /// runs in it. A host callee returns straight to this frame's caller.
    fn tail_call(&mut self, callee: u32, r: &mut Regs, io: &mut Io) -> Result<bool, Stop> {
        let n = self.shapes[callee as usize].params as usize;
        self.stack.copy_within(r.sp - n..r.sp, r.fp);
        r.sp = r.fp + n;
        if (callee as usize) < self.module.imports.len() {
            // the host's result lands at the frame pointer, which is where
            // this frame returns it
            r.sp = self.host(callee, r.sp, io)?;
            return Ok(self.resume_caller(r));
        }
        (r.fp, r.ob) = self.enter(callee, r.sp)?;
        (r.func, r.pc, r.sp) = (callee, 0, r.ob);
        Ok(true)
    }

    /// `memory.grow`: the old size in pages, or -1 past the cap. The bytes
    /// it adds are zero already — nothing writes past the current size.
    fn grow(&mut self, delta: u32) -> u64 {
        let grown = u64::from(self.pages) + u64::from(delta);
        if grown > u64::from(self.max_pages) {
            return u64::from(u32::MAX);
        }
        let old = self.pages;
        self.pages = grown as u32;
        u64::from(old)
    }

    /// Leave the current frame: its result (if any) moves to the frame
    /// pointer, which becomes the caller's new stack top.
    fn leave(&mut self, func: u32, sp: usize, fp: usize) -> usize {
        if self.shapes[func as usize].result {
            self.stack[fp] = self.stack[sp - 1];
            fp + 1
        } else {
            fp
        }
    }
}

/// Cut the operand stack back to `to`, keeping the top value when `keep`.
#[inline(always)]
fn cut(stack: &mut [u64], sp: usize, to: usize, keep: bool) -> usize {
    if keep {
        stack[to] = stack[sp - 1];
        to + 1
    } else {
        to
    }
}
