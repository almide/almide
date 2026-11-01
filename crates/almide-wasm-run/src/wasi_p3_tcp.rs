// `include!`d part of wasi_p3.rs (#3620, the net/tcp design step 4): TCP on
// the WASI 0.3 component over wasi:sockets@0.3.0 `tcp-socket`, the ops
// 91..=103 the embedded host serves with native's net core in the interim.
// The same guest bodies (stdlib/net_wasm.almd) call the same ops; this shim
// answers what native's `net_core` answers (C-377, C-378, C-379):
//
// - streams and listeners are two guest-side tables numbered from 0, the
//   lowest free slot reused after a close, as native's are;
// - a stream's bytes ride the `stream<u8>` pair `receive` / `send` hand
//   over, opened on first use; a read answers what one copy delivers (at
//   most 64 KiB, native's read chunk), an empty answer at end of stream, and
//   the receive future's `error-code` when the peer reset it;
// - a timeout (`tcp_set_timeout`, `tcp_read_timeout`) is guest-enforced
//   (WASI has no SO_RCVTIMEO): the copy races an async-lowered
//   `monotonic-clock.wait-for` in a waitable set, and a timer win cancels the
//   copy with the baseline sync `stream.cancel-read` / `cancel-write`;
// - `tcp_available` fills a guest-side lookahead buffer (≤ 64 KiB, the Q4
//   ruling) with whatever one copy delivers without waiting — the copy is
//   cancelled if it would block — and later reads drain it first;
// - every err text is the net core's (`NetTexts`), the class from the
//   `error-code` case by name.
//
// Where it blocks: connect is a sync lower of the async func, accept waits
// for the next client on the listener's stream, and a read or write without
// a timeout waits for its copy — each holds the run task as native's call
// holds its thread. `fan` arms run in turn on this component (op 74 answers
// "not served"), and the serve export imports no sockets.

/// The tcp family's ops, in net_host.rs's order. `tcp_read_exact` (op 95)
/// has no guest body (io.read_exact loops io.read), so the shim leaves it to
/// the defined refusal and `serves` does not claim it.
const OP_TCP_LISTEN: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::TCP, 0);
const OP_TCP_ACCEPT: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::TCP, 1);
const OP_TCP_CONNECT: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::TCP, 2);
const OP_TCP_READ: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::TCP, 3);
const OP_TCP_READ_EXACT: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::TCP, 4);
const OP_TCP_READ_TIMEOUT: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::TCP, 5);
const OP_TCP_WRITE: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::TCP, 6);
const OP_TCP_AVAILABLE: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::TCP, 7);
const OP_TCP_SET_TIMEOUT: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::TCP, 8);
const OP_TCP_CLOSE: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::TCP, 9);
const OP_TCP_CLOSE_LISTENER: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::TCP, 10);
const OP_TCP_IS_OPEN: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::TCP, 11);
const OP_TCP_LOCAL_PORT: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::TCP, 12);

/// Whether the p3 shim serves tcp op `op`.
pub(crate) fn tcp_served(op: i32) -> bool {
    almide_wasm::host_ops::TCP.contains(&op) && op != OP_TCP_READ_EXACT
}

/// The most one read answers: native's `ALMIDE_NET_READ_CHUNK`, which is
/// also the lookahead's capacity and `tcp_available`'s ceiling.
const TCP_CHUNK: i32 = 64 * 1024;

/// A stream slot: the socket (-1 once closed), the receive pair (-1 until
/// first read), the send pair (-1 until first write), the end-of-stream
/// state (0 open, 1 the receive stream ended and its future is unread, 2
/// the future read), the lookahead buffer (base, offset of the first unread
/// byte, unread count) and the timeout in ms (0 = none).
const TS_SOCK: u64 = 0;
const TS_RX: u64 = 4;
const TS_RXF: u64 = 8;
const TS_TX: u64 = 12;
const TS_TXF: u64 = 16;
const TS_EOF: u64 = 20;
const TS_LOOK: u64 = 24;
const TS_LOFF: u64 = 28;
const TS_LLEN: u64 = 32;
const TS_TMO: u64 = 40;
const TS_STRIDE: i32 = 48;
/// A listener slot: the socket (-1 once closed) and its client stream.
const TL_SOCK: u64 = 0;
const TL_STREAM: u64 = 4;
const TL_STRIDE: i32 = 8;

