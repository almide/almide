//! #4250 / #4252: the walks over a value of a RECURSIVE type — its drop, its
//! `==` and its display — take a bounded number of wasm frames whatever the
//! value's depth.
//!
//! Each walk recurses as before while the value is shallow, and counts its
//! nesting in a global. Past `DEEP_LIMIT` levels it stops recursing:
//!
//! - drop and `==` queue the nested work (the block and its release fn's
//!   table slot; the operand pair and the comparison's slot) on a growable
//!   list held in a global, and the OUTERMOST walk drains the queue in a loop
//!   — `==` is the conjunction of its leaves, and the order a value's blocks
//!   are freed in is unobservable;
//! - display has an order, so only a nested value written LAST in its parent
//!   — followed by nothing but fixed text, the `)` of `Cons(1, …)`, the ` }`
//!   of a record — is continued without a frame: the parent queues that text
//!   and tail-calls the nested value's display, and the frame that made the
//!   last ordinary call writes the queued text back, innermost first.
//!
//! A shallow value never reaches the limit, so it allocates nothing and runs
//! the same instructions as before. The globals exist only in a module whose
//! helpers use them.

use std::collections::HashSet;

use wasm_encoder::{BlockType, ConstExpr, Function, GlobalSection, GlobalType, InstructionSink, MemArg, ValType};

use crate::types_table::{NamedDef, TypeTable};
use crate::work::FnWork;
use crate::*;

/// Nesting levels a walk recurses through before it queues. Each level is a
/// few small frames, far inside the 8 MiB embedded stack and a stock
/// runtime's 512 KiB default.
pub(crate) const DEEP_LIMIT: i32 = 1024;

pub(crate) const DROP_Q: u32 = 0;
pub(crate) const DROP_DEPTH: u32 = 1;
pub(crate) const EQ_Q: u32 = 2;
pub(crate) const EQ_DEPTH: u32 = 3;
pub(crate) const DISP_Q: u32 = 4;
pub(crate) const DISP_DEPTH: u32 = 5;
pub(crate) const DISP_INS_Q: u32 = 6;
const COUNT: u32 = 7;

/// The index of one deep-walk global; marks the module as declaring them.
pub(crate) fn global(work: &FnWork, which: u32) -> u32 {
    work.deep_globals_used.set(true);
    work.deep_global_base.get() + which
}

/// Declare the deep-walk globals (i32, mutable, 0) when any helper uses them.
pub(crate) fn declare_globals(globals: &mut GlobalSection, work: &FnWork) {
    if !work.deep_globals_used.get() {
        return;
    }
    for _ in 0..COUNT {
        globals.global(GlobalType { val_type: ValType::I32, mutable: true, shared: false }, &ConstExpr::i32_const(0));
    }
}

impl TypeTable {
    /// Is named type `ti` on a cycle of the type graph — can a value of it
    /// hold another value of it, through any container?
    pub(crate) fn named_on_cycle(&self, ti: u32) -> bool {
        let mut seen: HashSet<u32> = HashSet::new();
        let mut stack: Vec<SliceTy> = self.named_children(ti);
        while let Some(t) = stack.pop() {
            match t {
                SliceTy::Named(tj) => {
                    if tj == ti {
                        return true;
                    }
                    if seen.insert(tj) {
                        stack.extend(self.named_children(tj));
                    }
                }
                SliceTy::Option(h) | SliceTy::List(h) | SliceTy::Set(h) => stack.push(self.el(h)),
                SliceTy::Result(a, b) | SliceTy::Map(a, b) => {
                    stack.push(self.el(a));
                    stack.push(self.el(b));
                }
                SliceTy::Tuple(id) => stack.extend(self.tuple_def(id).fields.iter().map(|&(t, _)| t)),
                _ => {}
            }
        }
        false
    }

    fn named_children(&self, ti: u32) -> Vec<SliceTy> {
        match self.def(ti) {
            NamedDef::Record(r) => r.fields.iter().map(|f| f.ty).collect(),
            NamedDef::Variant(v) => v.cases.iter().flat_map(|c| c.fields.iter().map(|f| f.ty)).collect(),
            NamedDef::Excluded => Vec::new(),
        }
    }
}

