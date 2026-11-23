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
// `host_ops::unix_shm_caps` counts the fs capability of these three by number.
const _: () = assert!(OP_UNIX_CONNECT == 104 && OP_UNIX_LISTEN == 105 && OP_SHM_CREATE == 113);

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

/// The host op a net / udp / unix / shm leaf lowers to — the one lookup the
/// emitter and the category agreement gate (#4916) both read.
pub(crate) fn leaf_op(name: &str) -> Option<i32> {
    tcp_op(name).or_else(|| udp_op(name)).or_else(|| unix_op(name))
}

impl Emitter<'_> {
    /// A net leaf, or `Ok(None)` when `name` is not one.
    pub(crate) fn lower_net_leaf(&mut self, name: &str, args: &[IrExpr]) -> Result<Option<SliceTy>, EmitError> {
        let Some(op) = leaf_op(name) else { return Ok(None) };
        let [a, b] = args else { return unsup("net-leaf-arity") };
        let b_ty = if SENDS_BYTES.contains(&op) { crate::bytes::BYTES } else { STR };
        self.fs_call_two((a, STR), (b, b_ty), op)?;
        let t = if READS.contains(&op) { self.fs_result_bytes()? } else { self.fs_result_string()? };
        Ok(Some(t))
    }
}

#[cfg(test)]
mod category_agreement {
    //! #4916, the wasm leg: every `@implements("m.f")` body of the net / unix
    //! wasm lowering reaches its leaves' host ops, and those ops' declared
    //! capabilities (`host_ops::caps_of`) must be exactly what the checker's
    //! categories for `m.f` name. The wrappers and the leaves they call are
    //! read from the sources; no fn is listed here.
    use super::leaf_op;
    use crate::cert_project::cap::{CLOCK, ENTROPY, ENV, FS_READ, FS_WRITE, NET, PROC, STDIN, STDOUT};
    use almide_ir::effect::{stdlib_call_effect, Effect};
    use std::collections::BTreeSet;

    fn leaf_cap(e: Effect) -> Option<u32> {
        Some(match e {
            Effect::FsRead => FS_READ,
            Effect::FsWrite => FS_WRITE,
            Effect::NetFetch | Effect::NetListen => NET,
            Effect::IoStdin => STDIN,
            Effect::IoStdout | Effect::IoStderr => STDOUT,
            Effect::EnvRead | Effect::EnvWrite => ENV,
            Effect::Proc => PROC,
            Effect::Time => CLOCK,
            Effect::Rand => ENTROPY,
            Effect::Fan | Effect::Abort => return None,
        })
    }

    /// `(implemented fn, the caps its leaves reach)` for every `@implements`
    /// body of `src` that calls at least one leaf.
    fn wrappers(src: &str) -> Vec<(String, BTreeSet<u32>)> {
        let mut out = Vec::new();
        for chunk in src.split("@implements(\"").skip(1) {
            let Some((target, rest)) = chunk.split_once('"') else { continue };
            // `rest` opens with the attribute's `)`; the declaration line
            // follows, and the body runs to the next top-level declaration.
            let mut lines = rest.lines().skip(1);
            let first = lines.next().unwrap_or("");
            let body: String = lines.take_while(|l| !(l.starts_with("fn ") || l.starts_with("effect fn ") || l.starts_with('@'))).collect::<Vec<_>>().join("\n");
            let text = format!("{}\n{body}", first.split_once('=').map_or("", |(_, b)| b));
            let mut caps = BTreeSet::new();
            let mut hit = false;
            for (i, _) in text.match_indices("__") {
                let name: String = text[i..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                if text[i + name.len()..].starts_with('(') {
                    if let Some(op) = leaf_op(&name) {
                        hit = true;
                        caps.extend(crate::host_ops::caps_of(op).unwrap_or_else(|| panic!("op {op} of {name} has no caps")).iter().copied());
                    }
                }
            }
            if hit {
                out.push((target.to_string(), caps));
            }
        }
        out
    }

    fn disagreements(rows: &[(String, BTreeSet<u32>)]) -> Vec<String> {
        rows.iter().filter_map(|(target, caps)| {
            let (m, f) = target.split_once('.').expect("m.f");
            let cats: BTreeSet<u32> = stdlib_call_effect(m, f).into_iter().filter_map(leaf_cap).collect();
            (cats != *caps).then(|| format!("{target}: host ops reach {caps:?}, checker categories {cats:?}"))
        }).collect()
    }

    fn rows() -> Vec<(String, BTreeSet<u32>)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../stdlib");
        let mut rows = Vec::new();
        for f in std::fs::read_dir(&dir).expect("stdlib dir") {
            let p = f.expect("entry").path();
            if p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with("_wasm.almd")) {
                rows.extend(wrappers(&std::fs::read_to_string(&p).expect("read")));
            }
        }
        rows
    }

    #[test]
    fn every_net_leaf_wrapper_reaches_the_capabilities_its_categories_name() {
        let rows = rows();
        assert!(rows.len() >= 25, "only {} leaf wrappers read: the scan went blind", rows.len());
        let bad = disagreements(&rows);
        assert!(bad.is_empty(), "wasm leaf capabilities disagree with the checker's categories (#4916):\n{}", bad.join("\n"));
    }

    /// The control: each wrapper's reach with one capability added (and, when
    /// it has one, one dropped) is caught.
    #[test]
    fn a_wrapper_whose_reach_drifts_from_its_categories_is_caught() {
        let rows = rows();
        for (i, (t, caps)) in rows.iter().enumerate() {
            let extra = [STDOUT, ENTROPY, ENV, FS_READ, FS_WRITE, CLOCK, STDIN, NET, PROC].into_iter().find(|c| !caps.contains(c)).expect("a cap");
            let mut added = rows.clone();
            added[i].1.insert(extra);
            assert!(disagreements(&added).iter().any(|l| l.starts_with(&format!("{t}:"))), "{t} + {extra} not caught");
            if let Some(c) = caps.iter().next().copied() {
                let mut dropped = rows.clone();
                dropped[i].1.remove(&c);
                assert!(disagreements(&dropped).iter().any(|l| l.starts_with(&format!("{t}:"))), "{t} - {c} not caught");
            }
        }
    }
}