/// The tcp funcs the WIT declares and the layout facts of their results.
struct TcpAbiParts {
    f_create: wit_parser::Function,
    f_bind: wit_parser::Function,
    f_connect: wit_parser::Function,
    f_listen: wit_parser::Function,
    f_send: wit_parser::Function,
    f_receive: wit_parser::Function,
    f_local: wit_parser::Function,
    create_payload: u64,
    unit_payload: u64,
    listen_payload: u64,
    local_payload: u64,
    recv_fields: [u64; 2],
}

type ResultParts<'a> = &'a dyn Fn(&wit_parser::Function) -> anyhow::Result<(u64, Option<wit_parser::Type>)>;
type TupleOffsets<'a> = &'a dyn Fn(wit_parser::Type) -> anyhow::Result<Vec<u64>>;

fn tcp_abi(
    _resolve: &wit_parser::Resolve,
    types: &wit_parser::Interface,
    result_parts: ResultParts<'_>,
    tuple_offsets: TupleOffsets<'_>,
) -> anyhow::Result<TcpAbiParts> {
    let func = |name: &str| -> anyhow::Result<wit_parser::Function> {
        types.functions.get(name).cloned().ok_or_else(|| anyhow::anyhow!("wasi:sockets {name} not found"))
    };
    let f_create = func("[static]tcp-socket.create")?;
    let f_bind = func("[method]tcp-socket.bind")?;
    let f_connect = func("[method]tcp-socket.connect")?;
    let f_listen = func("[method]tcp-socket.listen")?;
    let f_send = func("[method]tcp-socket.send")?;
    let f_receive = func("[method]tcp-socket.receive")?;
    let f_local = func("[method]tcp-socket.get-local-address")?;
    let (create_payload, _) = result_parts(&f_create)?;
    let (unit_payload, _) = result_parts(&f_bind)?;
    anyhow::ensure!(result_parts(&f_connect)?.0 == unit_payload, "connect's result layout differs from bind's");
    let (listen_payload, _) = result_parts(&f_listen)?;
    let (local_payload, _) = result_parts(&f_local)?;
    let rt = f_receive.result.ok_or_else(|| anyhow::anyhow!("receive answers nothing"))?;
    let rf = tuple_offsets(rt)?;
    anyhow::ensure!(rf.len() == 2, "receive's tuple drifted");
    Ok(TcpAbiParts {
        f_create,
        f_bind,
        f_connect,
        f_listen,
        f_send,
        f_receive,
        f_local,
        create_payload,
        unit_payload,
        listen_payload,
        local_payload,
        recv_fields: [rf[0], rf[1]],
    })
}

/// The tcp family's text pieces, cut out of what the net core answers for
/// marker operands (the `NetTexts` doctrine): the connect / listen heads'
/// openings, the stream and listener handle texts, the negative-length and
/// non-positive-timeout texts, and the io-failure heads; plus the
/// `error-code` discriminants the shim names itself (a timer win, a send
/// stream the host dropped without an error, an accept the host could not
/// complete).
struct TcpTexts {
    connect_open: NetPiece,
    listen_open: NetPiece,
    invalid_stream: [NetPiece; 2],
    closed_stream: [NetPiece; 2],
    invalid_listener: [NetPiece; 2],
    closed_listener: [NetPiece; 2],
    read_negative: [NetPiece; 2],
    rt_negative: [NetPiece; 2],
    rt_not_positive: [NetPiece; 2],
    accept_head: NetPiece,
    read_head: NetPiece,
    rt_head: NetPiece,
    write_head: NetPiece,
    local_head: NetPiece,
    ec_timeout: i32,
    ec_broken: i32,
    ec_other: i32,
}

