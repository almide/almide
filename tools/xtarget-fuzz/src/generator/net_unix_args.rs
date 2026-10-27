//! Arguments of net's Unix-socket and shared-memory calls in a generated
//! program (#3620). A descriptor is a raw OS number, and the program's table
//! refuses one it does not hold, so a generated Int is never handed to a
//! descriptor parameter: it could name a descriptor the program holds on one
//! leg and not on another. Each descriptor parameter gets a live one of the
//! kind its call needs, a socket path a short relative name in the sandbox
//! (or the fixture's sink, `oracle/fixture.rs`), and a size, offset or length
//! a bounded value (an unbounded read length is an allocation of that size).
//!
//! What a call can wait on is ready before it runs: a socket a `recv` reads
//! holds "hello world" and one shared-memory descriptor, with its peer
//! already closed, so a second `recv` answers the empty end instead of
//! blocking; a listener an `accept` takes from has a connection pending; and
//! a `send` writes to the sink, which drains everything.

use super::term::Builder;

/// The fixture's Unix-domain sink path.
const SINK: &str = "(env.get(\"XF_UNIX_SINK\") ?? \"\")";

/// A short socket name of its own, relative to the sandbox.
fn socket_name(b: &mut Builder) -> String {
    let n = b.fresh;
    b.fresh += 1;
    format!("\"u{n}.sock\"")
}

/// A stream to the sink: what is sent drains, nothing ever arrives.
fn sink_stream() -> String {
    format!("(unix.connect({SINK}) ?? -1)")
}

/// A fresh shared-memory file of 8 bytes.
fn shm() -> String {
    "(unix.shm_create(8) ?? -1)".into()
}

/// A listener at `p` with one connection pending.
fn pending_listener(p: &str) -> String {
    format!("{{ let ul: Int = unix.listen({p}) ?? -1\n let _uc: Int = unix.connect({p}) ?? -1\n ul }}")
}

/// A socket holding "hello world" and one shared-memory descriptor, its
/// peer closed.
fn pending_socket(p: &str) -> String {
    format!(
        "{{ let ul: Int = unix.listen({p}) ?? -1\n let uc: Int = unix.connect({p}) ?? -1\n \
         let us: Int = unix.accept(ul) ?? -1\n \
         let _uw: Result[Unit, String] = unix.write_fds(uc, bytes.from_string(\"hello world\"), [{}])\n \
         let _ux: Result[Unit, String] = io.close(uc)\n let _uy: Result[Unit, String] = io.close(ul)\n us }}",
        shm()
    )
}

/// The argument for a descriptor, path or descriptor-list parameter of a
/// `net.unix_*` / `net.shm_*` call; None for any other parameter.
pub(super) fn arg(b: &mut Builder, func: &str, name: &str) -> Option<String> {
    if !(func.starts_with("unix_") || func.starts_with("shm_")) {
        return None;
    }
    let a = match (func, name) {
        ("unix_connect", "path") => {
            if b.rng.below(4) == 0 { "\"missing.sock\"".into() } else { SINK.into() }
        }
        ("unix_listen", "path") => socket_name(b),
        ("unix_accept", "listener") => pending_listener(&socket_name(b)),
        ("unix_send", "sock") => sink_stream(),
        ("unix_send", "fds") => {
            if b.rng.below(2) == 0 { "[]".into() } else { format!("[{}]", shm()) }
        }
        ("unix_recv" | "unix_take_fds", "sock") => pending_socket(&socket_name(b)),
        ("unix_poll" | "unix_close", "fd") => match b.rng.below(3) {
            0 => sink_stream(),
            1 => shm(),
            _ => pending_socket(&socket_name(b)),
        },
        ("unix_wait", "fds") => format!("[{}, {}]", sink_stream(), pending_socket(&socket_name(b))),
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
        ("unix_recv", "max") => Some("({x}) % 64"),
        ("shm_create" | "shm_resize", "size") => Some("({x}) % 4096"),
        ("shm_write" | "shm_read", "offset") | ("shm_read", "len") => Some("({x}) % 64"),
        _ => None,
    }
}