/// The first deep-walk global: they follow the fixed globals, the
/// `top_lets` top-let globals, the alloc counters and the replay ordinal
/// (assembly.rs declares them in that order).
pub(crate) fn global_base(top_lets: usize) -> u32 {
    G_FIXED_COUNT
        + top_lets as u32
        + if crate::alloc_count::armed() { crate::alloc_count::EXPORTS.len() as u32 } else { 0 }
        + u32::from(crate::fuel::omega_replay() >= 0)
}

fn word(offset: u32) -> MemArg {
    MemArg { offset: u64::from(offset), align: 2, memory_index: 0 }
}

/// Push the i32 on top of the stack onto the queue in global `gq`
/// (allocated on first use). `q` and `v` are i32 scratch locals.
pub(crate) fn emit_push(i: &mut InstructionSink<'_>, gq: u32, q: u32, v: u32) {
    i.local_set(v);
    i.global_get(gq).local_tee(q).i32_eqz().if_(BlockType::Empty);
    i.i32_const(0).call(F_ALLOC).local_set(q);
    i.end();
    i.local_get(q).local_get(v).call(F_LIST_PUSH_4).global_set(gq);
}

/// Pop `n` i32 words off the queue in global `gq` (known non-empty, holding
/// at least `n`), leaving them on the stack in push order. `q` / `len` are
/// i32 scratch locals.
pub(crate) fn emit_pop(i: &mut InstructionSink<'_>, gq: u32, n: u32, q: u32, len: u32) {
    i.global_get(gq).local_tee(q).i32_load(len_memarg()).i32_const((4 * n) as i32).i32_sub().local_set(len);
    i.local_get(q).local_get(len).i32_store(len_memarg());
    for k in 0..n {
        i.local_get(q).local_get(len).i32_add().i32_load(word(almide_layout::PAYLOAD + 4 * k));
    }
}

/// The queue's live length in bytes (0 when it was never allocated).
pub(crate) fn emit_queue_len(i: &mut InstructionSink<'_>, gq: u32) {
    i.global_get(gq).i32_eqz().if_(BlockType::Result(ValType::I32));
    i.i32_const(0);
    i.else_();
    i.global_get(gq).i32_load(len_memarg());
    i.end();
}

/// One handle slot of a deep drop: its payload offset, its release fn, and
/// that fn's funcref-table slot (0 = a flat release, always called — it
/// frees one block and recurses into nothing).
pub(crate) type DeepSlot = (u32, u32, u32);