impl TcpTexts {
    fn cut(put: &mut dyn FnMut(&[u8]) -> NetPiece, abi: &NetAbi) -> Self {
        use almide_rt_core::net_core as nc;
        let split2 = |text: &str, mark: &str| -> (String, String) {
            let (a, b) = text.split_once(mark).unwrap_or_else(|| panic!("net text {text:?} lost its marker {mark:?}"));
            (a.to_string(), b.to_string())
        };
        let num = NET_MARK_NUM.to_string();
        let mut t = nc::AlmideNetTables::default();
        // `tcp_connect(<host>:<port>)` / `tcp_listen(...)`: the port-range
        // refusal comes before any transport.
        let open = |r: Result<i64, String>, what: &str| -> String {
            let e = r.expect_err(what);
            split2(&e, NET_MARK_HOST).0
        };
        let connect_open = open(t.tcp_connect(NET_MARK_HOST, 70_000), "tcp_connect's port range");
        let listen_open = open(t.tcp_listen(NET_MARK_HOST, 70_000), "tcp_listen's port range");
        // An empty table's handles: `tcp_read` checks its length first, so
        // a length of 1 reaches the stream check.
        let inv_s = t.tcp_read(NET_MARK_NUM, 1).expect_err("an empty table's stream");
        let inv_l = t.tcp_local_port(NET_MARK_NUM).expect_err("an empty table's listener");
        let kind = |inv: &str| inv.strip_prefix("invalid ").and_then(|k| k.strip_suffix(&format!(" handle: {num}"))).map(str::to_string);
        let ks = kind(&inv_s).expect("the stream kind");
        let kl = kind(&inv_l).expect("the listener kind");
        let neg = t.tcp_read(0, -NET_MARK_NUM).expect_err("tcp_read's negative length");
        let (neg_pre, neg_post) = split2(&neg, &format!("-{num}"));
        let read_head = neg_pre.split_once(':').map(|(h, _)| h.to_string()).expect("tcp_read's head");
        let tmo = nc::net_err_timeout_not_positive(NET_MARK_NUM);
        let (tmo_pre, tmo_post) = split2(&tmo, &num);
        let rt_head = tmo_pre.split_once(':').map(|(h, _)| h.to_string()).expect("tcp_read_timeout's head");
        let (rtn_pre, rtn_post) = split2(&nc::net_err_negative_len(&rt_head, -NET_MARK_NUM), &format!("-{num}"));
        let pair = |put: &mut dyn FnMut(&[u8]) -> NetPiece, (a, b): (String, String)| [put(a.as_bytes()), put(b.as_bytes())];
        let disc = |c: nc::NetErrorClass| abi.ec_class.iter().position(|x| *x == c).map(|p| p as i32).expect("an error-code case for the class");
        TcpTexts {
            connect_open: put(connect_open.as_bytes()),
            listen_open: put(listen_open.as_bytes()),
            invalid_stream: pair(&mut *put, split2(&inv_s, &num)),
            closed_stream: pair(&mut *put, split2(&nc::net_err_closed_handle(&ks, NET_MARK_NUM), &num)),
            invalid_listener: pair(&mut *put, split2(&inv_l, &num)),
            closed_listener: pair(&mut *put, split2(&nc::net_err_closed_handle(&kl, NET_MARK_NUM), &num)),
            read_negative: [put(neg_pre.as_bytes()), put(neg_post.as_bytes())],
            rt_negative: [put(rtn_pre.as_bytes()), put(rtn_post.as_bytes())],
            rt_not_positive: [put(tmo_pre.as_bytes()), put(tmo_post.as_bytes())],
            // The heads of io failures, as the core passes them.
            accept_head: put(b"tcp_accept"),
            read_head: put(read_head.as_bytes()),
            rt_head: put(rt_head.as_bytes()),
            write_head: put(b"tcp_write"),
            // No `tcp_available` head: past the handle check the lookahead
            // copy never fails; a broken stream reaches the next read.
            local_head: put(b"tcp_local_port"),
            ec_timeout: disc(nc::NetErrorClass::Timeout),
            ec_broken: disc(nc::NetErrorClass::ConnectionBroken),
            ec_other: disc(nc::NetErrorClass::Other),
        }
    }
}
