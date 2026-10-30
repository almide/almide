//! The embedded host's Unix-domain sockets and shared-memory files (#3620):
//! INTERIM ops 104..=116, served with the SAME core native runs
//! (`almide_rt_core::net_unix_core`, which runtime/rs/src/net.rs includes
//! verbatim), so the bytes, the descriptors received and every err text
//! agree with native by construction. The guest receives the host's raw OS
//! descriptor numbers, as native returns them; the run's table holds every
//! descriptor the guest was given and refuses any other number, so a guest
//! cannot reach (or close) a descriptor of the host's own.
//!
//! RETIREMENT COMMITMENT: these ops exist only while the embedded lane runs
//! the p1 core module. When ADR-0023 step 7 runs the embedded lane as the
//! WASI 0.3 component, ops 104..=116 are removed, with their guest leaves
//! (crates/almide-wasm/src/net_call.rs, stdlib/net_wasm.almd), and the family
//! is served through Almide's own interface in the component world or not at
//! all on wasm — docs/roadmap/active/net-unix-shm-embedded.md. WASI has
//! neither Unix-domain sockets nor shared memory, so a stock artifact never
//! carries these ops: `almide build --target wasm` refuses them (E081).
//!
//! Operands are stdlib/net_wasm.almd's decimal text: the path, descriptor or
//! size in `a`; the max, milliseconds or size in `b`; a descriptor list one
//! number per line; `unix_send`'s `a` is the socket then each descriptor to
//! send, its `b` the raw bytes; `shm_write`'s `a` is `<fd>\n<offset>`, its
//! `b` the raw bytes; `shm_read`'s `b` is `<offset>\n<len>`.

use almide_rt_core::net_unix_core::AlmideUnixTables;

/// The first and last unix-socket / shm op.
pub(crate) const OP_FIRST: i32 = *almide_wasm::host_ops::UNIX_SHM.start();
pub(crate) const OP_LAST: i32 = *almide_wasm::host_ops::UNIX_SHM.end();

fn int(s: &str) -> Result<i64, String> {
    s.trim().parse().map_err(|_| format!("malformed net operand {}", almide_base::err_text::almide_rt_errtext_quote(s)))
}

fn ints(s: &str) -> Result<Vec<i64>, String> {
    s.lines().filter(|l| !l.is_empty()).map(int).collect()
}

fn pair(s: &str) -> Result<(i64, i64), String> {
    let (x, y) = s.split_once('\n').ok_or("malformed net operand (missing second number)")?;
    Ok((int(x)?, int(y)?))
}

fn text(r: Result<i64, String>) -> Result<Vec<u8>, String> {
    r.map(|n| n.to_string().into_bytes())
}

fn unit(r: Result<(), String>) -> Result<Vec<u8>, String> {
    r.map(|()| Vec::new())
}

fn lines(fds: Vec<i64>) -> Vec<u8> {
    fds.iter().map(i64::to_string).collect::<Vec<_>>().join("\n").into_bytes()
}

fn send(t: &mut AlmideUnixTables, a: &str, b: &[u8]) -> Result<Vec<u8>, String> {
    let fds = ints(a)?;
    let (sock, riders) = fds.split_first().ok_or("malformed net operand (missing socket)")?;
    unit(t.unix_send(*sock, b, riders))
}

/// One op on the run's table; the answer bytes or the err text.
pub(crate) fn call(t: &mut AlmideUnixTables, op: i32, a: &str, b: &[u8]) -> Result<Vec<u8>, String> {
    let bs = || String::from_utf8_lossy(b).into_owned();
    match op {
        104 => text(t.unix_connect(a)),
        105 => text(t.unix_listen(a)),
        106 => text(t.unix_accept(int(a)?)),
        107 => send(t, a, b),
        108 => t.unix_recv(int(a)?, int(&bs())?),
        109 => Ok(lines(t.unix_take_fds(int(a)?))),
        110 => Ok(if t.unix_poll(int(a)?, int(&bs())?)? { b"1".to_vec() } else { b"0".to_vec() }),
        111 => t.unix_wait(&ints(a)?, int(&bs())?).map(lines),
        112 => unit(t.unix_close(int(a)?)),
        113 => text(t.shm_create(int(a)?)),
        114 => unit(t.shm_resize(int(a)?, int(&bs())?)),
        115 => {
            let (fd, offset) = pair(a)?;
            unit(t.shm_write(fd, offset, b))
        }
        116 => {
            let (offset, len) = pair(&bs())?;
            t.shm_read(int(a)?, offset, len)
        }
        _ => Err(format!("unknown net op {op}")),
    }
}
