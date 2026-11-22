//! Arguments of `unix`'s socket and shared-memory calls in a generated
//! program (#3620), and the handles of the calls that wait on what a handle
//! holds. A descriptor (`unix.Fd`) is an OS number behind the type, and the
//! program's table refuses one it does not hold, so each descriptor parameter
//! gets a live one of the kind its call needs, a socket path a short relative
//! name in the sandbox (or the fixture's sink, `oracle/fixture.rs`), and a
//! size, offset or length a bounded value (an unbounded read length is an
//! allocation of that size).
//!
//! What a call can wait on is ready before it runs: a socket a `take_fds`
//! reads holds "hello world" and one shared-memory descriptor, with its peer
//! already closed, so a second read answers the empty end instead of
//! blocking; a listener an `accept` takes from has a connection pending; and
//! a `write_fds` writes to the sink, which drains everything. A call that
//! consumes what its handle holds (an accept, a read) gets a handle of its own
//! ([`fresh_handle`]): a reused variable's second accept would wait forever.

use std::collections::HashMap;

use super::catalogue::Signature;
use super::term::Builder;
use super::types::GenType;

/// The fixture's Unix-domain sink path.
const SINK: &str = "(env.get(\"XF_UNIX_SINK\") ?? \"\")";

/// `e`'s ok payload, an `err` stopping the program.
fn ok_or_panic(e: &str) -> String {
    format!("(match {e} {{ ok(okv) => okv, err(e) => panic(e) }})")
}

/// A short socket name of its own, relative to the sandbox.
fn socket_name(b: &mut Builder) -> String {
    let n = b.fresh;
    b.fresh += 1;
    format!("\"u{n}.sock\"")
}

/// A stream to the sink: what is sent drains, nothing ever arrives.
fn sink_stream() -> String {
    ok_or_panic(&format!("unix.connect({SINK})"))
}

/// A fresh shared-memory file of 8 bytes.
fn shm() -> String {
    ok_or_panic("unix.shm_create(8)")
}

/// A listener at `p` with one connection pending.
fn pending_listener(p: &str) -> String {
    format!(
        "{{ let ul = {}\n let _uc: Result[Fd, String] = unix.connect({p})\n ul }}",
        ok_or_panic(&format!("unix.listen({p})"))
    )
}

/// A socket holding "hello world" and one shared-memory descriptor, its
/// peer closed.
fn pending_socket(p: &str) -> String {
    format!(
        "{{ let ul = {}\n let uc = {}\n let us = {}\n \
         let _uw: Result[Unit, String] = unix.write_fds(uc, bytes.from_string(\"hello world\"), [{}])\n \
         let _ux: Result[Unit, String] = io.close(uc)\n let _uy: Result[Unit, String] = io.close(ul)\n us }}",
        ok_or_panic(&format!("unix.listen({p})")),
        ok_or_panic(&format!("unix.connect({p})")),
        ok_or_panic("unix.accept(ul)"),
        shm()
    )
}

/// The argument for a descriptor, path or descriptor-list parameter of a
/// `unix.*` call; None for any other parameter.
pub(super) fn arg(b: &mut Builder, func: &str, name: &str) -> Option<String> {
    let a = match (func, name) {
        ("connect", "path") => {
            if b.rng.below(4) == 0 {
                "\"missing.sock\"".into()
            } else {
                SINK.into()
            }
        }
        ("listen", "path") => socket_name(b),
        ("accept", "listener") => pending_listener(&socket_name(b)),
        ("write_fds", "sock") => sink_stream(),
        ("write_fds", "fds") => {
            if b.rng.below(2) == 0 {
                "[]".into()
            } else {
                format!("[{}]", shm())
            }
        }
        ("take_fds", "sock") => pending_socket(&socket_name(b)),
        ("select", "xs") => format!("[{}, {}]", sink_stream(), pending_socket(&socket_name(b))),
        (_, "fd") if func.starts_with("shm_") => shm(),
        _ => return None,
    };
    b.effect = true;
    Some(a)
}

/// A generated Int argument brought into a bounded domain (`{x}` is the
/// generated expression); None when the parameter takes any Int. `%` keeps a
/// negative value negative, so the call's own refusal of it stays reachable.
pub(super) fn int_domain(func: &str, name: &str) -> Option<&'static str> {
    match (func, name) {
        ("shm_create" | "shm_resize", "size") => Some("({x}) % 4096"),
        ("shm_write" | "shm_read", "offset") | ("shm_read", "len") => Some("({x}) % 64"),
        ("select", "timeout_ms") => Some("int.abs({x}) % 50"),
        _ => None,
    }
}

/// A UDP socket with one datagram of its own pending, so a receive never
/// waits.
const UDP_PENDING: &str = "{ let us = (match net.udp_bind(\"127.0.0.1\", 0) { ok(okv) => okv, err(e) => panic(e) })\n let _us: Result[Unit, String] = net.udp_send_to(us, bytes.from_string(\"hello world\"), \"127.0.0.1\", net.udp_local_port(us) ?? 0)\n us }";

/// A child that runs until it is stopped.
const LONG_CHILD: &str = "(match process.start(process.ProcessCommand { cmd: \"/bin/sleep\", args: [\"5\"] }) { ok(okv) => okv, err(e) => panic(e) })";

/// A handle of its own for a call that consumes what its handle holds (an
/// accept, a read, a child's end), instead of a variable another call may
/// already have drained; None for any other parameter.
pub(super) fn fresh_handle(
    b: &mut Builder,
    sig: &Signature,
    name: &str,
    subst: &HashMap<String, GenType>,
) -> Option<String> {
    let mut nominal = |k: &str| super::nominal::get(k).map(|n| b.rng.pick(n.leaves).to_string());
    let a = match (sig.module, sig.func.as_str(), name) {
        ("net", "tcp_accept", "listener") => nominal("TcpListener")?,
        ("net", "tcp_read_timeout" | "tcp_available", "stream") => nominal("TcpStream")?,
        ("net", "udp_recv_from", "sock") => UDP_PENDING.to_string(),
        ("io", "read" | "read_exact" | "read_to_end", "h") => match subst.get("H") {
            Some(GenType::Nominal(k)) => nominal(k)?,
            _ => return None,
        },
        ("process", "stop", "c") => LONG_CHILD.to_string(),
        ("process", _, "c") if sig.params.first().is_some_and(|p| {
            *p == super::sig_type::SigType::Nominal("ProcessChild")
        }) =>
        {
            nominal("ProcessChild")?
        }
        _ => return None,
    };
    b.effect = true;
    Some(a)
}