/// `$drop_<shape>(block)` of a type on a cycle (#4252): `emit_drop_shape`'s
/// credit arithmetic and releases, with the releases queued past
/// `DEEP_LIMIT` nesting levels and drained by the outermost drop.
pub(crate) fn emit_drop_shape_deep(
    slots: &[DeepSlot],
    tagged: Option<(u32, Vec<(u32, Vec<DeepSlot>)>)>,
    (gq, gd): (u32, u32),
    drop_ty: u32,
) -> Function {
    // params: 0=block; locals: 1=rc, 2=q, 3=v, 4=len
    let (block, rc, q, v, len) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let mut f = Function::new([(4, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(block).global_get(G_LINE_END).i32_lt_u().if_(BlockType::Empty);
    i.return_();
    i.end();
    i.local_get(block).i32_load(word(almide_layout::RC.offset)).i32_const(1).i32_sub().local_set(rc);
    i.local_get(rc).i32_const(-1).i32_eq().if_(BlockType::Empty);
    i.unreachable();
    i.end();
    i.local_get(block).local_get(rc).i32_store(word(almide_layout::RC.offset));
    i.local_get(rc).i32_eqz().if_(BlockType::Empty);
    // The releases, by tag for a variant: `queue` = push instead of call.
    let releases = |i: &mut InstructionSink<'_>, queue: bool| {
        let one = |i: &mut InstructionSink<'_>, &(off, dec, slot): &DeepSlot| {
            i.local_get(block).i32_load(word(almide_layout::PAYLOAD + off));
            if queue && slot != 0 {
                emit_push(i, gq, q, v);
                i.i32_const(slot as i32);
                emit_push(i, gq, q, v);
            } else {
                i.call(dec);
            }
        };
        for s in slots {
            one(i, s);
        }
        if let Some((tag_off, cases)) = &tagged {
            for (tag, cslots) in cases {
                i.local_get(block).i32_load(word(almide_layout::PAYLOAD + tag_off)).i32_const(*tag as i32).i32_eq();
                i.if_(BlockType::Empty);
                for s in cslots {
                    one(i, s);
                }
                i.end();
            }
        }
    };
    i.global_get(gd).i32_const(DEEP_LIMIT).i32_lt_u().if_(BlockType::Empty);
    i.global_get(gd).i32_const(1).i32_add().global_set(gd);
    releases(&mut i, false);
    i.local_get(block).call(F_FREE);
    i.global_get(gd).i32_const(1).i32_sub().global_set(gd);
    // The outermost drop drains what the deep ones queued, one level each,
    // at depth 1 so a drained drop never starts a drain of its own.
    i.global_get(gd).i32_eqz().if_(BlockType::Empty);
    i.i32_const(1).global_set(gd);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    emit_queue_len(&mut i, gq);
    i.i32_eqz().br_if(1);
    emit_pop(&mut i, gq, 2, q, len);
    i.call_indirect(0, drop_ty);
    i.br(0);
    i.end();
    i.end();
    i.i32_const(0).global_set(gd);
    i.end();
    i.else_();
    // Deep: the children's addresses are read before the block is freed.
    releases(&mut i, true);
    i.local_get(block).call(F_FREE);
    i.end();
    i.end();
    i.end();
    f
}

impl crate::emitter::Emitter<'_> {
    /// The body of `$named_eq_<ti>` for a type on a cycle (#4250): params
    /// `(a, b)`, verdict on the stack. Past the limit the pair is queued with
    /// this helper's own table slot and answers `true` for now; the
    /// outermost comparison folds every queued pair into its verdict.
    pub(crate) fn emit_named_eq_deep(&mut self, ti: u32) -> Result<(), EmitError> {
        use crate::work::{Helper, NamedOp};
        let gq = global(self.work, EQ_Q);
        let gd = global(self.work, EQ_DEPTH);
        let me = self.work.helper(Helper::NamedOp { op: NamedOp::Eq, ti });
        let slot = self.work.slot(TableEntry::Direct(me));
        let eq_ty = self.work.itype(vec![ValType::I32, ValType::I32], Some(ValType::I32));
        let q = self.hold_i32()?;
        let v = self.hold_i32()?;
        let len = self.hold_i32()?;
        let r = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.global_get(gd).i32_const(DEEP_LIMIT).i32_ge_u().if_(BlockType::Empty);
            i.local_get(0);
            emit_push(&mut i, gq, q, v);
            i.local_get(1);
            emit_push(&mut i, gq, q, v);
            i.i32_const(slot as i32);
            emit_push(&mut i, gq, q, v);
            i.i32_const(1).return_();
            i.end();
            i.global_get(gd).i32_const(1).i32_add().global_set(gd);
            i.local_get(0).local_get(1);
        }
        self.emit_named_eq(ti, &mut vec![ti])?;
        {
            let mut i = self.f.instructions();
            i.local_set(r);
            i.global_get(gd).i32_const(1).i32_sub().global_set(gd);
            i.global_get(gd).i32_eqz().if_(BlockType::Empty);
            i.i32_const(1).global_set(gd);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(r).i32_eqz().br_if(1);
            emit_queue_len(&mut i, gq);
            i.i32_eqz().br_if(1);
            emit_pop(&mut i, gq, 3, q, len);
            i.call_indirect(0, eq_ty).local_set(r);
            i.br(0);
            i.end();
            i.end();
            // A `false` verdict leaves the rest of the queue unread.
            i.global_get(gq).if_(BlockType::Empty);
            i.global_get(gq).i32_const(0).i32_store(len_memarg());
            i.end();
            i.i32_const(0).global_set(gd);
            i.end();
            i.local_get(r);
        }
        for _ in 0..4 {
            self.release_i32();
        }
        Ok(())
    }

    /// Emit a component of a display that is NOT the last thing its level
    /// writes: no nested cut inside it is followed by fixed text alone.
    pub(crate) fn disp_off(&mut self, f: impl FnOnce(&mut Self) -> Result<(), EmitError>) -> Result<(), EmitError> {
        let saved = self.work.disp_tail.borrow_mut().take();
        let r = f(self);
        *self.work.disp_tail.borrow_mut() = saved;
        r
    }

    /// Emit the LAST component of a display level, whose closing text
    /// `close` is all its level writes after it.
    pub(crate) fn disp_last(&mut self, close: &'static str, f: impl FnOnce(&mut Self) -> Result<(), EmitError>) -> Result<(), EmitError> {
        if let Some(s) = self.work.disp_tail.borrow_mut().as_mut() {
            s.push(close);
        }
        let r = f(self);
        if let Some(s) = self.work.disp_tail.borrow_mut().as_mut() {
            s.pop();
        }
        r
    }

    /// The call of a recursive type's display helper `idx` on the block on
    /// the stack (#4250). Past the limit a nested value is not displayed in
    /// place: written LAST in its body, it continues by tail call after
    /// queueing the body's remaining fixed text (written back, innermost
    /// first, by the nearest ordinary call); anywhere else, its position in
    /// the line is queued with the value, and the OUTERMOST call displays
    /// each queued value at the end of the line and rotates it into its
    /// position — the rightmost first, so no queued position moves.
    pub(crate) fn emit_display_call_deep(&mut self, idx: u32) -> Result<(), EmitError> {
        let gq = global(self.work, DISP_Q);
        let gd = global(self.work, DISP_DEPTH);
        let gi = global(self.work, DISP_INS_Q);
        let slot = self.work.slot(TableEntry::Direct(idx));
        let disp_ty = self.work.itype(vec![ValType::I32, ValType::I32], Some(ValType::I32));
        let cursor = self.cursor_local;
        let tail: Option<String> = self.work.disp_tail.borrow().as_ref().map(|s| s.iter().rev().copied().collect());
        let suffix = tail.as_ref().filter(|s| !s.is_empty()).map(|s| self.pool.intern(s));
        let hv = self.hold_i32()?;
        let hh = self.hold_i32()?;
        let hi = self.hold_i32()?;
        let outer = self.hold_i32()?;
        let q = self.hold_i32()?;
        let v = self.hold_i32()?;
        let hp = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hv);
        i.block(BlockType::Empty);
        i.global_get(gd).i32_const(DEEP_LIMIT).i32_ge_u().if_(BlockType::Empty);
        if tail.is_some() {
            if let Some(base) = suffix {
                i.i32_const(base as i32);
                emit_push(&mut i, gq, q, v);
            }
            i.local_get(hv).local_get(cursor).return_call(idx);
        } else {
            i.local_get(cursor);
            emit_push(&mut i, gi, q, v);
            i.local_get(hv);
            emit_push(&mut i, gi, q, v);
            i.i32_const(slot as i32);
            emit_push(&mut i, gi, q, v);
            i.br(1);
        }
        i.end();
        emit_queue_len(&mut i, gq);
        i.local_set(hh);
        emit_queue_len(&mut i, gi);
        i.local_set(hi);
        i.global_get(gd).i32_eqz().local_set(outer);
        i.global_get(gd).i32_const(1).i32_add().global_set(gd);
        i.local_get(hv).local_get(cursor).call(idx).local_set(cursor);
        i.global_get(gd).i32_const(1).i32_sub().global_set(gd);
        emit_flush_suffixes(&mut i, gq, hh, cursor, q, v);
        i.local_get(outer).if_(BlockType::Empty);
        emit_place_queued(&mut i, gi, hi, cursor, disp_ty, [q, v, hv, hh, hp]);
        i.end();
        i.end();
        drop(i);
        for _ in 0..7 {
            self.release_i32();
        }
        Ok(())
    }
}

