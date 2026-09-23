//! Interpolation builds, and the BOUNDED build (#2312 shape 1).
//!
//! A build appends through `$append_copy`, which checks the room and calls
//! `$line_grow` when a write would leave it (#1826). `$line_grow` calls
//! `$alloc`, and `$alloc` carries the C-197 out-of-memory abort and the
//! stderr writer — so ONE `println("${n}")` used to link the allocator
//! into a program that never allocates (fib: 472 of 1,144 optimized
//! bytes). A bounded build writes through the room-free appends
//! (`runtime_line.rs`, `Helper::AppendRaw` & co.) instead.
//!
//! # The soundness condition
//!
//! A room-free append at logical cursor `cur` of at most `n` bytes is sound
//! iff `cur + n <= G_LINE_ROOM` at the moment it runs. We establish it as
//! follows, and ONLY for an OUTERMOST build:
//!
//! 1. **Outermost** = lowered in `main`'s own body (`in_main`: not a lifted
//!    lambda, not a program fn, not a display helper) at `build_depth == 0`
//!    (not lexically inside another build's part). `main`'s body runs
//!    exactly once, as the program entry: a user call to `main` targets the
//!    separately lowered program-fn copy (`in_main == false`), and nothing
//!    in the module calls back into the entry body. So when an outermost
//!    build begins, NO build is open anywhere — every byte in
//!    `[line_start, G_LINE_CURSOR)` is dead.
//! 2. Because of (1) the build STARTS AT `line_start` (read from
//!    `G_LINE_START`, not from `G_LINE_CURSOR`). This does not assume the
//!    cursor global was restored by every earlier build — a build abandoned
//!    by an early exit (`!` out of a part) leaves it advanced, and a proof
//!    that trusted it would drift with each such exit.
//! 3. `G_LINE_ROOM >= line_start + LINE_BUF_MIN` always: it starts at the
//!    heap floor `line_start + LINE_BUF_MIN` and only `$line_grow` writes
//!    it, never downward (capacity at least doubles).
//! 4. A part's STATIC bound: a literal is its byte length, an `Int` is at
//!    most 20 bytes (`-9223372036854775808`), a `Bool` at most 5
//!    (`false`), `int.to_string(e)` is `e`'s Int display. A running budget
//!    starts at `LINE_BUF_MIN` and each room-free append spends its bound;
//!    the first part with no static bound (or one that would overdraw) ends
//!    the room-free prefix, and it and every later part go through the
//!    CHECKED appends. So a room-free append always runs at
//!    `cur <= line_start + spent` with `spent + n <= LINE_BUF_MIN`, hence
//!    `cur + n <= G_LINE_ROOM` by (3).
//! 5. A part expression can run arbitrary user code, and that code can
//!    open its OWN builds (a value-position `"${…}"` in a called fn, in a
//!    lambda, or lexically inside the part). Those are never outermost
//!    (another function's body, or `build_depth > 0` here), so they take
//!    the checked appends; they start at the cursor this build publishes
//!    before each part, so they write only ABOVE this build's live bytes;
//!    they may relocate the region (`$line_grow` copies this build's
//!    partial text and re-bases `G_LINE_DELTA`) — the room-free appends add
//!    `G_LINE_DELTA` at write time, so they follow it — and they only ever
//!    grow the room, so (3) and (4) survive them. This build's cursor is
//!    its own local; nothing they do moves it.
//!
//! Only the room check is dropped: the bytes written, their order, the
//! flush (`$line_println` from `start + G_LINE_DELTA`), and the cursor
//! restore are the checked path's, so stdout / stderr / exit are unchanged.
//! A non-outermost build is exactly the checked build it always was.

use almide_ir::{CallTarget, IrExpr, IrExprKind, IrStringPart};

use almide_types::types::Ty;

use crate::emitter::Emitter;
use crate::*;

/// Byte bound of an `Int`'s decimal display (`i64::MIN`).
const INT_DISPLAY_MAX: u64 = 20;
/// Byte bound of a `Bool`'s display (`false`).
const BOOL_DISPLAY_MAX: u64 = 5;

/// `int.to_string(e)` → `e`: the String displays exactly as `e`'s Int
/// display. (Inside an interpolation the IR already made that rewrite —
/// `arg_temps::fold_int_display_parts`; this is the bare
/// `println(int.to_string(e))` line.)
fn int_to_string_arg(e: &IrExpr) -> Option<&IrExpr> {
    match &e.kind {
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
            if module.as_str() == "int" && func.as_str() == "to_string" && args.len() == 1 =>
        {
            Some(&args[0])
        }
        _ => None,
    }
}

/// The Int a whole line displays, if the line is exactly one Int.
fn sole_int_display(arg: &IrExpr) -> Option<&IrExpr> {
    let part = match &arg.kind {
        IrExprKind::StringInterp { parts } => match parts.as_slice() {
            [IrStringPart::Expr { expr }] => expr,
            _ => return None,
        },
        _ => arg,
    };
    match int_to_string_arg(part) {
        Some(inner) => Some(inner),
        None if matches!(arg.kind, IrExprKind::StringInterp { .. }) && part.ty == Ty::Int => Some(part),
        None => None,
    }
}

