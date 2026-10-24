//! The rooted fs family's descriptor calls (#4084): `fs.root_*` reach the
//! `fs.__root_*` intrinsics, one host op each, whose operand is
//! `<root>\0<segments>`. The host (and the p1 fs service on a stock runtime)
//! walks from the root without following a symlink and answers what the
//! plain call on `<root>/<segments>` answers, so each op carries the PLAIN
//! call's name for its errors (`fs.read_text`, ...).

use almide_ir::IrExpr;

use crate::emitter::Emitter;
use crate::*;

pub(crate) const OP_ROOT_READ_TEXT: i32 = 120;
pub(crate) const OP_ROOT_READ_BYTES: i32 = 121;
pub(crate) const OP_ROOT_LIST_DIR: i32 = 122;
pub(crate) const OP_ROOT_EXISTS: i32 = 123;
pub(crate) const OP_ROOT_WRITE_TEXT: i32 = 124;
pub(crate) const OP_ROOT_REMOVE: i32 = 125;

impl Emitter<'_> {
    /// `fs.__root_*`. Ok(None) = not handled here.
    pub(crate) fn lower_fs_root_call(&mut self, func: &str, args: &[IrExpr]) -> Result<Option<Option<Lowered>>, EmitError> {
        let out = match (func, args) {
            ("__root_read_text", [a]) => {
                self.fs_call_1(a, OP_ROOT_READ_TEXT)?;
                self.fs_result_string()?
            }
            ("__root_read_bytes", [a]) => {
                self.fs_call_1(a, OP_ROOT_READ_BYTES)?;
                self.fs_result_bytes()?
            }
            ("__root_list_dir", [a]) => {
                self.fs_call_1(a, OP_ROOT_LIST_DIR)?;
                self.fs_result_string_list()?
            }
            ("__root_exists", [a]) => {
                self.fs_call_1(a, OP_ROOT_EXISTS)?;
                self.fs_result_bytes()?
            }
            ("__root_write_text", [a, c]) => {
                self.fs_call_str2(a, c, OP_ROOT_WRITE_TEXT)?;
                self.fs_result_unit()?
            }
            ("__root_remove", [a]) => {
                self.fs_call_1(a, OP_ROOT_REMOVE)?;
                self.fs_result_unit()?
            }
            _ => return Ok(None),
        };
        Ok(Some(Some(Lowered::owned(out))))
    }
}