/// Write back the queued fixed texts above height `h`, innermost first.
fn emit_flush_suffixes(i: &mut InstructionSink<'_>, gq: u32, h: u32, cursor: u32, q: u32, v: u32) {
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    emit_queue_len(i, gq);
    i.local_get(h).i32_le_u().br_if(1);
    emit_pop(i, gq, 1, q, v);
    i.local_set(v);
    i.local_get(cursor);
    i.local_get(v).i32_const(almide_layout::PAYLOAD as i32).i32_add();
    i.local_get(v).i32_load(len_memarg());
    i.call(F_APPEND_COPY).local_set(cursor);
    i.br(0);
    i.end();
    i.end();
}

/// Display every value queued above height `h` at its queued position in
/// the line: popped rightmost first, displayed at the line's end (its own
/// nested cuts are placed by its own outermost calls), then rotated into
/// place. `[q, w, p, a, hp]` are i32 scratch locals.
fn emit_place_queued(i: &mut InstructionSink<'_>, gi: u32, h: u32, cursor: u32, disp_ty: u32, [q, w, p, a, hp]: [u32; 5]) {
    let phys = |i: &mut InstructionSink<'_>, l: u32| {
        i.local_get(l).global_get(G_LINE_DELTA).i32_add();
    };
    let payload = |i: &mut InstructionSink<'_>, l: u32| {
        i.local_get(l).i32_const(almide_layout::PAYLOAD as i32).i32_add();
    };
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    emit_queue_len(i, gi);
    i.local_get(h).i32_le_u().br_if(1);
    // [pos, value, slot]; `hp` = the height without this entry.
    emit_pop(i, gi, 3, q, hp);
    i.local_set(w);
    i.local_set(q);
    i.local_set(p);
    i.local_get(cursor).local_set(a);
    i.local_get(q).local_get(cursor).local_get(w).call_indirect(0, disp_ty).local_set(cursor);
    // Anything the value left queued sits in [a, cursor): it moves by a - p.
    emit_queue_len(i, gi);
    i.local_set(w);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(w).local_get(hp).i32_le_u().br_if(1);
    i.local_get(w).i32_const(12).i32_sub().local_set(w);
    i.global_get(gi).local_get(w).i32_add();
    i.global_get(gi).local_get(w).i32_add().i32_load(word(almide_layout::PAYLOAD));
    i.local_get(a).local_get(p).i32_sub().i32_sub();
    i.i32_store(word(almide_layout::PAYLOAD));
    i.br(0);
    i.end();
    i.end();
    // Rotate [p, a) | [a, cursor) into [a, cursor) | [p, a) through a
    // scratch block, at the line's physical addresses.
    i.local_get(cursor).local_get(a).i32_sub().call(F_ALLOC).local_set(q);
    payload(i, q);
    phys(i, a);
    i.local_get(cursor).local_get(a).i32_sub();
    i.memory_copy(0, 0);
    phys(i, p);
    i.local_get(cursor).local_get(a).i32_sub().i32_add();
    phys(i, p);
    i.local_get(a).local_get(p).i32_sub();
    i.memory_copy(0, 0);
    phys(i, p);
    payload(i, q);
    i.local_get(cursor).local_get(a).i32_sub();
    i.memory_copy(0, 0);
    i.local_get(q).call(F_FREE);
    i.br(0);
    i.end();
    i.end();
}

