//! The tcp family's host-op leaves (#3620, the net/tcp design step 2) — the
//! guest half of ops 91..=103, which the embedded host serves with native's
//! net core (crates/almide-rt-core/src/net_core.rs). stdlib/net_wasm.almd
//! declares the leaves bodyless (`= _`); they intercept before resolution
//! through `lower_host_leaf`, as the http and subprocess leaves do.
//!
//! Every leaf takes two operands: text in a, text in b — except
//! `__net_write`, whose b is the Bytes to send, byte-exact (the host decodes
//! a as UTF-8, so data never rides a). The reads answer
//! `Result[Bytes, String]`; the rest answer `Result[String, String]` text the
//! Almide body parses.

use almide_ir::IrExpr;

use crate::emitter::Emitter;
use crate::fs_meta::{
    OP_NET_ACCEPT, OP_NET_AVAILABLE, OP_NET_CLOSE, OP_NET_CLOSE_LISTENER, OP_NET_CONNECT, OP_NET_IS_OPEN,
    OP_NET_LISTEN, OP_NET_LOCAL_PORT, OP_NET_READ, OP_NET_READ_EXACT, OP_NET_READ_TIMEOUT, OP_NET_SET_TIMEOUT,
    OP_NET_WRITE,
};
use crate::*;

impl Emitter<'_> {
    /// A `__net_*` leaf, or `Ok(None)` when `name` is not one.
    pub(crate) fn lower_net_leaf(&mut self, name: &str, args: &[IrExpr]) -> Result<Option<SliceTy>, EmitError> {
        let op = match name {
            "__net_listen" => OP_NET_LISTEN,
            "__net_accept" => OP_NET_ACCEPT,
            "__net_connect" => OP_NET_CONNECT,
            "__net_read" => OP_NET_READ,
            "__net_read_exact" => OP_NET_READ_EXACT,
            "__net_read_timeout" => OP_NET_READ_TIMEOUT,
            "__net_write" => OP_NET_WRITE,
            "__net_available" => OP_NET_AVAILABLE,
            "__net_set_timeout" => OP_NET_SET_TIMEOUT,
            "__net_close" => OP_NET_CLOSE,
            "__net_close_listener" => OP_NET_CLOSE_LISTENER,
            "__net_is_open" => OP_NET_IS_OPEN,
            "__net_local_port" => OP_NET_LOCAL_PORT,
            _ => return Ok(None),
        };
        let [a, b] = args else { return unsup("net-leaf-arity") };
        let b_ty = if op == OP_NET_WRITE { crate::bytes::BYTES } else { STR };
        self.fs_call_two((a, STR), (b, b_ty), op)?;
        let reads = [OP_NET_READ, OP_NET_READ_EXACT, OP_NET_READ_TIMEOUT];
        let t = if reads.contains(&op) { self.fs_result_bytes()? } else { self.fs_result_string()? };
        Ok(Some(t))
    }
}
