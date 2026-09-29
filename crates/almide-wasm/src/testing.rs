//! `testing.assert_*` (#2743): the library asserts as native arms — test,
//! then on failure write one fixed line to stderr and exit 1.
//!
//! The self-host bodies (stdlib/testing_assert.almd) cannot be linked on
//! this leg: the Option / Result twins read the INCUMBENT's tag positions
//! raw (`load32(h+4)` / `load32(h+16)`), and they are typed at one payload
//! class each (the incumbent rewrites every other instantiation to an
//! unlinkable name). The arms below read the tag through THIS emitter's
//! layout and take any payload type.
//!
//! The failure bytes are the self-host bodies' `prim.die` lines, verbatim —
//! the bytes the wasm lane has always printed for these asserts. Native
//! panics through libtest with the operand values in the message instead;
//! no contract pins the failure text of this family (the pass path is
//! silent on every target, and that is the observable the test lane
//! compares). `assert_throws` stays native-only (an uncatchable trap), and
//! `assert_snapshot` is desugared by the frontend before it reaches here.

use almide_ir::IrExpr;
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// Ok(None) = not an assert this arm lowers (the caller falls through
    /// to the linked path, which walls honestly).
    pub(crate) fn lower_testing_assert(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<Option<Lowered>>, EmitError> {
        // Leaves the PASS condition (i32 0/1) on the stack.
        let msg = match (func, args) {
            ("assert_gt" | "assert_lt", [a, b]) => {
                self.lower_arg(a, Some(INT), ArgMode::Borrow)?;
                self.lower_arg(b, Some(INT), ArgMode::Borrow)?;
                let mut i = self.f.instructions();
                if func == "assert_gt" {
                    i.i64_gt_s();
                } else {
                    i.i64_lt_s();
                }
                if func == "assert_gt" { "assert_gt failed\n" } else { "assert_lt failed\n" }
            }
            // `abs(a - b) < epsilon`, strict: f64.abs is IEEE abs (NaN in,
            // NaN out, and NaN compares false — native's `(a - b).abs() <
            // epsilon` fails the same inputs).
            ("assert_approx", [a, b, eps]) => {
                self.lower_arg(a, Some(FLOAT), ArgMode::Borrow)?;
                self.lower_arg(b, Some(FLOAT), ArgMode::Borrow)?;
                self.f.instructions().f64_sub().f64_abs();
                self.lower_arg(eps, Some(FLOAT), ArgMode::Borrow)?;
                self.f.instructions().f64_lt();
                "assert_approx failed\n"
            }
            // The needle test IS `string.contains` — the linked, audited
            // impl, under its own argument conventions (the demand is
            // registered by the wasm leg's self-host scan).
            ("assert_contains", [_, _]) => {
                self.lower_linked_call("string", "contains", args, false)?;
                "assert_contains failed\n"
            }
            // A `none` is the NULL handle on this leg (option.is_some).
            ("assert_some" | "assert_none", [o]) => {
                let SliceTy::Option(_) = self.lower_arg(o, None, ArgMode::Borrow)? else {
                    return unsup(&format!("testing-{func}-of-nonoption"));
                };
                let mut i = self.f.instructions();
                i.i32_eqz();
                if func == "assert_some" {
                    i.i32_eqz();
                    "assert_some failed: got None\n"
                } else {
                    "assert_none failed\n"
                }
            }
            // Tag 0 = ok at SUM_TAG (result.is_ok).
            ("assert_ok" | "assert_err", [r]) => {
                let SliceTy::Result(..) = self.lower_arg(r, None, ArgMode::Borrow)? else {
                    return unsup(&format!("testing-{func}-of-nonresult"));
                };
                let mut i = self.f.instructions();
                i.i32_load(slot_memarg(almide_layout::SUM_TAG));
                if func == "assert_ok" {
                    i.i32_eqz();
                    "assert_ok failed\n"
                } else {
                    i.i32_const(0).i32_ne();
                    "assert_err failed\n"
                }
            }
            _ => return Ok(None),
        };
        self.f.instructions().i32_eqz().if_(BlockType::Empty);
        // The line is a pool static (no credit to release): lowered plain,
        // not through `lower_arg` — it is not one of the call's arguments.
        let line = IrExpr {
            kind: almide_ir::IrExprKind::LitStr { value: msg.to_string() },
            ty: almide_types::types::Ty::String,
            span: None,
            def_id: None,
        };
        self.lower(&line, Some(STR))?;
        self.io_raw(crate::fs_meta::OP_STDERR_RAW)?;
        self.f.instructions().i32_const(1).call(F_EXIT_IMPORT).unreachable().end();
        Ok(Some(None))
    }
}