impl crate::emitter::Emitter<'_> {
    /// A display component: the last of its level (`Some(close)`, the text
    /// its level writes after it) or not (`None`).
    pub(crate) fn disp_part(
        &mut self,
        last: Option<&'static str>,
        f: impl FnOnce(&mut Self) -> Result<(), EmitError>,
    ) -> Result<(), EmitError> {
        match last {
            Some(close) => self.disp_last(close, f),
            None => self.disp_off(f),
        }
    }

    /// The call of named type `ti`'s display helper `idx` on the block on
    /// the stack: a type on a cycle displays a deep value on a bounded stack.
    pub(crate) fn emit_display_call(&mut self, ti: u32, idx: u32) -> Result<(), EmitError> {
        if self.types.named_on_cycle(ti) {
            return self.emit_display_call_deep(idx);
        }
        self.f.instructions().local_get(self.cursor_local).call(idx).local_set(self.cursor_local);
        Ok(())
    }

    /// Write the display body of named type `ti` with `f`: a type on a cycle
    /// starts on its tail spine (#4250).
    pub(crate) fn on_tail_spine(
        &mut self,
        ti: u32,
        f: impl FnOnce(&mut Self) -> Result<(), EmitError>,
    ) -> Result<(), EmitError> {
        *self.work.disp_tail.borrow_mut() = self.types.named_on_cycle(ti).then(Vec::new);
        let r = f(self);
        *self.work.disp_tail.borrow_mut() = None;
        r
    }
}
