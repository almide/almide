//! net's host-op leaves (#3620) — the guest half of the tcp family's ops
//! 91..=103 (the net/tcp design step 2), which the embedded host serves with
//! native's net core (crates/almide-rt-core/src/net_core.rs), and of the
//! unix-socket and shared-memory family's INTERIM ops 104..=116, served with
//! native's net_unix_core, and of the udp and name-lookup ops 140..=145
//! (#4306), served with the net core. stdlib/net_wasm.almd declares the leaves bodyless
//! (`= _`); they intercept before resolution through `lower_host_leaf`, as
//! the http and subprocess leaves do.
//!
//! Every leaf takes two operands: text in a, text in b — except
//! `__net_write`, `__unix_send` and `__shm_write`, whose b is the Bytes to
//! send, byte-exact (the host decodes a as UTF-8, so data never rides a). The
//! reads answer `Result[Bytes, String]`; the rest answer
//! `Result[String, String]` text the Almide body parses.
//!
//! Ops 104..=116 RETIRE when ADR-0023 step 7 runs the embedded lane as the p3
//! component (the commitment, docs/roadmap/active/net-unix-shm-embedded.md):
//! WASI has neither Unix-domain sockets nor shared memory, so the family then
//! needs Almide's own interface in the component world (#3620), not these
//! core-module ops. The stock artifact never carries them (E081 at build).

use almide_ir::IrExpr;

use crate::emitter::Emitter;
use crate::fs_meta::{
    OP_NET_ACCEPT, OP_NET_AVAILABLE, OP_NET_CLOSE, OP_NET_CLOSE_LISTENER, OP_NET_CONNECT, OP_NET_IS_OPEN,
    OP_NET_LISTEN, OP_NET_LOCAL_PORT, OP_NET_READ, OP_NET_READ_EXACT, OP_NET_READ_TIMEOUT, OP_NET_SET_TIMEOUT,
    OP_NET_WRITE,
};
use crate::*;
use crate::host_ops;

/// The unix-socket and shm family's ops (interim; see the module doc).
const OP_UNIX_CONNECT: i32 = host_ops::nth(host_ops::UNIX_SHM, 0);
const OP_UNIX_LISTEN: i32 = host_ops::nth(host_ops::UNIX_SHM, 1);
const OP_UNIX_ACCEPT: i32 = host_ops::nth(host_ops::UNIX_SHM, 2);
const OP_UNIX_SEND: i32 = host_ops::nth(host_ops::UNIX_SHM, 3);
const OP_UNIX_RECV: i32 = host_ops::nth(host_ops::UNIX_SHM, 4);
const OP_UNIX_TAKE_FDS: i32 = host_ops::nth(host_ops::UNIX_SHM, 5);
const OP_UNIX_POLL: i32 = host_ops::nth(host_ops::UNIX_SHM, 6);
const OP_UNIX_WAIT: i32 = host_ops::nth(host_ops::UNIX_SHM, 7);
const OP_UNIX_CLOSE: i32 = host_ops::nth(host_ops::UNIX_SHM, 8);
const OP_SHM_CREATE: i32 = host_ops::nth(host_ops::UNIX_SHM, 9);
const OP_SHM_RESIZE: i32 = host_ops::nth(host_ops::UNIX_SHM, 10);
const OP_SHM_WRITE: i32 = host_ops::nth(host_ops::UNIX_SHM, 11);
const OP_SHM_READ: i32 = host_ops::nth(host_ops::UNIX_SHM, 12);
const _: () = assert!(OP_SHM_READ == *host_ops::UNIX_SHM.end());

/// The udp and name-lookup ops (#4306): the embedded host serves them with
/// the net core (crates/almide-wasm-run/src/net_host.rs); a stock artifact
/// does not (the p1 op set omits them) until the wasi:sockets route.
const OP_UDP_BIND: i32 = host_ops::nth(host_ops::UDP, 0);
const OP_UDP_LOCAL_PORT: i32 = host_ops::nth(host_ops::UDP, 1);
const OP_UDP_SEND_TO: i32 = host_ops::nth(host_ops::UDP, 2);
const OP_UDP_RECV_FROM: i32 = host_ops::nth(host_ops::UDP, 3);
const OP_UDP_CLOSE: i32 = host_ops::nth(host_ops::UDP, 4);
const OP_LOOKUP_HOST: i32 = host_ops::nth(host_ops::UDP, 5);
const _: () = assert!(OP_LOOKUP_HOST == *host_ops::UDP.end());

/// The ops whose b operand is the Bytes to send.
const SENDS_BYTES: [i32; 4] = [OP_NET_WRITE, OP_UNIX_SEND, OP_SHM_WRITE, OP_UDP_SEND_TO];
/// The ops that answer `Result[Bytes, String]`.
const READS: [i32; 6] = [OP_NET_READ, OP_NET_READ_EXACT, OP_NET_READ_TIMEOUT, OP_UNIX_RECV, OP_SHM_READ, OP_UDP_RECV_FROM];

/// The tcp family's op for a `__net_*` leaf.
fn tcp_op(name: &str) -> Option<i32> {
    Some(match name {
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
        _ => return None,
    })
}

/// The udp and name-lookup op for a `__net_udp_*` / `__net_lookup_host` leaf.
fn udp_op(name: &str) -> Option<i32> {
    Some(match name {
        "__net_udp_bind" => OP_UDP_BIND,
        "__net_udp_local_port" => OP_UDP_LOCAL_PORT,
        "__net_udp_send_to" => OP_UDP_SEND_TO,
        "__net_udp_recv_from" => OP_UDP_RECV_FROM,
        "__net_udp_close" => OP_UDP_CLOSE,
        "__net_lookup_host" => OP_LOOKUP_HOST,
        _ => return None,
    })
}

/// The unix-socket and shm family's op for a `__unix_*` / `__shm_*` leaf.
fn unix_op(name: &str) -> Option<i32> {
    Some(match name {
        "__unix_connect" => OP_UNIX_CONNECT,
        "__unix_listen" => OP_UNIX_LISTEN,
        "__unix_accept" => OP_UNIX_ACCEPT,
        "__unix_send" => OP_UNIX_SEND,
        "__unix_recv" => OP_UNIX_RECV,
        "__unix_take_fds" => OP_UNIX_TAKE_FDS,
        "__unix_poll" => OP_UNIX_POLL,
        "__unix_wait" => OP_UNIX_WAIT,
        "__unix_close" => OP_UNIX_CLOSE,
        "__shm_create" => OP_SHM_CREATE,
        "__shm_resize" => OP_SHM_RESIZE,
        "__shm_write" => OP_SHM_WRITE,
        "__shm_read" => OP_SHM_READ,
        _ => return None,
    })
}

impl Emitter<'_> {
    /// A net leaf, or `Ok(None)` when `name` is not one.
    pub(crate) fn lower_net_leaf(&mut self, name: &str, args: &[IrExpr]) -> Result<Option<SliceTy>, EmitError> {
        let Some(op) = tcp_op(name).or_else(|| udp_op(name)).or_else(|| unix_op(name)) else { return Ok(None) };
        let [a, b] = args else { return unsup("net-leaf-arity") };
        let b_ty = if SENDS_BYTES.contains(&op) { crate::bytes::BYTES } else { STR };
        self.fs_call_two((a, STR), (b, b_ty), op)?;
        let t = if READS.contains(&op) { self.fs_result_bytes()? } else { self.fs_result_string()? };
        Ok(Some(t))
    }
}
