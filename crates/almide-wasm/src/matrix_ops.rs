//! The second and third stage of the `matrix.*` arm chain (#1423 stage 4):
//! the element/row kernels (matrix_elem.rs) and the products, movers and
//! encoders (matrix_prod.rs, matrix_shape.rs). `lower_matrix_call`
//! (matrix.rs) falls through here; `Ok(None)` falls through to the linked
//! compositions (calls_modules.rs → stdlib/matrix_fused.almd).

use almide_ir::IrExpr;

use crate::emitter::Emitter;
use crate::matrix_elem::ZipOp;
use crate::*;

impl Emitter<'_> {
    /// Element-wise and row-wise kernels.
    pub(crate) fn lower_matrix_call_b(&mut self, func: &str, args: &[IrExpr]) -> Result<Option<Option<Lowered>>, EmitError> {
        let out = match (func, args) {
            ("add", [a, b]) => self.lower_matrix_zip(ZipOp::Add, a, b)?,
            ("sub", [a, b]) => self.lower_matrix_zip(ZipOp::Sub, a, b)?,
            ("div", [a, b]) => self.lower_matrix_zip(ZipOp::Div, a, b)?,
            ("silu_mul", [a, b]) => self.lower_matrix_zip(ZipOp::SiluMul, a, b)?,
            ("neg", [m]) => self.lower_matrix_unary(func, m, None)?,
            ("scale" | "map", [m, x]) => self.lower_matrix_unary(func, m, Some(x))?,
            ("broadcast_add_row" | "causal_mask_add", [m, x]) => self.lower_matrix_row_bias(func, m, x)?,
            ("layer_norm_rows", [m, g, b, eps]) => self.lower_matrix_layer_norm(m, g, b, eps)?,
            _ => return self.lower_matrix_call_c(func, args),
        };
        Ok(Some(out))
    }

    /// Products, row/column movers and byte encoders.
    fn lower_matrix_call_c(&mut self, func: &str, args: &[IrExpr]) -> Result<Option<Option<Lowered>>, EmitError> {
        let out = match (func, args) {
            ("mul", [a, b]) => self.lower_matrix_mul(a, b)?,
            ("linear_row", [x, w, b]) => self.lower_matrix_linear(x, w, Some(b))?,
            ("linear_row_no_bias", [x, w]) => self.lower_matrix_linear(x, w, None)?,
            ("swiglu_gate", [x, g, u]) => self.lower_matrix_swiglu(x, g, u)?,
            ("conv1d", [_, _, _, _, _, _]) => self.lower_matrix_conv1d(args)?,
            ("slice_rows", [m, s, e]) => self.lower_matrix_slice_rows(m, s, e)?,
            ("gather_rows", [m, ids]) => self.lower_matrix_gather_rows(m, ids)?,
            ("concat_cols" | "concat_cols_many", [ms]) => self.lower_matrix_concat_cols(ms)?,
            ("split_cols_even", [m, n]) => self.lower_matrix_split_cols(m, n)?,
            ("to_bytes_f64_le" | "to_bytes_f32_le", [m]) => {
                self.lower_matrix_to_bytes(func == "to_bytes_f32_le", m)?
            }
            _ => return Ok(None),
        };
        Ok(Some(out))
    }
}