/// Spend `n` bytes of the room-free budget. `false` (and the budget is
/// gone for the rest of the build) when there is none or it would
/// overdraw — condition (4).
fn spend(budget: &mut Option<u64>, n: u64) -> bool {
    match *budget {
        Some(rem) if n <= rem => {
            *budget = Some(rem - n);
            true
        }
        _ => {
            *budget = None;
            false
        }
    }
}

impl Emitter<'_> {
    /// Whether this pass emits the bounded-line rewrites at all
    /// (`FnWork::bounded_lines`; the driver compares against the checked
    /// emission and ships the smaller). Asking records that one fired.
    fn bounded_rewrite(&self, applies: bool) -> bool {
        let on = applies && self.work.bounded_lines.get();
        if on {
            self.work.bounded_fired.set(true);
        }
        on
    }

    /// Condition (1): the build opened here is outermost.
    fn build_is_outermost(&self) -> bool {
        self.bounded_rewrite(self.in_main && self.build_depth == 0)
    }

    /// A line that is ONE Int's decimal display — `println(int.to_string(e))`,
    /// `println("${e}")` with `e: Int`, `println("${int.to_string(e)}")` —
    /// prints straight from the itoa scratch through `$print_i64` (any
    /// context: it never touches the line buffer, so no build is opened and
    /// no room is needed). `false` = not this shape; nothing was emitted.
    pub(crate) fn lower_int_line(&mut self, arg: &IrExpr, import: u32) -> Result<bool, EmitError> {
        let Some(e) = sole_int_display(arg).filter(|_| self.bounded_rewrite(true)) else { return Ok(false) };
        let helper = self.work.helper(Helper::PrintI64 { import });
        self.arm_scope(|em| {
            em.lower_arg(e, Some(INT), ArgMode::Borrow)?;
            em.f.instructions().call(helper);
            Ok(())
        })?;
        Ok(true)
    }

    /// Build interpolation parts into the line buffer (stack-disciplined:
    /// nested value-position builds start after our partial content and
    /// restore on their exit). Returns the hold local carrying the build's
    /// start; the caller consumes the region [start, cursor_local), then
    /// must restore `G_LINE_CURSOR = start` and `release_i32()`.
    pub(crate) fn lower_interp_build(&mut self, parts: &[IrStringPart]) -> Result<u32, EmitError> {
        let start = self.hold_i32()?;
        let outermost = self.build_is_outermost();
        // (2): an outermost build starts at line_start; any other at the
        // current cursor.
        let origin = if outermost { G_LINE_START } else { G_LINE_CURSOR };
        self.f
            .instructions()
            .global_get(origin)
            .local_tee(start)
            .local_set(self.cursor_local);
        let mut budget = outermost.then_some(LINE_BUF_MIN);
        self.build_depth += 1;
        let out = parts.iter().try_for_each(|part| self.lower_interp_part(part, &mut budget));
        self.build_depth -= 1;
        out.map(|()| start)
    }

    fn lower_interp_part(&mut self, part: &IrStringPart, budget: &mut Option<u64>) -> Result<(), EmitError> {
        match part {
            IrStringPart::Lit { value } => {
                if value.is_empty() {
                    return Ok(());
                }
                let base = self.pool.intern(value);
                let len = value.len();
                let append = if spend(budget, len as u64) { self.raw_append_helper(None) } else { F_APPEND_COPY };
                self.f
                    .instructions()
                    .local_get(self.cursor_local)
                    .i32_const((base + almide_layout::PAYLOAD) as i32)
                    .i32_const(len as i32)
                    .call(append)
                    .local_set(self.cursor_local);
            }
            IrStringPart::Expr { expr } => {
                // Publish our cursor so a nested build starts past it.
                self.f
                    .instructions()
                    .local_get(self.cursor_local)
                    .global_set(G_LINE_CURSOR);
                let got = self.lower(expr, None)?;
                self.append_display_part(got, budget)?;
            }
        }
        Ok(())
    }

    /// The value of one part is on the stack: append it room-free when its
    /// display has a static bound the budget covers, checked otherwise.
    fn append_display_part(&mut self, got: SliceTy, budget: &mut Option<u64>) -> Result<(), EmitError> {
        let bound = match got {
            INT => INT_DISPLAY_MAX,
            BOOL => BOOL_DISPLAY_MAX,
            _ => {
                *budget = None;
                return self.emit_display_value(got, false);
            }
        };
        if !spend(budget, bound) {
            return self.emit_display_value(got, false);
        }
        let helper = self.raw_append_helper(Some(got));
        let scratch = if got == INT { self.scr_i64_local } else { self.tmp_i32_local };
        self.f.instructions().local_set(scratch);
        self.f
            .instructions()
            .local_get(self.cursor_local)
            .local_get(scratch)
            .call(helper)
            .local_set(self.cursor_local);
        Ok(())
    }

    /// The room-free append for a value of `ty` (`None` = a byte range).
    /// `$append_raw` is registered FIRST, always: the Int / Bool wrappers
    /// call it, and their bodies look its index up at assembly time.
    fn raw_append_helper(&mut self, ty: Option<SliceTy>) -> u32 {
        let raw = self.work.helper(Helper::AppendRaw);
        match ty {
            Some(INT) => self.work.helper(Helper::AppendI64Raw),
            Some(_) => {
                let true_base = self.pool.intern("true");
                let false_base = self.pool.intern("false");
                self.work.helper(Helper::AppendBoolRaw { true_base, false_base })
            }
            None => raw,
        }
    }
}
