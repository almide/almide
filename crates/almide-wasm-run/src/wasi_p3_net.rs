// `include!`d part of wasi_p3.rs (#4306): UDP and name lookup on the WASI 0.3
// component over wasi:sockets@0.3.0 `udp-socket` and `ip-name-lookup` — the
// destination R-net-02 (b) names for ops 140..=145, which the embedded host
// serves with native's net core in the interim.
//
// The guest side is unchanged: stdlib/net_wasm.almd's bodies call the same
// six ops with the same operands on every wasm leg, and this shim answers
// what native's `net_core` answers:
//
// - handles are the core's table indices — the lowest free slot of ONE
//   udp table, reused after a close — kept guest-side, each slot holding the
//   `udp-socket` resource (-1 once closed);
// - an IP literal (`[...]` stripped) is parsed by Rust's `IpAddr` grammar
//   here and never reaches the resolver; only a name goes to
//   `resolve-addresses` (whose own literal grammar is WHATWG's and whose
//   grant would otherwise be needed for `127.0.0.1` too);
// - every err text is rendered from almide-rt-core's net core, executed at
//   emit time with marker operands (`NetTexts::new`), plus the classified
//   `error-code` text of `net_error_core` — one table on every leg (C-443);
// - an address is printed as Rust's `Display` prints it (RFC 5952, the
//   IPv4-mapped form as `::ffff:a.b.c.d`).
//
// Where it blocks: `send`, `receive` and `resolve-addresses` are async funcs
// lowered SYNCHRONOUSLY (the fs `open-at` precedent, baseline
// component-model-async): the run task blocks for the datagram or the
// answer, as native's call blocks its thread. On this component that is the
// whole program — `fan` arms already run sequentially here (op 74 answers
// "not served") — and the serve export imports no sockets at all
// (`check_service` refuses ops 140..=145), so no request is ever held behind
// a datagram wait.
//
// Memory: the answer is built in one grow-only buffer the shim keeps; what
// the host lowers into guest memory for a call (a datagram, an address list,
// an `other(string)` err) is dropped when the op returns by rewinding the
// bump frontier to where the op found it — nothing else allocates in
// between — so a receive loop does not grow the heap per datagram.

/// The net texts and scratch: the serve statics' slot of the park's text
/// page. A serve-shaped program never carries the net shim (`check_service`
/// refuses its ops), so the two never share an artifact.
const NET_TEXT: u64 = SERVE_TEXT;

const OP_NET_BIND: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::UDP, 0);
const OP_NET_LOCAL_PORT: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::UDP, 1);
const OP_NET_SEND_TO: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::UDP, 2);
const OP_NET_RECV_FROM: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::UDP, 3);
const OP_NET_CLOSE: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::UDP, 4);
const OP_NET_LOOKUP: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::UDP, 5);

/// The reservation (#2119) before a call whose only landing is an
/// `other(option<string>)` err text.
const NET_RESERVE: i32 = 4096;
/// The most of a datagram a receive answers: native's buffer bound
/// (net_core.rs, `n.min(65535)`).
const NET_DATAGRAM: i32 = 65535;
/// The declared ceilings before a receive (one datagram plus an err text)
/// and a lookup (the address list) lower into guest memory.
const NET_RECV_RESERVE: i32 = 65536 + NET_RESERVE;
const NET_LOOKUP_RESERVE: i32 = 65536;

/// Scratch inside the net text page: an `ip-address` element for a parsed
/// literal (and for a received sender), the flat `ip-socket-address`
/// operands, and the two IPv6 halves' groups (16 bytes + 4 for an IPv4 tail).
const NET_SCR_EL: u64 = 0;
const NET_SCR_EL2: u64 = 32;
const NET_SCR_SA: u64 = 64;
const NET_SCR_HEAD: u64 = 128;
const NET_SCR_TAIL: u64 = 160;
const NET_SCR_SPAN: u64 = 192;

/// The `error-code` cases of wasi:sockets/types, by name, and the class
/// net_error_core gives each. The class enum is declared in sockets.wit's
/// order; the shim still maps by NAME, so a reordered WIT cannot shift a
/// text onto the wrong case.
const NET_EC_NAMES: &[(&str, almide_rt_core::net_core::NetErrorClass)] = {
    use almide_rt_core::net_core::NetErrorClass as C;
    &[
        ("access-denied", C::AccessDenied),
        ("not-supported", C::NotSupported),
        ("invalid-argument", C::InvalidArgument),
        ("out-of-memory", C::OutOfMemory),
        ("timeout", C::Timeout),
        ("invalid-state", C::InvalidState),
        ("address-not-bindable", C::AddressNotBindable),
        ("address-in-use", C::AddressInUse),
        ("remote-unreachable", C::RemoteUnreachable),
        ("connection-refused", C::ConnectionRefused),
        ("connection-broken", C::ConnectionBroken),
        ("connection-reset", C::ConnectionReset),
        ("connection-aborted", C::ConnectionAborted),
        ("datagram-too-large", C::DatagramTooLarge),
        ("other", C::Other),
    ]
};

/// Canonical-ABI facts of the sockets interfaces, DERIVED from the vendored
/// WIT (the fs_abi doctrine): case indices, payload and field offsets, the
/// flat slot of every `ip-socket-address` field, and each import's core
/// signature.
struct NetAbi {
    /// `ip-address-family` discriminants: [ipv4, ipv6].
    fam: [i32; 2],
    /// `ip-address`: case discriminants [ipv4, ipv6], payload offset, size.
    ia_case: [i32; 2],
    ia_payload: u64,
    ia_size: u64,
    /// `ip-socket-address`: case discriminants, payload offset; the port and
    /// first address byte / group of each record (bytes and groups are
    /// contiguous, asserted).
    sa_case: [i32; 2],
    sa_payload: u64,
    sa4_port: u64,
    sa4_addr: u64,
    sa6_port: u64,
    sa6_addr: u64,
    /// The flat slot (0 = the discriminant) of each field: v4 [port,
    /// octets 0..4], v6 [port, flow-info, groups 0..8, scope-id], and the
    /// whole variant's flat width.
    flat4: [usize; 5],
    flat6: [usize; 11],
    flat_len: usize,
    /// Result payload offsets: create's `result<udp-socket, error-code>`,
    /// bind / send's `result<_, error-code>`, receive's (and its tuple's
    /// list and address), get-local-address's, resolve-addresses'.
    create_payload: u64,
    unit_payload: u64,
    recv_payload: u64,
    recv_list: u64,
    recv_addr: u64,
    local_payload: u64,
    lookup_payload: u64,
    /// The socket `error-code` cases' classes, in discriminant order.
    ec_class: Vec<almide_rt_core::net_core::NetErrorClass>,
    /// The imports in index order: module, name, core params, core results.
    imports: Vec<(&'static str, &'static str, Vec<ValType>, Vec<ValType>)>,
}

const NET_TYPES_IFACE: &str = "wasi:sockets/types@0.3.0";
const NET_LOOKUP_IFACE: &str = "wasi:sockets/ip-name-lookup@0.3.0";

/// The imports' index order within the net block.
const NI_CREATE: u32 = 0;
const NI_BIND: u32 = 1;
const NI_SEND: u32 = 2;
const NI_RECEIVE: u32 = 3;
const NI_LOCAL: u32 = 4;
const NI_DROP: u32 = 5;
const NI_RESOLVE: u32 = 6;
const NET_IMPORTS: u32 = 7;

fn net_abi(resolve: &wit_parser::Resolve) -> anyhow::Result<NetAbi> {
    use wit_parser::{Type, TypeDefKind};
    let (_, pkg) = resolve
        .packages
        .iter()
        .find(|(_, p)| p.name.namespace == "wasi" && p.name.name == "sockets")
        .ok_or_else(|| anyhow::anyhow!("wasi:sockets package not in the resolve"))?;
    let iface_of = |name: &str| -> anyhow::Result<&wit_parser::Interface> {
        let id = pkg.interfaces.get(name).ok_or_else(|| anyhow::anyhow!("wasi:sockets/{name} not found"))?;
        Ok(&resolve.interfaces[*id])
    };
    let types = iface_of("types")?;
    let lookup = iface_of("ip-name-lookup")?;
    let mut sa = wit_parser::SizeAlign::default();
    sa.fill(resolve)?;
    let size = |t: &Type| sa.size(t).size_wasm32() as u64;
    // Follow `type x = y` aliases to the definition.
    fn def_of(resolve: &wit_parser::Resolve, mut t: Type) -> &TypeDefKind {
        loop {
            match t {
                Type::Id(id) => match &resolve.types[id].kind {
                    TypeDefKind::Type(inner) => t = *inner,
                    k => return k,
                },
                _ => panic!("a primitive where the sockets WIT has a definition"),
            }
        }
    }
    fn variant_of(resolve: &wit_parser::Resolve, t: Type) -> anyhow::Result<&wit_parser::Variant> {
        match def_of(resolve, t) {
            TypeDefKind::Variant(v) => Ok(v),
            k => Err(anyhow::anyhow!("expected a variant, got {k:?}")),
        }
    }
    let def = |t: Type| def_of(resolve, t);
    let variant = |t: Type| variant_of(resolve, t);
    let named = |name: &str| -> anyhow::Result<Type> {
        types.types.get(name).map(|id| Type::Id(*id)).ok_or_else(|| anyhow::anyhow!("wasi:sockets/types {name} not found"))
    };
    let case = |v: &wit_parser::Variant, name: &str| -> anyhow::Result<i32> {
        v.cases.iter().position(|c| c.name == name).map(|p| p as i32).ok_or_else(|| anyhow::anyhow!("case {name} not found"))
    };
    let payload = |v: &wit_parser::Variant| sa.payload_offset(v.tag(), v.cases.iter().map(|c| c.ty.as_ref())).size_wasm32() as u64;
    let case_ty = |v: &wit_parser::Variant, name: &str| -> anyhow::Result<Type> {
        v.cases.iter().find(|c| c.name == name).and_then(|c| c.ty).ok_or_else(|| anyhow::anyhow!("case {name} has no payload"))
    };
    let tuple_offsets = |t: Type| -> anyhow::Result<Vec<u64>> {
        match def(t) {
            TypeDefKind::Tuple(tu) => Ok(sa.field_offsets(tu.types.iter()).into_iter().map(|(o, _)| o.size_wasm32() as u64).collect()),
            k => Err(anyhow::anyhow!("expected a tuple, got {k:?}")),
        }
    };
    let flat_width = |t: &Type| -> usize {
        let mut storage = [wit_parser::abi::WasmType::I32; 32];
        let mut flat = wit_parser::abi::FlatTypes::new(&mut storage);
        assert!(resolve.push_flat(t, &mut flat), "a flat shape past 32 slots");
        flat.to_vec().len()
    };
    let contiguous = |offs: &[u64], stride: u64, what: &str| -> anyhow::Result<()> {
        anyhow::ensure!(offs.iter().enumerate().all(|(k, o)| *o == offs[0] + stride * k as u64), "{what} is not contiguous");
        Ok(())
    };

    // ip-address-family.
    let fam_t = named("ip-address-family")?;
    let fam = match def(fam_t) {
        TypeDefKind::Enum(e) => {
            let at = |n: &str| e.cases.iter().position(|c| c.name == n).map(|p| p as i32).ok_or_else(|| anyhow::anyhow!("family {n}"));
            [at("ipv4")?, at("ipv6")?]
        }
        k => anyhow::bail!("ip-address-family is not an enum: {k:?}"),
    };

    // ip-address.
    let ia_t = named("ip-address")?;
    let ia = variant(ia_t)?;
    let ia_payload = payload(ia);
    contiguous(&tuple_offsets(case_ty(ia, "ipv4")?)?, 1, "ipv4-address")?;
    contiguous(&tuple_offsets(case_ty(ia, "ipv6")?)?, 2, "ipv6-address")?;
    anyhow::ensure!(tuple_offsets(case_ty(ia, "ipv4")?)?.len() == 4 && tuple_offsets(case_ty(ia, "ipv6")?)?.len() == 8);

    // ip-socket-address and its two records.
    let isa_t = named("ip-socket-address")?;
    let isa = variant(isa_t)?;
    let record = |t: Type, want: &[&str]| -> anyhow::Result<(Vec<u64>, Vec<usize>, Vec<Type>)> {
        let TypeDefKind::Record(r) = def(t) else { anyhow::bail!("a socket address is not a record") };
        let names: Vec<&str> = r.fields.iter().map(|f| f.name.as_str()).collect();
        anyhow::ensure!(names == want, "socket address fields drifted: {names:?}");
        let offs = sa.field_offsets(r.fields.iter().map(|f| &f.ty)).into_iter().map(|(o, _)| o.size_wasm32() as u64).collect();
        // The flat slot each field starts at, past the variant's discriminant.
        let mut at = 1usize;
        let mut starts = Vec::new();
        for f in &r.fields {
            starts.push(at);
            at += flat_width(&f.ty);
        }
        Ok((offs, starts, r.fields.iter().map(|f| f.ty).collect()))
    };
    let (o4, s4, t4) = record(case_ty(isa, "ipv4")?, &["port", "address"])?;
    let (o6, s6, t6) = record(case_ty(isa, "ipv6")?, &["port", "flow-info", "address", "scope-id"])?;
    contiguous(&tuple_offsets(t4[1])?, 1, "ipv4-socket-address.address")?;
    contiguous(&tuple_offsets(t6[2])?, 2, "ipv6-socket-address.address")?;
    let flat_len = flat_width(&isa_t);
    anyhow::ensure!(flat_width(&t4[1]) == 4 && flat_width(&t6[2]) == 8, "an address tuple's flat width drifted");
    let flat4 = [s4[0], s4[1], s4[1] + 1, s4[1] + 2, s4[1] + 3];
    let mut flat6 = [0usize; 11];
    flat6[0] = s6[0];
    flat6[1] = s6[1];
    for k in 0..8 {
        flat6[2 + k] = s6[2] + k;
    }
    flat6[10] = s6[3];
    anyhow::ensure!(flat4.iter().chain(flat6.iter()).all(|s| *s < flat_len), "a field's flat slot past the variant");

    // Functions and their results.
    let func = |iface: &wit_parser::Interface, name: &str| -> anyhow::Result<wit_parser::Function> {
        iface.functions.get(name).cloned().ok_or_else(|| anyhow::anyhow!("wasi:sockets {name} not found"))
    };
    let result_parts = |f: &wit_parser::Function| -> anyhow::Result<(u64, Option<Type>)> {
        let Some(rt) = f.result else { anyhow::bail!("{} has no result", f.name) };
        let TypeDefKind::Result(r) = def(rt) else { anyhow::bail!("{} answers no result<..>", f.name) };
        let off = sa.payload_offset(wit_parser::Int::U8, [r.ok.as_ref(), r.err.as_ref()]).size_wasm32() as u64;
        Ok((off, r.ok))
    };
    let f_create = func(types, "[static]udp-socket.create")?;
    let f_bind = func(types, "[method]udp-socket.bind")?;
    let f_send = func(types, "[method]udp-socket.send")?;
    let f_recv = func(types, "[method]udp-socket.receive")?;
    let f_local = func(types, "[method]udp-socket.get-local-address")?;
    let f_resolve = func(lookup, "resolve-addresses")?;
    let (create_payload, _) = result_parts(&f_create)?;
    let (unit_payload, _) = result_parts(&f_bind)?;
    anyhow::ensure!(result_parts(&f_send)?.0 == unit_payload, "send's result layout differs from bind's");
    let (recv_payload, recv_ok) = result_parts(&f_recv)?;
    let recv_fields = tuple_offsets(recv_ok.ok_or_else(|| anyhow::anyhow!("receive answers no datagram"))?)?;
    let (local_payload, _) = result_parts(&f_local)?;
    let (lookup_payload, _) = result_parts(&f_resolve)?;

    // error-code classes, by name.
    let ec = variant(named("error-code")?)?;
    let ec_class = ec
        .cases
        .iter()
        .map(|c| {
            NET_EC_NAMES
                .iter()
                .find(|(n, _)| *n == c.name)
                .map(|(_, cls)| *cls)
                .ok_or_else(|| anyhow::anyhow!("wasi:sockets error-code case {} has no class in net_error_core", c.name))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    // Core signatures: the sync lowering of each func.
    let sig = |f: &wit_parser::Function| -> anyhow::Result<(Vec<ValType>, Vec<ValType>)> {
        use wit_parser::abi::{AbiVariant, WasmType};
        let s = resolve.wasm_signature(AbiVariant::GuestImport, f);
        anyhow::ensure!(!s.indirect_params, "{} lowers its params indirectly", f.name);
        let vt = |w: &WasmType| match w {
            WasmType::I32 | WasmType::Pointer | WasmType::Length => ValType::I32,
            WasmType::I64 | WasmType::PointerOrI64 => ValType::I64,
            WasmType::F32 => ValType::F32,
            WasmType::F64 => ValType::F64,
        };
        Ok((s.params.iter().map(vt).collect(), s.results.iter().map(vt).collect()))
    };
    let mut imports = Vec::new();
    for (module, name, f) in [
        (NET_TYPES_IFACE, "[static]udp-socket.create", &f_create),
        (NET_TYPES_IFACE, "[method]udp-socket.bind", &f_bind),
        (NET_TYPES_IFACE, "[method]udp-socket.send", &f_send),
        (NET_TYPES_IFACE, "[method]udp-socket.receive", &f_recv),
        (NET_TYPES_IFACE, "[method]udp-socket.get-local-address", &f_local),
    ] {
        let (p, r) = sig(f)?;
        imports.push((module, name, p, r));
    }
    imports.push((NET_TYPES_IFACE, "[resource-drop]udp-socket", vec![ValType::I32], vec![]));
    let (p, r) = sig(&f_resolve)?;
    imports.push((NET_LOOKUP_IFACE, "resolve-addresses", p, r));
    assert_eq!(imports.len() as u32, NET_IMPORTS);
    // The shapes the shim pushes: create (family, retptr); bind (self, the
    // address, retptr); send (self, data ptr/len, the option's
    // discriminant, the address, retptr); receive / get-local-address
    // (self, retptr); resolve (name ptr/len, retptr) — all into a retptr,
    // none answering a flat value.
    let want = [2, 2 + flat_len, 5 + flat_len, 2, 2, 1, 3];
    for ((_, n, p, r), w) in imports.iter().zip(want) {
        anyhow::ensure!(p.len() == w && p.iter().all(|t| *t == ValType::I32) && r.is_empty(), "{n}'s core signature drifted");
    }

    Ok(NetAbi {
        fam,
        ia_case: [case(ia, "ipv4")?, case(ia, "ipv6")?],
        ia_payload,
        ia_size: size(&ia_t),
        sa_case: [case(isa, "ipv4")?, case(isa, "ipv6")?],
        sa_payload: payload(isa),
        sa4_port: o4[0],
        sa4_addr: o4[1],
        sa6_port: o6[0],
        sa6_addr: o6[2],
        flat4,
        flat6,
        flat_len,
        create_payload,
        unit_payload,
        recv_payload,
        recv_list: recv_fields[0],
        recv_addr: recv_fields[1],
        local_payload,
        lookup_payload,
        ec_class,
        imports,
    })
}

/// The net block's import indices (present when the op set names an op of
/// the udp family): `base` is the first.
#[derive(Clone, Copy)]
struct NetImports {
    base: u32,
}

impl NetImports {
    fn plan(host_ops: &[i32], base: u32) -> (Option<Self>, u32) {
        if host_ops.iter().any(|op| almide_wasm::host_ops::UDP.contains(op)) {
            (Some(NetImports { base }), base + NET_IMPORTS)
        } else {
            (None, base)
        }
    }

    fn at(self, k: u32) -> u32 {
        self.base + k
    }

    fn import_list(self, abi: &NetAbi, types: &mut Vec<(Vec<ValType>, Vec<ValType>)>) -> Vec<(u32, &'static str, &'static str, u32)> {
        abi.imports
            .iter()
            .enumerate()
            .map(|(k, (m, n, p, r))| (self.base + k as u32, *m, *n, type_index(types, p, r)))
            .collect()
    }
}

/// A text piece's absolute address and length.
#[derive(Clone, Copy, Default)]
struct NetPiece {
    at: i32,
    len: i32,
}

/// The texts the net shim renders from, laid out at `park + NET_TEXT` past
/// the scratch. Every piece is cut out of what almide-rt-core's net core
/// ANSWERS for marker operands at emit time — the code native runs — so a
/// changed text in the core changes this shim with it.
struct NetTexts {
    blob: Vec<u8>,
    base: u64,
    /// `udp_bind(` / `udp_send_to(` / `lookup_host(`: a head's opening, then
    /// `:` between the host and the port, `)` after the port.
    bind_open: NetPiece,
    send_open: NetPiece,
    lookup_open: NetPiece,
    colon: NetPiece,
    close: NetPiece,
    /// `: port out of range 0..=65535` after a head.
    port_range: NetPiece,
    /// `: cannot resolve host "` and `"` around the escaped host.
    unres: [NetPiece; 2],
    /// `: ` between a head and a class text.
    class_sep: NetPiece,
    /// The io-failure heads of the calls whose err carries no operand.
    local_head: NetPiece,
    send_head: NetPiece,
    recv_head: NetPiece,
    /// `invalid UdpSocket handle: ` / `UdpSocket handle `, ` is closed`
    /// around the handle, and `udp_recv_from: negative length ` before it.
    invalid: [NetPiece; 2],
    closed: [NetPiece; 2],
    negative: [NetPiece; 2],
    /// `::ffff:` — Rust's IPv4-mapped spelling.
    mapped: NetPiece,
    /// One (address, length) entry per `error-code` case: its class text.
    ec_table: i32,
}

/// Markers no text of the core contains: a private-use host and handle /
/// port / length numbers no test reaches.
const NET_MARK_HOST: &str = "\u{e000}";
const NET_MARK_HOST2: &str = "\u{e001}";
const NET_MARK_NUM: i64 = 7_654_321;

impl NetTexts {
    fn new(park: u64, abi: &NetAbi) -> Self {
        use almide_rt_core::net_core as nc;
        let split2 = |text: &str, mark: &str| -> (String, String) {
            let (a, b) = text.split_once(mark).unwrap_or_else(|| panic!("net text {text:?} lost its marker {mark:?}"));
            (a.to_string(), b.to_string())
        };
        let num = NET_MARK_NUM.to_string();
        let big = 70_000i64; // past 0..=65535: the port-range text, no transport touched
        let bigs = big.to_string();
        let mut t = nc::AlmideNetTables::default();
        // `udp_bind(<host>:<port>): port out of range 0..=65535`.
        let bind = t.udp_bind(NET_MARK_HOST, big).expect_err("udp_bind's port range");
        let range_tail = nc::net_err_port_range("");
        let (bind_open, rest) = split2(&bind, NET_MARK_HOST);
        let rest = rest.strip_suffix(&range_tail).expect("udp_bind's range text ends with the core's tail").to_string();
        let (colon, close) = split2(&rest, &bigs);
        // `udp_send_to(<host>:<port>)`: the same head shape.
        let send = t.udp_send_to(0, b"", NET_MARK_HOST, big).expect_err("udp_send_to's port range");
        let (send_open, srest) = split2(&send, NET_MARK_HOST);
        assert_eq!(srest, format!("{colon}{bigs}{close}{range_tail}"), "udp_send_to's head differs from udp_bind's");
        // `udp_recv_from: negative length <n>`.
        let neg = t.udp_recv_from(0, -NET_MARK_NUM).expect_err("udp_recv_from's negative length");
        let (neg_pre, neg_post) = split2(&neg, &format!("-{num}"));
        let recv_head = neg_pre.split_once(':').map(|(h, _)| h.to_string()).expect("udp_recv_from's head");
        // `invalid UdpSocket handle: <h>` from an empty table; the closed text
        // names the same kind.
        let inv = t.udp_local_port(NET_MARK_NUM).expect_err("an empty table's handle");
        let (inv_pre, inv_post) = split2(&inv, &num);
        let kind = inv_pre.strip_prefix("invalid ").and_then(|k| k.strip_suffix(" handle: ")).expect("the handle kind");
        let (cl_pre, cl_post) = split2(&nc::net_err_closed_handle(kind, NET_MARK_NUM), &num);
        // `<f>: cannot resolve host "<host>"`.
        let unres = nc::net_err_unresolvable(NET_MARK_HOST, NET_MARK_HOST2);
        let (u0, urest) = split2(&unres, NET_MARK_HOST);
        assert!(u0.is_empty(), "the unresolvable text starts with its head");
        let (u_mid, u_post) = split2(&urest, NET_MARK_HOST2);
        // `<head>: <class text>`.
        let io = nc::net_io_error_text(NET_MARK_HOST, &std::io::Error::from(std::io::ErrorKind::ConnectionRefused));
        let (_, io_rest) = split2(&io, NET_MARK_HOST);
        let class_sep = io_rest
            .strip_suffix(nc::net_class_text(nc::NetErrorClass::ConnectionRefused))
            .expect("an io err ends with its class text")
            .to_string();
        // The heads of the io failures without an operand: the call's own
        // name, as the core passes it (`udp_local_port` names no operand, so
        // its name is spelled from the open-head family's).
        let send_head = send_open.strip_suffix('(').expect("udp_send_to's head opens with `(`").to_string();
        let local_head = "udp_local_port".to_string();
        assert!(bind_open.starts_with("udp_") && send_head.starts_with("udp_"), "the udp heads' family");

        let base = park + NET_TEXT;
        let mut blob: Vec<u8> = vec![0; NET_SCR_SPAN as usize];
        let mut put = |bytes: &[u8]| -> NetPiece {
            let at = (base + blob.len() as u64) as i32;
            blob.extend_from_slice(bytes);
            NetPiece { at, len: bytes.len() as i32 }
        };
        let pieces = NetTexts {
            bind_open: put(bind_open.as_bytes()),
            send_open: put(send_open.as_bytes()),
            lookup_open: put(b"lookup_host("),
            colon: put(colon.as_bytes()),
            close: put(close.as_bytes()),
            port_range: put(range_tail.as_bytes()),
            unres: [put(u_mid.as_bytes()), put(u_post.as_bytes())],
            class_sep: put(class_sep.as_bytes()),
            local_head: put(local_head.as_bytes()),
            send_head: put(send_head.as_bytes()),
            recv_head: put(recv_head.as_bytes()),
            invalid: [put(inv_pre.as_bytes()), put(inv_post.as_bytes())],
            closed: [put(cl_pre.as_bytes()), put(cl_post.as_bytes())],
            negative: [put(neg_pre.as_bytes()), put(neg_post.as_bytes())],
            mapped: put(b"::ffff:"),
            ec_table: 0,
            blob: Vec::new(),
            base,
        };
        // The class texts, then the table of (address, length) per case.
        let texts: Vec<NetPiece> = abi.ec_class.iter().map(|c| put(nc::net_class_text(*c).as_bytes())).collect();
        while (base + blob.len() as u64) % 4 != 0 {
            blob.push(0);
        }
        let ec_table = (base + blob.len() as u64) as i32;
        for p in texts {
            blob.extend_from_slice(&p.at.to_le_bytes());
            blob.extend_from_slice(&p.len.to_le_bytes());
        }
        assert!(NET_TEXT + blob.len() as u64 <= crate::wasi::PARK_SPAN, "the net texts overrun the park");
        NetTexts { blob, ec_table, ..pieces }
    }

    fn scratch(&self, off: u64) -> i32 {
        (self.base + off) as i32
    }
}

/// The net shim's globals: the udp slot table (base, count, capacity), the
/// answer buffer (base, capacity) and the address list the last resolve
/// left.
#[derive(Clone, Copy)]
struct NetGlobals {
    tab: u32,
    n: u32,
    cap: u32,
    buf: u32,
    bufcap: u32,
    addrs: u32,
}

impl NetGlobals {
    const COUNT: u32 = 6;

    fn at(first: u32) -> Self {
        NetGlobals { tab: first, n: first + 1, cap: first + 2, buf: first + 3, bufcap: first + 4, addrs: first + 5 }
    }

    fn emit(globals: &mut GlobalSection) {
        let t = GlobalType { val_type: ValType::I32, mutable: true, shared: false };
        for _ in 0..Self::COUNT {
            globals.global(t, &ConstExpr::i32_const(0));
        }
    }
}

/// The net shim's helper functions, by index: the shim itself (the fs_call
/// ABI), then `$esc`, `$dec`, `$int`, `$v4`, `$side`, `$ip_parse`,
/// `$ip_fmt`, `$sa`, `$resolve`.
#[derive(Clone, Copy)]
struct NetFns {
    shim: u32,
    esc: u32,
    dec: u32,
    int: u32,
    v4: u32,
    side: u32,
    ip_parse: u32,
    ip_fmt: u32,
    sa: u32,
    resolve: u32,
}

impl NetFns {
    const COUNT: u32 = 10;

    fn at(first: u32) -> Self {
        NetFns {
            shim: first,
            esc: first + 1,
            dec: first + 2,
            int: first + 3,
            v4: first + 4,
            side: first + 5,
            ip_parse: first + 6,
            ip_fmt: first + 7,
            sa: first + 8,
            resolve: first + 9,
        }
    }

    /// The helpers' core types (registered before the type section is
    /// written), in `NetFns` order past the shim.
    fn types(types: &mut Vec<(Vec<ValType>, Vec<ValType>)>) -> Vec<u32> {
        use ValType::{I32, I64};
        [
            (&[I32, I32, I32][..], &[I32][..]), // esc (dst, src, len) -> dst
            (&[I32, I64], &[I32]),              // dec (dst, v) -> dst
            (&[I32, I32], &[I64]),              // int (ptr, len) -> v
            (&[I32, I32, I32], &[I32]),         // v4 (ptr, len, out) -> ok
            (&[I32, I32, I32], &[I32]),         // side (ptr, len, tmp) -> count | v4<<8, -1
            (&[I32, I32, I32], &[I32]),         // ip_parse (ptr, len, el) -> 0 v4 / 1 v6 / -1
            (&[I32, I32], &[I32]),              // ip_fmt (dst, el) -> dst
            (&[I32, I32], &[]),                 // sa (el, port) -> () into NET_SCR_SA
            (&[I32, I32], &[I32]),              // resolve (host ptr, len) -> count
        ]
        .into_iter()
        .map(|(p, r)| type_index(types, p, r))
        .collect()
    }

    /// The function-section entries, in index order.
    fn declare(functions: &mut FunctionSection, t_fs: u32, helper_types: &[u32]) {
        assert_eq!(helper_types.len() as u32 + 1, Self::COUNT);
        functions.function(t_fs);
        for t in helper_types {
            functions.function(*t);
        }
    }
}

fn net_m8(off: u64) -> MemArg {
    MemArg { offset: off, align: 0, memory_index: 0 }
}
fn net_m16(off: u64) -> MemArg {
    MemArg { offset: off, align: 1, memory_index: 0 }
}
fn net_m32(off: u64) -> MemArg {
    MemArg { offset: off, align: 2, memory_index: 0 }
}

type Sink<'a> = wasm_encoder::InstructionSink<'a>;

/// `w = w + byte`.
fn net_put_byte(i: &mut Sink<'_>, w: u32, byte: u8) {
    i.local_get(w).i32_const(byte as i32).i32_store8(net_m8(0));
    i.local_get(w).i32_const(1).i32_add().local_set(w);
}

/// `w = w + piece`.
fn net_put(i: &mut Sink<'_>, w: u32, p: NetPiece) {
    if p.len == 0 {
        return;
    }
    i.local_get(w).i32_const(p.at).i32_const(p.len).memory_copy(0, 0);
    i.local_get(w).i32_const(p.len).i32_add().local_set(w);
}

/// `(dst, src, len) -> dst_end`: `src[..len]` escaped as
/// `almide_rt_errtext_escape` escapes it (C-418): `\n` `\r` `\t` `\0`, every
/// other character of the rule's table as `\u{<lowercase hex>}`, every other
/// byte as is — the quote and the backslash included (http's `$http_quote`
/// escapes those too; the net heads do not).
fn net_fn_esc() -> Function {
    let (dst, src, len) = (0u32, 1u32, 2u32);
    let (k, b, cp, m, sh) = (3u32, 4u32, 5u32, 6u32, 7u32);
    let mut f = Function::new([(5, ValType::I32)]);
    let mut i = f.instructions();
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).local_get(len).i32_ge_u().br_if(1);
    i.local_get(src).local_get(k).i32_add().i32_load8_u(net_m8(0)).local_set(b);
    i.block(BlockType::Empty);
    for (byte, esc) in [(b'\t', b't'), (b'\r', b'r'), (b'\n', b'n'), (0u8, b'0')] {
        i.local_get(b).i32_const(byte as i32).i32_eq().if_(BlockType::Empty);
        net_put_byte(&mut i, dst, b'\\');
        net_put_byte(&mut i, dst, esc);
        i.br(1);
        i.end();
    }
    i.local_get(b).i32_const(0x20).i32_lt_u();
    i.local_get(b).i32_const(0x7f).i32_eq();
    i.i32_or().if_(BlockType::Empty);
    for c in *b"\\u{" {
        net_put_byte(&mut i, dst, c);
    }
    i.local_get(b).i32_const(16).i32_ge_u().if_(BlockType::Empty);
    net_hex_digit(&mut i, b, 4, dst);
    i.end();
    net_hex_digit(&mut i, b, 0, dst);
    net_put_byte(&mut i, dst, b'}');
    i.br(1);
    i.end();
    let addr = |i: &mut Sink<'_>| {
        i.local_get(src).local_get(k).i32_add();
    };
    almide_wasm::emit_errtext_multibyte(&mut i, &addr, (k, len, b, dst), (cp, m, sh), 1);
    i.local_get(dst).local_get(b).i32_store8(net_m8(0));
    i.local_get(dst).i32_const(1).i32_add().local_set(dst);
    i.end();
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    i.local_get(dst);
    i.end();
    f
}

/// Write the lowercase hex digit of `(v >> shift) & 15` at `w`, `w += 1`.
fn net_hex_digit(i: &mut Sink<'_>, v: u32, shift: i32, w: u32) {
    i.local_get(w);
    i.local_get(v).i32_const(shift).i32_shr_u().i32_const(15).i32_and();
    i.local_get(v).i32_const(shift).i32_shr_u().i32_const(15).i32_and().i32_const(10).i32_lt_u();
    i.if_(BlockType::Result(ValType::I32)).i32_const('0' as i32).else_().i32_const('a' as i32 - 10).end();
    i.i32_add().i32_store8(net_m8(0));
    i.local_get(w).i32_const(1).i32_add().local_set(w);
}

/// `(dst, v) -> dst_end`: the signed decimal of `v`, as Rust's `{}` writes
/// an i64 (`-` then the magnitude; i64::MIN's magnitude read unsigned).
fn net_fn_dec() -> Function {
    let (dst, v) = (0u32, 1u32);
    let (n, j) = (2u32, 3u32);
    let (u, t) = (4u32, 5u32);
    let mut f = Function::new([(2, ValType::I32), (2, ValType::I64)]);
    let mut i = f.instructions();
    i.local_get(v).local_set(u);
    i.local_get(v).i64_const(0).i64_lt_s().if_(BlockType::Empty);
    net_put_byte(&mut i, dst, b'-');
    i.i64_const(0).local_get(v).i64_sub().local_set(u);
    i.end();
    i.i32_const(1).local_set(n);
    i.local_get(u).local_set(t);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(t).i64_const(10).i64_lt_u().br_if(1);
    i.local_get(t).i64_const(10).i64_div_u().local_set(t);
    i.local_get(n).i32_const(1).i32_add().local_set(n);
    i.br(0).end().end();
    i.local_get(n).local_set(j);
    i.local_get(u).local_set(t);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).i32_eqz().br_if(1);
    i.local_get(j).i32_const(1).i32_sub().local_set(j);
    i.local_get(dst).local_get(j).i32_add();
    i.local_get(t).i64_const(10).i64_rem_u().i32_wrap_i64().i32_const('0' as i32).i32_add();
    i.i32_store8(net_m8(0));
    i.local_get(t).i64_const(10).i64_div_u().local_set(t);
    i.br(0).end().end();
    i.local_get(dst).local_get(n).i32_add();
    i.end();
    f
}

/// `(ptr, len) -> v`: the decimal net_wasm.almd writes (`int.to_string`:
/// an optional `-`, then digits).
fn net_fn_int() -> Function {
    let (ptr, len) = (0u32, 1u32);
    let (k, neg) = (2u32, 3u32);
    let v = 4u32;
    let mut f = Function::new([(2, ValType::I32), (1, ValType::I64)]);
    let mut i = f.instructions();
    i.local_get(len).i32_const(0).i32_gt_s().if_(BlockType::Empty);
    i.local_get(ptr).i32_load8_u(net_m8(0)).i32_const('-' as i32).i32_eq().if_(BlockType::Empty);
    i.i32_const(1).local_set(neg).i32_const(1).local_set(k);
    i.end();
    i.end();
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).local_get(len).i32_ge_u().br_if(1);
    i.local_get(v).i64_const(10).i64_mul();
    i.local_get(ptr).local_get(k).i32_add().i32_load8_u(net_m8(0)).i32_const('0' as i32).i32_sub().i64_extend_i32_u();
    i.i64_add().local_set(v);
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    i.local_get(neg).if_(BlockType::Result(ValType::I64));
    i.i64_const(0).local_get(v).i64_sub();
    i.else_();
    i.local_get(v);
    i.end();
    i.end();
    f
}

/// `(ptr, len, out) -> ok`: Rust's `Ipv4Addr` grammar — exactly four parts
/// separated by `.`, each one to three ASCII digits, at most 255, no leading
/// zero unless the part is `0` — the four bytes at `out`.
fn net_fn_v4() -> Function {
    let (ptr, len, out) = (0u32, 1u32, 2u32);
    let (k, part, digits, val, first, c) = (3u32, 4u32, 5u32, 6u32, 7u32, 8u32);
    let mut f = Function::new([(6, ValType::I32)]);
    let mut i = f.instructions();
    i.block(BlockType::Empty).loop_(BlockType::Empty); // parts
    i.i32_const(0).local_set(digits);
    i.i32_const(0).local_set(val);
    i.i32_const(0).local_set(first);
    i.block(BlockType::Empty).loop_(BlockType::Empty); // digits
    i.local_get(k).local_get(len).i32_ge_u().br_if(1);
    i.local_get(ptr).local_get(k).i32_add().i32_load8_u(net_m8(0)).i32_const('0' as i32).i32_sub().local_tee(c);
    i.i32_const(9).i32_gt_u().br_if(1);
    i.local_get(digits).i32_eqz().if_(BlockType::Empty);
    i.local_get(c).i32_const(1).i32_add().local_set(first); // 1 = a leading `0`
    i.end();
    // A fourth digit already fails; stop the value from wrapping.
    i.local_get(digits).i32_const(3).i32_lt_u().if_(BlockType::Empty);
    i.local_get(val).i32_const(10).i32_mul().local_get(c).i32_add().local_set(val);
    i.end();
    i.local_get(digits).i32_const(1).i32_add().local_set(digits);
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    // digits in 1..=3, val <= 255, and a leading zero only alone.
    i.local_get(digits).i32_eqz();
    i.local_get(digits).i32_const(3).i32_gt_u().i32_or();
    i.local_get(val).i32_const(255).i32_gt_u().i32_or();
    i.local_get(first).i32_const(1).i32_eq().local_get(digits).i32_const(1).i32_gt_u().i32_and().i32_or();
    i.if_(BlockType::Empty).i32_const(0).return_().end();
    i.local_get(out).local_get(part).i32_add().local_get(val).i32_store8(net_m8(0));
    i.local_get(part).i32_const(1).i32_add().local_tee(part);
    i.i32_const(4).i32_eq().br_if(1);
    i.local_get(k).local_get(len).i32_lt_u().if_(BlockType::Empty);
    i.local_get(ptr).local_get(k).i32_add().i32_load8_u(net_m8(0)).i32_const('.' as i32).i32_eq().if_(BlockType::Empty);
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(2); // the next part
    i.end();
    i.end();
    i.i32_const(0).return_();
    i.end().end();
    i.local_get(k).local_get(len).i32_eq();
    i.end();
    f
}

/// `(ptr, len, tmp) -> count | v4 << 8, or -1`: one side of an IPv6 `::`
/// (or the whole address without one) as its groups at `tmp` (u16 each; an
/// IPv4 tail is two, and only the last part may be one — its bytes staged
/// at `tmp + 16`). Empty text is no groups; more than eight is -1.
fn net_fn_side(v4: u32) -> Function {
    let (ptr, len, tmp) = (0u32, 1u32, 2u32);
    let (k, cnt, end, dot, val, c, plen) = (3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32);
    let mut f = Function::new([(7, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(len).i32_eqz().if_(BlockType::Empty).i32_const(0).return_().end();
    i.loop_(BlockType::Empty); // parts
    // end = the next `:` (or len); dot = whether the part holds a `.`.
    i.local_get(k).local_set(end);
    i.i32_const(0).local_set(dot);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(end).local_get(len).i32_ge_u().br_if(1);
    i.local_get(ptr).local_get(end).i32_add().i32_load8_u(net_m8(0)).local_tee(c);
    i.i32_const(':' as i32).i32_eq().br_if(1);
    i.local_get(c).i32_const('.' as i32).i32_eq().if_(BlockType::Empty);
    i.i32_const(1).local_set(dot);
    i.end();
    i.local_get(end).i32_const(1).i32_add().local_set(end);
    i.br(0).end().end();
    i.local_get(end).local_get(k).i32_sub().local_set(plen);
    i.local_get(dot).if_(BlockType::Empty);
    // An IPv4 tail: the last part, two free groups, a valid address.
    i.local_get(end).local_get(len).i32_ne();
    i.local_get(cnt).i32_const(6).i32_gt_u().i32_or();
    i.if_(BlockType::Empty).i32_const(-1).return_().end();
    i.local_get(ptr).local_get(k).i32_add().local_get(plen).local_get(tmp).i32_const(16).i32_add().call(v4);
    i.i32_eqz().if_(BlockType::Empty).i32_const(-1).return_().end();
    for (g, hi, lo) in [(0u32, 16u64, 17u64), (1, 18, 19)] {
        i.local_get(tmp).local_get(cnt).i32_const(g as i32).i32_add().i32_const(1).i32_shl().i32_add();
        i.local_get(tmp).i32_load8_u(net_m8(hi)).i32_const(8).i32_shl();
        i.local_get(tmp).i32_load8_u(net_m8(lo)).i32_or();
        i.i32_store16(net_m16(0));
    }
    i.local_get(cnt).i32_const(2).i32_add().i32_const(256).i32_or().return_();
    i.end();
    // A hex group: one to four digits, a ninth group is -1.
    i.local_get(plen).i32_eqz();
    i.local_get(plen).i32_const(4).i32_gt_u().i32_or();
    i.local_get(cnt).i32_const(8).i32_ge_u().i32_or();
    i.if_(BlockType::Empty).i32_const(-1).return_().end();
    i.i32_const(0).local_set(val);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).local_get(end).i32_ge_u().br_if(1);
    i.local_get(ptr).local_get(k).i32_add().i32_load8_u(net_m8(0)).local_set(c);
    // c -> its hex value, or 16 for a non-hex byte.
    i.local_get(c).i32_const('0' as i32).i32_sub().i32_const(10).i32_lt_u().if_(BlockType::Result(ValType::I32));
    i.local_get(c).i32_const('0' as i32).i32_sub();
    i.else_();
    i.local_get(c).i32_const(0x20).i32_or().i32_const('a' as i32).i32_sub().i32_const(6).i32_lt_u().if_(BlockType::Result(ValType::I32));
    i.local_get(c).i32_const(0x20).i32_or().i32_const('a' as i32 - 10).i32_sub();
    i.else_();
    i.i32_const(16);
    i.end();
    i.end();
    i.local_tee(c).i32_const(15).i32_gt_u().if_(BlockType::Empty).i32_const(-1).return_().end();
    i.local_get(val).i32_const(4).i32_shl().local_get(c).i32_or().local_set(val);
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    i.local_get(tmp).local_get(cnt).i32_const(1).i32_shl().i32_add().local_get(val).i32_store16(net_m16(0));
    i.local_get(cnt).i32_const(1).i32_add().local_set(cnt);
    i.local_get(end).local_get(len).i32_eq().if_(BlockType::Empty).local_get(cnt).return_().end();
    // Past the `:`; an empty next part fails as a zero-length group.
    i.local_get(end).i32_const(1).i32_add().local_set(k);
    i.br(0);
    i.end();
    i.unreachable();
    i.end();
    f
}

/// `(ptr, len, el) -> 0 v4 / 1 v6 / -1`: Rust's `IpAddr` grammar (no `:`
/// means IPv4); the address written at `el` as an `ip-address` element. An
/// IPv6 `::` stands for at least one zero group, at most one `::` appears,
/// and an IPv4 tail ends the address — never the head before a `::`
/// (Rust's `read_ipv6_addr` refuses it there).
fn net_fn_ip_parse(a: &NetAbi, t: &NetTexts, fns: NetFns) -> Function {
    let (ptr, len, el) = (0u32, 1u32, 2u32);
    let (k, d, rh, rt, h, tl, j) = (3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32);
    let (head, tail) = (t.scratch(NET_SCR_HEAD), t.scratch(NET_SCR_TAIL));
    let mut f = Function::new([(7, ValType::I32)]);
    let mut i = f.instructions();
    // d = the first `::`, or -1; k scans for any `:`.
    i.i32_const(-1).local_set(d);
    i.i32_const(-1).local_set(k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).local_get(len).i32_ge_u().br_if(1);
    i.local_get(ptr).local_get(j).i32_add().i32_load8_u(net_m8(0)).i32_const(':' as i32).i32_eq().if_(BlockType::Empty);
    i.local_get(k).i32_const(0).i32_lt_s().if_(BlockType::Empty).local_get(j).local_set(k).end();
    i.local_get(d).i32_const(0).i32_lt_s();
    i.local_get(j).i32_const(1).i32_add().local_get(len).i32_lt_u().i32_and().if_(BlockType::Empty);
    i.local_get(ptr).local_get(j).i32_add().i32_load8_u(net_m8(1)).i32_const(':' as i32).i32_eq().if_(BlockType::Empty);
    i.local_get(j).local_set(d);
    i.local_get(j).i32_const(1).i32_add().local_set(j); // `::` is one separator
    i.end();
    i.end();
    i.end();
    i.local_get(j).i32_const(1).i32_add().local_set(j);
    i.br(0).end().end();
    // IPv4.
    i.local_get(k).i32_const(0).i32_lt_s().if_(BlockType::Empty);
    i.local_get(ptr).local_get(len).local_get(el).i32_const(a.ia_payload as i32).i32_add().call(fns.v4);
    i.if_(BlockType::Empty);
    i.local_get(el).i32_const(a.ia_case[0]).i32_store8(net_m8(0));
    i.i32_const(0).return_();
    i.end();
    i.i32_const(-1).return_();
    i.end();
    // IPv6: a second `::` (past the first) fails.
    i.local_get(d).i32_const(0).i32_ge_s().if_(BlockType::Empty);
    i.local_get(d).i32_const(2).i32_add().local_set(j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).i32_const(1).i32_add().local_get(len).i32_ge_u().br_if(1);
    i.local_get(ptr).local_get(j).i32_add().i32_load8_u(net_m8(0)).i32_const(':' as i32).i32_eq();
    i.local_get(ptr).local_get(j).i32_add().i32_load8_u(net_m8(1)).i32_const(':' as i32).i32_eq().i32_and();
    i.if_(BlockType::Empty).i32_const(-1).return_().end();
    i.local_get(j).i32_const(1).i32_add().local_set(j);
    i.br(0).end().end();
    i.end();
    // The eight groups start zeroed (the `::` run).
    i.local_get(el).i32_const(a.ia_payload as i32).i32_add().i32_const(0).i32_const(16).memory_fill(0);
    i.local_get(d).i32_const(0).i32_lt_s().if_(BlockType::Empty);
    // No `::`: exactly eight groups.
    i.local_get(ptr).local_get(len).i32_const(head).call(fns.side).local_tee(rh);
    i.i32_const(0).i32_lt_s();
    i.local_get(rh).i32_const(255).i32_and().i32_const(8).i32_ne().i32_or();
    i.if_(BlockType::Empty).i32_const(-1).return_().end();
    i.local_get(el).i32_const(a.ia_payload as i32).i32_add().i32_const(head).i32_const(16).memory_copy(0, 0);
    i.else_();
    // head :: tail — at most seven groups between them, no IPv4 head.
    i.local_get(ptr).local_get(d).i32_const(head).call(fns.side).local_tee(rh);
    i.i32_const(0).i32_lt_s();
    i.local_get(rh).i32_const(256).i32_and().i32_or();
    i.if_(BlockType::Empty).i32_const(-1).return_().end();
    i.local_get(ptr).local_get(d).i32_add().i32_const(2).i32_add();
    i.local_get(len).local_get(d).i32_sub().i32_const(2).i32_sub();
    i.i32_const(tail).call(fns.side).local_tee(rt);
    i.i32_const(0).i32_lt_s().if_(BlockType::Empty).i32_const(-1).return_().end();
    i.local_get(rh).i32_const(255).i32_and().local_set(h);
    i.local_get(rt).i32_const(255).i32_and().local_set(tl);
    i.local_get(h).local_get(tl).i32_add().i32_const(7).i32_gt_u().if_(BlockType::Empty).i32_const(-1).return_().end();
    i.local_get(el).i32_const(a.ia_payload as i32).i32_add().i32_const(head).local_get(h).i32_const(1).i32_shl().memory_copy(0, 0);
    i.local_get(el).i32_const(a.ia_payload as i32 + 16).i32_add().local_get(tl).i32_const(1).i32_shl().i32_sub();
    i.i32_const(tail).local_get(tl).i32_const(1).i32_shl().memory_copy(0, 0);
    i.end();
    i.local_get(el).i32_const(a.ia_case[1]).i32_store8(net_m8(0));
    i.i32_const(1);
    i.end();
    f
}

/// `(dst, el) -> dst_end`: an `ip-address` element as Rust's `Display`
/// writes it — dotted IPv4; IPv6 as `::ffff:a.b.c.d` when IPv4-mapped, else
/// lowercase groups without leading zeros, the longest run of two or more
/// zero groups (the first when tied) as `::`.
fn net_fn_ip_fmt(a: &NetAbi, t: &NetTexts, fns: NetFns) -> Function {
    let (dst, el) = (0u32, 1u32);
    let (k, g, cs, cl, bs, bl, sh, end) = (2u32, 3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32);
    let po = a.ia_payload;
    let mut f = Function::new([(8, ValType::I32)]);
    let mut i = f.instructions();
    let group = |i: &mut Sink<'_>, k: u32| {
        i.local_get(el).local_get(k).i32_const(1).i32_shl().i32_add().i32_load16_u(net_m16(po));
    };
    i.local_get(el).i32_load8_u(net_m8(0)).i32_const(a.ia_case[0]).i32_eq().if_(BlockType::Empty);
    net_dotted(&mut i, dst, fns.dec, &|i: &mut Sink<'_>, b: u64| {
        i.local_get(el).i32_load8_u(net_m8(po + b));
    });
    i.local_get(dst).return_();
    i.end();
    // IPv4-mapped: five zero groups, then 0xffff.
    i.i32_const(1);
    for gk in 0..5u64 {
        i.local_get(el).i32_load16_u(net_m16(po + 2 * gk)).i32_eqz().i32_and();
    }
    i.local_get(el).i32_load16_u(net_m16(po + 10)).i32_const(0xffff).i32_eq().i32_and();
    i.if_(BlockType::Empty);
    net_put(&mut i, dst, t.mapped);
    net_dotted(&mut i, dst, fns.dec, &|i: &mut Sink<'_>, b: u64| {
        i.local_get(el).i32_load16_u(net_m16(po + 12 + 2 * (b / 2)));
        if b % 2 == 0 {
            i.i32_const(8).i32_shr_u();
        } else {
            i.i32_const(255).i32_and();
        }
    });
    i.local_get(dst).return_();
    i.end();
    // The longest zero run: (bs, bl); the current one (cs, cl).
    i.i32_const(-1).local_set(bs);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).i32_const(8).i32_ge_u().br_if(1);
    group(&mut i, k);
    i.i32_eqz().if_(BlockType::Empty);
    i.local_get(cl).i32_eqz().if_(BlockType::Empty).local_get(k).local_set(cs).end();
    i.local_get(cl).i32_const(1).i32_add().local_tee(cl);
    i.local_get(bl).i32_gt_u().if_(BlockType::Empty);
    i.local_get(cs).local_set(bs);
    i.local_get(cl).local_set(bl);
    i.end();
    i.else_();
    i.i32_const(0).local_set(cl);
    i.end();
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    i.local_get(bl).i32_const(2).i32_lt_u().if_(BlockType::Empty);
    i.i32_const(-1).local_set(bs);
    i.end();
    // Groups 0..8, `:` between, the run (when bs >= 0) written as `::`.
    i.i32_const(0).local_set(k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).i32_const(8).i32_ge_u().br_if(1);
    i.local_get(k).local_get(bs).i32_eq().if_(BlockType::Empty);
    net_put_byte(&mut i, dst, b':');
    net_put_byte(&mut i, dst, b':');
    i.local_get(k).local_get(bl).i32_add().local_set(k);
    i.br(1); // the loop's next group
    i.end();
    // A `:` before every group but the first and the one right after a run.
    i.local_get(k).i32_const(0).i32_ne();
    i.local_get(bs).i32_const(0).i32_lt_s();
    i.local_get(k).local_get(bs).local_get(bl).i32_add().i32_ne().i32_or().i32_and();
    i.if_(BlockType::Empty);
    net_put_byte(&mut i, dst, b':');
    i.end();
    // The group's hex without leading zeros: from the highest nonzero nibble.
    group(&mut i, k);
    i.local_set(g);
    i.i32_const(12).local_set(sh);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(sh).i32_eqz().br_if(1);
    i.local_get(g).local_get(sh).i32_shr_u().br_if(1);
    i.local_get(sh).i32_const(4).i32_sub().local_set(sh);
    i.br(0).end().end();
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(g).local_get(sh).i32_shr_u().local_set(end);
    i.local_get(dst);
    i.local_get(end).i32_const(15).i32_and();
    i.local_get(end).i32_const(15).i32_and().i32_const(10).i32_lt_u();
    i.if_(BlockType::Result(ValType::I32)).i32_const('0' as i32).else_().i32_const('a' as i32 - 10).end();
    i.i32_add().i32_store8(net_m8(0));
    i.local_get(dst).i32_const(1).i32_add().local_set(dst);
    i.local_get(sh).i32_eqz().br_if(1);
    i.local_get(sh).i32_const(4).i32_sub().local_set(sh);
    i.br(0).end().end();
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    i.local_get(dst);
    i.end();
    f
}

/// Four bytes as `a.b.c.d`: `byte_at` pushes byte `b` (an i32).
fn net_dotted(i: &mut Sink<'_>, dst: u32, dec: u32, byte_at: &dyn Fn(&mut Sink<'_>, u64)) {
    for b in 0..4u64 {
        if b > 0 {
            net_put_byte(i, dst, b'.');
        }
        i.local_get(dst);
        byte_at(i, b);
        i.i64_extend_i32_u().call(dec).local_set(dst);
    }
}

/// `(el, port) -> ()`: the flat `ip-socket-address` operands of `el` at
/// `port` (flow-info and scope-id 0), one i32 per slot at NET_SCR_SA.
fn net_fn_sa(a: &NetAbi, t: &NetTexts) -> Function {
    let (el, port) = (0u32, 1u32);
    let at = t.scratch(NET_SCR_SA);
    let slot = |s: usize| (at + 4 * s as i32) as u64;
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.i32_const(at).i32_const(0).i32_const(4 * a.flat_len as i32).memory_fill(0);
    i.local_get(el).i32_load8_u(net_m8(0)).i32_const(a.ia_case[0]).i32_eq().if_(BlockType::Empty);
    i.i32_const(0).i32_const(a.sa_case[0]).i32_store(net_m32(slot(0)));
    i.i32_const(0).local_get(port).i32_store(net_m32(slot(a.flat4[0])));
    for b in 0..4 {
        i.i32_const(0).local_get(el).i32_load8_u(net_m8(a.ia_payload + b as u64)).i32_store(net_m32(slot(a.flat4[1 + b])));
    }
    i.else_();
    i.i32_const(0).i32_const(a.sa_case[1]).i32_store(net_m32(slot(0)));
    i.i32_const(0).local_get(port).i32_store(net_m32(slot(a.flat6[0])));
    for g in 0..8 {
        i.i32_const(0).local_get(el).i32_load16_u(net_m16(a.ia_payload + 2 * g as u64)).i32_store(net_m32(slot(a.flat6[2 + g])));
    }
    i.end();
    i.end();
    f
}

/// `(host, len) -> count`: the addresses `host` names, as native's
/// `almide_net_resolve` finds them — `[...]` stripped, an IP literal itself
/// (parsed here, never sent to the resolver), else `resolve-addresses`; the
/// `ip-address` list at `g.addrs`. 0 = nothing (the resolver's err included:
/// native reads every resolver failure as unresolvable).
fn net_fn_resolve(g: P3Globals, a: &NetAbi, t: &NetTexts, fns: NetFns, ng: NetGlobals, imp: NetImports) -> Function {
    let (ptr, len) = (0u32, 1u32);
    let ret = (g.park + STATRET) as i32;
    let el = t.scratch(NET_SCR_EL);
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.local_get(len).i32_const(2).i32_ge_u().if_(BlockType::Empty);
    i.local_get(ptr).i32_load8_u(net_m8(0)).i32_const('[' as i32).i32_eq();
    i.local_get(ptr).local_get(len).i32_add().i32_const(1).i32_sub().i32_load8_u(net_m8(0)).i32_const(']' as i32).i32_eq();
    i.i32_and().if_(BlockType::Empty);
    i.local_get(ptr).i32_const(1).i32_add().local_set(ptr);
    i.local_get(len).i32_const(2).i32_sub().local_set(len);
    i.end();
    i.end();
    i.local_get(ptr).local_get(len).i32_const(el).call(fns.ip_parse).i32_const(0).i32_ge_s().if_(BlockType::Empty);
    i.i32_const(el).global_set(ng.addrs);
    i.i32_const(1).return_();
    i.end();
    i.i32_const(NET_LOOKUP_RESERVE).call(g.f_reserve);
    i.local_get(ptr).local_get(len).i32_const(ret).call(imp.at(NI_RESOLVE));
    i.i32_const(ret).i32_load8_u(net_m8(0)).if_(BlockType::Empty).i32_const(0).return_().end();
    i.i32_const(ret).i32_load(net_m32(a.lookup_payload)).global_set(ng.addrs);
    i.i32_const(ret).i32_load(net_m32(a.lookup_payload + 4));
    i.end();
    f
}

/// The shim's locals, named once.
#[derive(Clone, Copy)]
struct NetLocals {
    w: u32,
    h0: u32,
    s: u32,
    k: u32,
    cnt: u32,
    el: u32,
    last: u32,
    i1: u32,
    i2: u32,
    hp: u32,
    hl: u32,
    n: u32,
    j: u32,
    h: u32,
    port: u32,
    max: u32,
}

/// `(op, a_ptr, a_len, b_ptr, b_len) -> i64` over the fs_call ABI: ops
/// 140..=145 with stdlib/net_wasm.almd's operands, answered as the
/// embedded host's net_host.rs answers them (status 0 = ok, 1 = err, the
/// answer parked for `host_read`).
fn shim_net(g: P3Globals, a: &NetAbi, t: &NetTexts, fns: NetFns, ng: NetGlobals, imp: NetImports) -> Function {
    let (op, a_ptr, a_len, b_ptr, b_len) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let l = NetLocals {
        w: 5, h0: 6, s: 7, k: 8, cnt: 9, el: 10, last: 11, i1: 12, i2: 13, hp: 14, hl: 15, n: 16, j: 17,
        h: 18, port: 19, max: 20,
    };
    let ret = (g.park + STATRET) as i32;
    let mut f = Function::new([(13, ValType::I32), (3, ValType::I64)]);
    let mut i = f.instructions();

    // The answer buffer holds any head (12 bytes per host byte covers the
    // widest escape) — and, for a receive, a whole datagram past its line.
    i.local_get(a_len).i32_const(12).i32_mul().i32_const(512).i32_add();
    i.i32_const(NET_DATAGRAM + 128).i32_const(0).local_get(op).i32_const(OP_NET_RECV_FROM).i32_eq().select();
    i.i32_add().local_set(l.n);
    net_ensure(&mut i, g, ng, l, l.n);
    i.global_get(g.heap).local_set(l.h0);
    i.global_get(ng.buf).local_set(l.w);

    // ── udp_bind(host = a, port = b) ──
    i.local_get(op).i32_const(OP_NET_BIND).i32_eq().if_(BlockType::Empty);
    i.local_get(a_ptr).local_set(l.hp);
    i.local_get(a_len).local_set(l.hl);
    i.local_get(b_ptr).local_get(b_len).call(fns.int).local_set(l.port);
    net_port_check(&mut i, g, t, fns, ng, l, t.bind_open);
    net_resolve_or_err(&mut i, g, t, fns, ng, l, t.bind_open);
    // Each address in turn; the last failure classifies the err.
    i.i32_const(0).local_set(l.k);
    i.global_get(ng.addrs).local_set(l.el);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(l.k).local_get(l.cnt).i32_ge_u().br_if(1);
    i.block(BlockType::Empty); // the next address
    i.i32_const(NET_RESERVE).call(g.f_reserve);
    i.i32_const(a.fam[0]).i32_const(a.fam[1]);
    i.local_get(l.el).i32_load8_u(net_m8(0)).i32_const(a.ia_case[0]).i32_eq().select();
    i.i32_const(ret).call(imp.at(NI_CREATE));
    i.i32_const(ret).i32_load8_u(net_m8(0)).if_(BlockType::Empty);
    i.i32_const(ret).i32_load8_u(net_m8(a.create_payload)).local_set(l.last);
    i.br(1);
    i.end();
    i.i32_const(ret).i32_load(net_m32(a.create_payload)).local_set(l.s);
    i.local_get(l.el).local_get(l.port).i32_wrap_i64().call(fns.sa);
    i.i32_const(NET_RESERVE).call(g.f_reserve);
    i.local_get(l.s);
    net_push_sa(&mut i, a, t);
    i.i32_const(ret).call(imp.at(NI_BIND));
    i.i32_const(ret).i32_load8_u(net_m8(0)).if_(BlockType::Empty);
    i.i32_const(ret).i32_load8_u(net_m8(a.unit_payload)).local_set(l.last);
    i.local_get(l.s).call(imp.at(NI_DROP));
    i.br(1);
    i.end();
    // Bound: the lowest free slot holds it.
    net_slot_put(&mut i, g, ng, l);
    i.local_get(l.w).local_get(l.j).i64_extend_i32_u().call(fns.dec).local_set(l.w);
    net_answer(&mut i, g, ng, l, 0);
    i.end();
    i.local_get(l.el).i32_const(a.ia_size as i32).i32_add().local_set(l.el);
    i.local_get(l.k).i32_const(1).i32_add().local_set(l.k);
    i.br(0).end().end();
    net_head(&mut i, t, fns, l, t.bind_open);
    net_put(&mut i, l.w, t.class_sep);
    net_put_class(&mut i, t, l, l.last);
    net_answer(&mut i, g, ng, l, 1);
    i.end();

    // ── udp_local_port(sock = a) ──
    i.local_get(op).i32_const(OP_NET_LOCAL_PORT).i32_eq().if_(BlockType::Empty);
    i.local_get(a_ptr).local_get(a_len).call(fns.int).local_set(l.h);
    net_slot_get(&mut i, g, t, fns, ng, l);
    i.i32_const(NET_RESERVE).call(g.f_reserve);
    i.local_get(l.s).i32_const(ret).call(imp.at(NI_LOCAL));
    i.i32_const(ret).i32_load8_u(net_m8(0)).if_(BlockType::Empty);
    net_put(&mut i, l.w, t.local_head);
    net_put(&mut i, l.w, t.class_sep);
    i.i32_const(ret).i32_load8_u(net_m8(a.local_payload)).local_set(l.last);
    net_put_class(&mut i, t, l, l.last);
    net_answer(&mut i, g, ng, l, 1);
    i.end();
    i.local_get(l.w);
    net_sa_port(&mut i, a, ret + a.local_payload as i32);
    i.i64_extend_i32_u().call(fns.dec).local_set(l.w);
    net_answer(&mut i, g, ng, l, 0);
    i.end();

    // ── udp_send_to(a = `<sock>\n<port>\n<host>`, b = the datagram) ──
    i.local_get(op).i32_const(OP_NET_SEND_TO).i32_eq().if_(BlockType::Empty);
    net_find_nl(&mut i, l, a_ptr, a_len, 0, l.i1);
    net_find_nl(&mut i, l, a_ptr, a_len, 1, l.i2);
    i.local_get(a_ptr).local_get(l.i1).call(fns.int).local_set(l.h);
    i.local_get(a_ptr).local_get(l.i1).i32_add().i32_const(1).i32_add();
    i.local_get(l.i2).local_get(l.i1).i32_sub().i32_const(1).i32_sub().call(fns.int).local_set(l.port);
    i.local_get(a_ptr).local_get(l.i2).i32_add().i32_const(1).i32_add().local_set(l.hp);
    i.local_get(a_len).local_get(l.i2).i32_sub().i32_const(1).i32_sub().local_set(l.hl);
    // The socket first (net_host's `datagram` checks and clones it before
    // the destination resolves), then the destination.
    net_slot_get(&mut i, g, t, fns, ng, l);
    net_port_check(&mut i, g, t, fns, ng, l, t.send_open);
    net_resolve_or_err(&mut i, g, t, fns, ng, l, t.send_open);
    i.global_get(ng.addrs).local_get(l.port).i32_wrap_i64().call(fns.sa);
    i.i32_const(NET_RESERVE).call(g.f_reserve);
    i.local_get(l.s).local_get(b_ptr).local_get(b_len).i32_const(1); // some(address)
    net_push_sa(&mut i, a, t);
    i.i32_const(ret).call(imp.at(NI_SEND));
    i.i32_const(ret).i32_load8_u(net_m8(0)).if_(BlockType::Empty);
    net_put(&mut i, l.w, t.send_head);
    net_put(&mut i, l.w, t.class_sep);
    i.i32_const(ret).i32_load8_u(net_m8(a.unit_payload)).local_set(l.last);
    net_put_class(&mut i, t, l, l.last);
    net_answer(&mut i, g, ng, l, 1);
    i.end();
    net_answer(&mut i, g, ng, l, 0);
    i.end();

    // ── udp_recv_from(sock = a, max = b) ──
    i.local_get(op).i32_const(OP_NET_RECV_FROM).i32_eq().if_(BlockType::Empty);
    i.local_get(a_ptr).local_get(a_len).call(fns.int).local_set(l.h);
    net_slot_get(&mut i, g, t, fns, ng, l);
    i.local_get(b_ptr).local_get(b_len).call(fns.int).local_tee(l.max);
    i.i64_const(0).i64_lt_s().if_(BlockType::Empty);
    net_put(&mut i, l.w, t.negative[0]);
    i.local_get(l.w).local_get(l.max).call(fns.dec).local_set(l.w);
    net_put(&mut i, l.w, t.negative[1]);
    net_answer(&mut i, g, ng, l, 1);
    i.end();
    i.i32_const(NET_RECV_RESERVE).call(g.f_reserve);
    i.local_get(l.s).i32_const(ret).call(imp.at(NI_RECEIVE));
    i.i32_const(ret).i32_load8_u(net_m8(0)).if_(BlockType::Empty);
    net_put(&mut i, l.w, t.recv_head);
    net_put(&mut i, l.w, t.class_sep);
    i.i32_const(ret).i32_load8_u(net_m8(a.recv_payload)).local_set(l.last);
    net_put_class(&mut i, t, l, l.last);
    net_answer(&mut i, g, ng, l, 1);
    i.end();
    // n = min(datagram length, max, 65535): the rest is dropped, as the OS
    // drops it natively.
    let rp = a.recv_payload;
    i.i32_const(ret).i32_load(net_m32(rp + a.recv_list + 4)).local_set(l.n);
    i.local_get(l.max).i64_const(NET_DATAGRAM as i64).i64_lt_s().if_(BlockType::Empty);
    i.local_get(l.max).i32_wrap_i64().local_get(l.n).i32_lt_u().if_(BlockType::Empty);
    i.local_get(l.max).i32_wrap_i64().local_set(l.n);
    i.end();
    i.else_();
    i.local_get(l.n).i32_const(NET_DATAGRAM).i32_gt_u().if_(BlockType::Empty);
    i.i32_const(NET_DATAGRAM).local_set(l.n);
    i.end();
    i.end();
    // `<port> <ip>\n`, then the bytes.
    let sa_at = ret + (rp + a.recv_addr) as i32;
    i.local_get(l.w);
    net_sa_port(&mut i, a, sa_at);
    i.i64_extend_i32_u().call(fns.dec).local_set(l.w);
    net_put_byte(&mut i, l.w, b' ');
    net_sa_to_el(&mut i, a, sa_at, t.scratch(NET_SCR_EL2));
    i.local_get(l.w).i32_const(t.scratch(NET_SCR_EL2)).call(fns.ip_fmt).local_set(l.w);
    net_put_byte(&mut i, l.w, b'\n');
    i.local_get(l.w).i32_const(ret).i32_load(net_m32(rp + a.recv_list)).local_get(l.n).memory_copy(0, 0);
    i.local_get(l.w).local_get(l.n).i32_add().local_set(l.w);
    net_answer(&mut i, g, ng, l, 0);
    i.end();

    // ── close(sock = a): a closed socket is ok, a never-issued one is not ──
    i.local_get(op).i32_const(OP_NET_CLOSE).i32_eq().if_(BlockType::Empty);
    i.local_get(a_ptr).local_get(a_len).call(fns.int).local_set(l.h);
    net_invalid_check(&mut i, g, t, fns, ng, l);
    i.global_get(ng.tab).local_get(l.h).i32_wrap_i64().i32_const(2).i32_shl().i32_add().local_tee(l.el);
    i.i32_load(net_m32(0)).local_tee(l.s).i32_const(0).i32_ge_s().if_(BlockType::Empty);
    i.local_get(l.s).call(imp.at(NI_DROP));
    i.local_get(l.el).i32_const(-1).i32_store(net_m32(0));
    i.end();
    net_answer(&mut i, g, ng, l, 0);
    i.end();

    // ── lookup_host(host = a): one address per line, each once ──
    i.local_get(op).i32_const(OP_NET_LOOKUP).i32_eq().if_(BlockType::Empty);
    i.local_get(a_ptr).local_set(l.hp);
    i.local_get(a_len).local_set(l.hl);
    i.i64_const(0).local_set(l.port);
    net_resolve_or_err(&mut i, g, t, fns, ng, l, t.lookup_open);
    // 48 bytes per address (the longest spelling and its newline).
    i.local_get(l.cnt).i32_const(48).i32_mul().i32_const(64).i32_add().local_set(l.n);
    net_ensure(&mut i, g, ng, l, l.n);
    i.global_get(ng.buf).local_set(l.w);
    i.i32_const(0).local_set(l.k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(l.k).local_get(l.cnt).i32_ge_u().br_if(1);
    i.global_get(ng.addrs).local_get(l.k).i32_const(a.ia_size as i32).i32_mul().i32_add().local_set(l.el);
    // Seen before? j walks the earlier elements; i1 = 1 on a match.
    i.i32_const(0).local_set(l.i1);
    i.i32_const(0).local_set(l.j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(l.j).local_get(l.k).i32_ge_u().br_if(1);
    i.global_get(ng.addrs).local_get(l.j).i32_const(a.ia_size as i32).i32_mul().i32_add().local_set(l.i2);
    net_same_ip(&mut i, a, l.el, l.i2);
    i.if_(BlockType::Empty).i32_const(1).local_set(l.i1).br(2).end();
    i.local_get(l.j).i32_const(1).i32_add().local_set(l.j);
    i.br(0).end().end();
    i.local_get(l.i1).i32_eqz().if_(BlockType::Empty);
    i.local_get(l.w).global_get(ng.buf).i32_ne().if_(BlockType::Empty);
    net_put_byte(&mut i, l.w, b'\n');
    i.end();
    i.local_get(l.w).local_get(l.el).call(fns.ip_fmt).local_set(l.w);
    i.end();
    i.local_get(l.k).i32_const(1).i32_add().local_set(l.k);
    i.br(0).end().end();
    net_answer(&mut i, g, ng, l, 0);
    i.end();

    i.unreachable();
    i.end();
    f
}

/// Grow the answer buffer to hold `need` bytes (the old one is abandoned;
/// it only ever grows). A grow moves the op's rewind mark past the new
/// buffer, so the rewind never frees it.
fn net_ensure(i: &mut Sink<'_>, g: P3Globals, ng: NetGlobals, l: NetLocals, need: u32) {
    i.local_get(need).global_get(ng.bufcap).i32_gt_u().if_(BlockType::Empty);
    i.i32_const(0).i32_const(0).i32_const(8).local_get(need).call(g.f_alloc).global_set(ng.buf);
    i.local_get(need).global_set(ng.bufcap);
    i.global_get(g.heap).local_set(l.h0);
    i.end();
}

/// Rewind the bump frontier to where the op found it (what the host lowered
/// for this op is dead), park `buf[..w]` and answer `status`.
fn net_answer(i: &mut Sink<'_>, g: P3Globals, ng: NetGlobals, l: NetLocals, status: i64) {
    i.local_get(l.h0).global_set(g.heap);
    i.global_get(ng.buf).global_set(g.g_ppos);
    i.local_get(l.w).global_get(ng.buf).i32_sub().global_set(g.g_plen);
    i.global_get(g.g_plen).i64_extend_i32_u().i64_const(status << 32).i64_or().return_();
}

/// `<open><escaped host><colon><port><close>`: a bind / send / lookup head.
fn net_head(i: &mut Sink<'_>, t: &NetTexts, fns: NetFns, l: NetLocals, open: NetPiece) {
    net_put(i, l.w, open);
    i.local_get(l.w).local_get(l.hp).local_get(l.hl).call(fns.esc).local_set(l.w);
    net_put(i, l.w, t.colon);
    i.local_get(l.w).local_get(l.port).call(fns.dec).local_set(l.w);
    net_put(i, l.w, t.close);
}

/// A port outside 0..=65535: `<head>: port out of range 0..=65535`.
fn net_port_check(i: &mut Sink<'_>, g: P3Globals, t: &NetTexts, fns: NetFns, ng: NetGlobals, l: NetLocals, open: NetPiece) {
    i.local_get(l.port).i64_const(65535).i64_gt_u().if_(BlockType::Empty);
    net_head(i, t, fns, l, open);
    net_put(i, l.w, t.port_range);
    net_answer(i, g, ng, l, 1);
    i.end();
}

/// Resolve `(hp, hl)` into `cnt`; nothing is `<head>: cannot resolve host
/// "<escaped host>"`.
fn net_resolve_or_err(i: &mut Sink<'_>, g: P3Globals, t: &NetTexts, fns: NetFns, ng: NetGlobals, l: NetLocals, open: NetPiece) {
    i.local_get(l.hp).local_get(l.hl).call(fns.resolve).local_tee(l.cnt);
    i.i32_eqz().if_(BlockType::Empty);
    net_head(i, t, fns, l, open);
    net_put(i, l.w, t.unres[0]);
    i.local_get(l.w).local_get(l.hp).local_get(l.hl).call(fns.esc).local_set(l.w);
    net_put(i, l.w, t.unres[1]);
    net_answer(i, g, ng, l, 1);
    i.end();
}

/// The class text of the `error-code` discriminant in local `disc`.
fn net_put_class(i: &mut Sink<'_>, t: &NetTexts, l: NetLocals, disc: u32) {
    i.local_get(disc).i32_const(3).i32_shl().i32_const(t.ec_table).i32_add().local_set(l.i2);
    i.local_get(l.w).local_get(l.i2).i32_load(net_m32(0)).local_get(l.i2).i32_load(net_m32(4)).memory_copy(0, 0);
    i.local_get(l.w).local_get(l.i2).i32_load(net_m32(4)).i32_add().local_set(l.w);
}

/// A handle past the table (a negative one included): `invalid UdpSocket
/// handle: <h>`.
fn net_invalid_check(i: &mut Sink<'_>, g: P3Globals, t: &NetTexts, fns: NetFns, ng: NetGlobals, l: NetLocals) {
    i.local_get(l.h).global_get(ng.n).i64_extend_i32_u().i64_ge_u().if_(BlockType::Empty);
    net_put(i, l.w, t.invalid[0]);
    i.local_get(l.w).local_get(l.h).call(fns.dec).local_set(l.w);
    net_put(i, l.w, t.invalid[1]);
    net_answer(i, g, ng, l, 1);
    i.end();
}

/// `s` = the socket in slot `h`, or the invalid / closed err.
fn net_slot_get(i: &mut Sink<'_>, g: P3Globals, t: &NetTexts, fns: NetFns, ng: NetGlobals, l: NetLocals) {
    net_invalid_check(i, g, t, fns, ng, l);
    i.global_get(ng.tab).local_get(l.h).i32_wrap_i64().i32_const(2).i32_shl().i32_add().i32_load(net_m32(0)).local_tee(l.s);
    i.i32_const(0).i32_lt_s().if_(BlockType::Empty);
    net_put(i, l.w, t.closed[0]);
    i.local_get(l.w).local_get(l.h).call(fns.dec).local_set(l.w);
    net_put(i, l.w, t.closed[1]);
    net_answer(i, g, ng, l, 1);
    i.end();
}

/// Put socket `s` in the lowest free slot (native's `almide_net_slot`),
/// its index in `j`; a full table doubles (the rewind mark moves past it).
fn net_slot_put(i: &mut Sink<'_>, g: P3Globals, ng: NetGlobals, l: NetLocals) {
    i.i32_const(0).local_set(l.j);
    i.block(BlockType::Empty);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(l.j).global_get(ng.n).i32_ge_u().br_if(1);
    i.global_get(ng.tab).local_get(l.j).i32_const(2).i32_shl().i32_add().local_tee(l.i1);
    i.i32_load(net_m32(0)).i32_const(0).i32_lt_s().if_(BlockType::Empty);
    i.local_get(l.i1).local_get(l.s).i32_store(net_m32(0));
    i.br(3); // done
    i.end();
    i.local_get(l.j).i32_const(1).i32_add().local_set(l.j);
    i.br(0).end().end();
    i.global_get(ng.n).global_get(ng.cap).i32_eq().if_(BlockType::Empty);
    // The new capacity: double, eight at first.
    i.global_get(ng.cap).i32_const(1).i32_shl().i32_const(8).global_get(ng.cap).select().local_set(l.i1);
    i.i32_const(0).i32_const(0).i32_const(8).local_get(l.i1).i32_const(2).i32_shl().call(g.f_alloc).local_set(l.i2);
    i.local_get(l.i2).global_get(ng.tab).global_get(ng.n).i32_const(2).i32_shl().memory_copy(0, 0);
    i.local_get(l.i2).global_set(ng.tab);
    i.local_get(l.i1).global_set(ng.cap);
    i.global_get(g.heap).local_set(l.h0);
    i.end();
    i.global_get(ng.tab).global_get(ng.n).i32_const(2).i32_shl().i32_add().local_get(l.s).i32_store(net_m32(0));
    i.global_get(ng.n).local_set(l.j);
    i.global_get(ng.n).i32_const(1).i32_add().global_set(ng.n);
    i.end();
}

/// Push the flat `ip-socket-address` operands `$sa` staged.
fn net_push_sa(i: &mut Sink<'_>, a: &NetAbi, t: &NetTexts) {
    for s in 0..a.flat_len {
        i.i32_const(0).i32_load(net_m32((t.scratch(NET_SCR_SA) + 4 * s as i32) as u64));
    }
}

/// The port of the `ip-socket-address` at `at`.
fn net_sa_port(i: &mut Sink<'_>, a: &NetAbi, at: i32) {
    i.i32_const(0).i32_load16_u(net_m16((at as u64) + a.sa_payload + a.sa4_port));
    i.i32_const(0).i32_load16_u(net_m16((at as u64) + a.sa_payload + a.sa6_port));
    i.i32_const(0).i32_load8_u(net_m8(at as u64)).i32_const(a.sa_case[0]).i32_eq().select();
}

/// The `ip-socket-address` at `at` as an `ip-address` element at `el`.
fn net_sa_to_el(i: &mut Sink<'_>, a: &NetAbi, at: i32, el: i32) {
    let at = at as u64;
    i.i32_const(0).i32_load8_u(net_m8(at)).i32_const(a.sa_case[0]).i32_eq().if_(BlockType::Empty);
    i.i32_const(el).i32_const(a.ia_case[0]).i32_store8(net_m8(0));
    i.i32_const(el + a.ia_payload as i32).i32_const((at + a.sa_payload + a.sa4_addr) as i32).i32_const(4).memory_copy(0, 0);
    i.else_();
    i.i32_const(el).i32_const(a.ia_case[1]).i32_store8(net_m8(0));
    i.i32_const(el + a.ia_payload as i32).i32_const((at + a.sa_payload + a.sa6_addr) as i32).i32_const(16).memory_copy(0, 0);
    i.end();
}

/// Whether the `ip-address` elements at locals `x` and `y` are one address:
/// the same case and the same payload bytes (never the padding).
fn net_same_ip(i: &mut Sink<'_>, a: &NetAbi, x: u32, y: u32) {
    let po = a.ia_payload;
    i.local_get(x).i32_load8_u(net_m8(0)).local_get(y).i32_load8_u(net_m8(0)).i32_eq();
    // Four bytes for either case, then twelve more for IPv6.
    i.local_get(x).i32_load(MemArg { offset: po, align: 0, memory_index: 0 });
    i.local_get(y).i32_load(MemArg { offset: po, align: 0, memory_index: 0 }).i32_eq().i32_and();
    i.local_get(x).i32_load8_u(net_m8(0)).i32_const(a.ia_case[0]).i32_eq();
    i.i32_const(1);
    for off in [4u64, 8, 12] {
        i.local_get(x).i32_load(MemArg { offset: po + off, align: 0, memory_index: 0 });
        i.local_get(y).i32_load(MemArg { offset: po + off, align: 0, memory_index: 0 }).i32_eq().i32_and();
    }
    i.i32_or().i32_and();
}

/// `dst` = the index of the `nth` (0 or 1) `\n` of `a[..a_len]`.
fn net_find_nl(i: &mut Sink<'_>, l: NetLocals, a_ptr: u32, a_len: u32, nth: u32, dst: u32) {
    if nth == 0 {
        i.i32_const(0).local_set(dst);
    } else {
        i.local_get(l.i1).i32_const(1).i32_add().local_set(dst);
    }
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(dst).local_get(a_len).i32_ge_u().br_if(1);
    i.local_get(a_ptr).local_get(dst).i32_add().i32_load8_u(net_m8(0)).i32_const('\n' as i32).i32_eq().br_if(1);
    i.local_get(dst).i32_const(1).i32_add().local_set(dst);
    i.br(0).end().end();
}

/// The net functions' bodies, in `NetFns` order.
fn push_net_shims(code: &mut CodeSection, g: P3Globals, a: &NetAbi, t: &NetTexts, fns: NetFns, ng: NetGlobals, imp: NetImports) {
    code.function(&shim_net(g, a, t, fns, ng, imp));
    code.function(&net_fn_esc());
    code.function(&net_fn_dec());
    code.function(&net_fn_int());
    code.function(&net_fn_v4());
    code.function(&net_fn_side(fns.v4));
    code.function(&net_fn_ip_parse(a, t, fns));
    code.function(&net_fn_ip_fmt(a, t, fns));
    code.function(&net_fn_sa(a, t));
    code.function(&net_fn_resolve(g, a, t, fns, ng, imp));
}

#[cfg(test)]
mod net_tests {
    //! The net shim's text helpers against what native runs: Rust's own
    //! `IpAddr` parser and `Display`, almide-base's C-418 escaper and i64
    //! formatting — the helpers instantiated alone on wasmtime and fed a
    //! corpus of valid, near-valid and random text.
    use super::*;
    use wasm_encoder::{DataSection, ExportSection, Module, TypeSection};

    fn resolve() -> wit_parser::Resolve {
        let mut r = wit_parser::Resolve::default();
        for (name, text) in [
            ("clocks.wit", include_str!("../wit/p3/deps/clocks/package.wit")),
            ("random.wit", include_str!("../wit/p3/deps/random/package.wit")),
            ("cli.wit", include_str!("../wit/p3/deps/cli/package.wit")),
            ("filesystem.wit", include_str!("../wit/p3/deps/filesystem/package.wit")),
            ("http.wit", include_str!("../wit/p3/deps/http/package.wit")),
            ("sockets.wit", include_str!("../wit/p3/deps/sockets/package.wit")),
        ] {
            r.push_str(name, text).unwrap_or_else(|e| panic!("wit {name}: {e}"));
        }
        let pkg = r.push_str("world.wit", include_str!("../wit/p3/world.wit")).expect("world.wit");
        r.select_world(&[pkg], Some("p3-command")).expect("p3-command");
        r
    }

    const IN: usize = 1024;
    const EL: usize = 4096;
    const OUT: usize = 8192;

    struct H {
        store: wasmtime::Store<()>,
        mem: wasmtime::Memory,
        esc: wasmtime::TypedFunc<(i32, i32, i32), i32>,
        dec: wasmtime::TypedFunc<(i32, i64), i32>,
        int: wasmtime::TypedFunc<(i32, i32), i64>,
        parse: wasmtime::TypedFunc<(i32, i32, i32), i32>,
        fmt: wasmtime::TypedFunc<(i32, i32), i32>,
        abi: NetAbi,
    }

    /// The helpers alone in one module (the shim and `$resolve`, which
    /// need the sockets imports, are `unreachable` stand-ins at their
    /// indices), with the texts at park 0.
    fn harness() -> H {
        let abi = net_abi(&resolve()).expect("the sockets ABI");
        let texts = NetTexts::new(0, &abi);
        let fns = NetFns::at(0);
        let mut types: Vec<(Vec<ValType>, Vec<ValType>)> = Vec::new();
        let t_fs = type_index(&mut types, &[ValType::I32; 5], &[ValType::I64]);
        let helper = NetFns::types(&mut types);
        let mut m = Module::new();
        let mut ts = TypeSection::new();
        for (p, r) in &types {
            ts.ty().function(p.iter().copied(), r.iter().copied());
        }
        m.section(&ts);
        let mut funcs = FunctionSection::new();
        NetFns::declare(&mut funcs, t_fs, &helper);
        m.section(&funcs);
        let mut mems = wasm_encoder::MemorySection::new();
        mems.memory(wasm_encoder::MemoryType { minimum: 8, maximum: None, memory64: false, shared: false, page_size_log2: None });
        m.section(&mems);
        let mut ex = ExportSection::new();
        ex.export("memory", wasm_encoder::ExportKind::Memory, 0);
        for (n, f) in [("esc", fns.esc), ("dec", fns.dec), ("int", fns.int), ("parse", fns.ip_parse), ("fmt", fns.ip_fmt)] {
            ex.export(n, wasm_encoder::ExportKind::Func, f);
        }
        m.section(&ex);
        let stand_in = || {
            let mut f = Function::new([]);
            f.instructions().unreachable().end();
            f
        };
        let mut code = CodeSection::new();
        code.function(&stand_in());
        code.function(&net_fn_esc());
        code.function(&net_fn_dec());
        code.function(&net_fn_int());
        code.function(&net_fn_v4());
        code.function(&net_fn_side(fns.v4));
        code.function(&net_fn_ip_parse(&abi, &texts, fns));
        code.function(&net_fn_ip_fmt(&abi, &texts, fns));
        code.function(&net_fn_sa(&abi, &texts));
        code.function(&stand_in());
        m.section(&code);
        let mut data = DataSection::new();
        data.active(0, &ConstExpr::i32_const(texts.base as i32), texts.blob.iter().copied());
        m.section(&data);
        let bytes = m.finish();
        wasmparser::validate(&bytes).expect("the helpers validate");
        let engine = wasmtime::Engine::default();
        let module = wasmtime::Module::new(&engine, &bytes).expect("module");
        let mut store = wasmtime::Store::new(&engine, ());
        let inst = wasmtime::Instance::new(&mut store, &module, &[]).expect("instance");
        H {
            mem: inst.get_memory(&mut store, "memory").expect("memory"),
            esc: inst.get_typed_func(&mut store, "esc").expect("esc"),
            dec: inst.get_typed_func(&mut store, "dec").expect("dec"),
            int: inst.get_typed_func(&mut store, "int").expect("int"),
            parse: inst.get_typed_func(&mut store, "parse").expect("parse"),
            fmt: inst.get_typed_func(&mut store, "fmt").expect("fmt"),
            store,
            abi,
        }
    }

    impl H {
        fn put(&mut self, at: usize, b: &[u8]) {
            self.mem.data_mut(&mut self.store)[at..at + b.len()].copy_from_slice(b);
        }
        fn get(&self, from: usize, to: i32) -> String {
            String::from_utf8(self.mem.data(&self.store)[from..to as usize].to_vec()).expect("utf-8 out")
        }
        /// The shim's parse of `s`, as Rust's type.
        fn parse(&mut self, s: &str) -> Option<std::net::IpAddr> {
            self.put(IN, s.as_bytes());
            let fam = self.parse.call(&mut self.store, (IN as i32, s.len() as i32, EL as i32)).expect("parse");
            let d = self.mem.data(&self.store);
            let po = EL + self.abi.ia_payload as usize;
            match fam {
                -1 => None,
                0 => {
                    assert_eq!(d[EL] as i32, self.abi.ia_case[0]);
                    Some(std::net::IpAddr::from([d[po], d[po + 1], d[po + 2], d[po + 3]]))
                }
                1 => {
                    assert_eq!(d[EL] as i32, self.abi.ia_case[1]);
                    let g: Vec<u16> = (0..8).map(|k| u16::from_le_bytes([d[po + 2 * k], d[po + 2 * k + 1]])).collect();
                    Some(std::net::IpAddr::from([g[0], g[1], g[2], g[3], g[4], g[5], g[6], g[7]]))
                }
                other => panic!("parse answered {other}"),
            }
        }
        /// The shim's spelling of `ip`.
        fn fmt(&mut self, ip: std::net::IpAddr) -> String {
            let mut el = vec![0u8; self.abi.ia_size as usize];
            let po = self.abi.ia_payload as usize;
            match ip {
                std::net::IpAddr::V4(v) => {
                    el[0] = self.abi.ia_case[0] as u8;
                    el[po..po + 4].copy_from_slice(&v.octets());
                }
                std::net::IpAddr::V6(v) => {
                    el[0] = self.abi.ia_case[1] as u8;
                    for (k, g) in v.segments().iter().enumerate() {
                        el[po + 2 * k..po + 2 * k + 2].copy_from_slice(&g.to_le_bytes());
                    }
                }
            }
            self.put(EL, &el);
            let end = self.fmt.call(&mut self.store, (OUT as i32, EL as i32)).expect("fmt");
            self.get(OUT, end)
        }
    }

    /// A deterministic generator (no rand dependency).
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.0 >> 33
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    fn corpus() -> Vec<String> {
        let mut out: Vec<String> = [
            "", "0.0.0.0", "127.0.0.1", "255.255.255.255", "256.0.0.1", "01.2.3.4", "1.2.3", "1.2.3.4.5", "1.2.3.4.",
            ".1.2.3.4", "1..2.3", "1.2.3.04", "0.0.0.00", "1.2.3.-4", "1.2.3.4 ", " 1.2.3.4", "1e1.2.3.4", "1234.1.1.1",
            "::", "::1", ":::", "::::", ":", "1:", ":1", "1::", "::1:", "1:::2", "1::2::3", "1:2:3:4:5:6:7:8",
            "1:2:3:4:5:6:7:8:9", "1:2:3:4:5:6:7::", "1:2:3:4:5:6:7:8::", "::1:2:3:4:5:6:7", "::1:2:3:4:5:6:7:8",
            "1::2:3:4:5:6:7", "::ffff:1.2.3.4", "::ffff:01.2.3.4", "1.2.3.4::", "1:2:3:4:5:6:1.2.3.4",
            "1:2:3:4:5:6:7:1.2.3.4", "1:2:3:4:5::1.2.3.4", "1:2:3:4:5:6::1.2.3.4", "::1.2.3.4:5", "::1.2.3.4",
            "12345::", "ABCD::ef01", "fFfF::", "00000::", "0000::", "g::", "1:2:3:4:5:6:7:8%eth0", "[::1]", "::1]",
            "1.2.3.4:80", "localhost", "0x7f.0.0.1", "127.1", "::0:0", "2001:db8::1", "fe80::1:2", "1::", "::2:3:4:5:6:7:8",
            "1:2:3:4:5:6:7:8:", ":1:2:3:4:5:6:7:8", "1:2:3:4::5:6:7:8", "::255.255.255.255", "::256.1.1.1",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let mut g = Lcg(0x5eed);
        let alpha: &[u8] = b"0123456789abcdefABCDEF:.%x ";
        for _ in 0..20_000 {
            let n = g.below(18) as usize;
            out.push((0..n).map(|_| alpha[g.below(alpha.len() as u64) as usize] as char).collect());
        }
        // Valid addresses in every spelling Rust accepts, and near misses.
        for _ in 0..5_000 {
            let segs: [u16; 8] = std::array::from_fn(|_| if g.below(3) == 0 { g.below(65536) as u16 } else { 0 });
            let v6 = std::net::Ipv6Addr::from(segs);
            out.push(v6.to_string());
            out.push(segs.iter().map(|s| format!("{s:X}")).collect::<Vec<_>>().join(":"));
            out.push(segs.iter().map(|s| format!("{s:04x}")).collect::<Vec<_>>().join(":"));
            let v4 = std::net::Ipv4Addr::from(g.next() as u32);
            out.push(v4.to_string());
            out.push(format!("::ffff:{v4}"));
            out.push(format!("{}:{v4}", segs[..6].iter().map(|s| format!("{s:x}")).collect::<Vec<_>>().join(":")));
            let s = v6.to_string();
            let cut = g.below(s.len() as u64 + 1) as usize;
            out.push(format!("{}{}", &s[..cut], &s[cut.min(s.len())..].replacen(':', "", 1)));
        }
        out
    }

    /// Every input: the shim parses exactly what `IpAddr::from_str` parses,
    /// to the same address.
    #[test]
    fn ip_literals_parse_as_rust_parses_them() {
        let mut h = harness();
        let mut bad = Vec::new();
        for s in corpus() {
            let want = s.parse::<std::net::IpAddr>().ok();
            let got = h.parse(&s);
            if got != want {
                bad.push(format!("{s:?}: shim {got:?}, rust {want:?}"));
            }
        }
        assert!(bad.is_empty(), "{} disagreement(s):\n{}", bad.len(), bad.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
    }

    /// Every address: spelled as Rust's `Display` spells it (RFC 5952 runs,
    /// the IPv4-mapped form).
    #[test]
    fn addresses_print_as_rust_displays_them() {
        let mut h = harness();
        let mut g = Lcg(0xadd5);
        let mut ips: Vec<std::net::IpAddr> = [
            "0.0.0.0", "127.0.0.1", "255.255.255.255", "::", "::1", "1::", "::ffff:1.2.3.4", "::ffff:0.0.0.0", "1:0:0:2:0:0:0:3",
            "1:0:2:0:3:0:4:0", "0:0:1:0:0:1:0:0", "ffff::", "::ffff:0:1", "0:0:0:0:0:fffe:1:2", "2001:db8:0:0:1:0:0:1",
        ]
        .iter()
        .map(|s| s.parse().expect("seed"))
        .collect();
        for _ in 0..20_000 {
            let segs: [u16; 8] = std::array::from_fn(|_| match g.below(4) {
                0 => g.below(65536) as u16,
                1 => g.below(16) as u16,
                _ => 0,
            });
            ips.push(std::net::Ipv6Addr::from(segs).into());
            ips.push(std::net::Ipv4Addr::from(g.next() as u32).into());
        }
        for ip in ips {
            assert_eq!(h.fmt(ip), ip.to_string(), "{ip:?}");
        }
    }

    /// The C-418 escaper's twin: byte for byte `almide_rt_errtext_escape`.
    #[test]
    fn hosts_escape_as_the_core_escapes_them() {
        let mut h = harness();
        let mut cases: Vec<String> = [
            "", "localhost", "a\"b\\c", "tab\there", "nl\nx\r\0", "\u{1b}[31m", "\u{7f}", "\u{85}", "\u{9f}", "\u{a0}",
            "\u{ad}", "\u{200b}", "\u{202e}rtl", "\u{2028}", "\u{feff}", "\u{e0041}\u{e007f}", "日本語", "😀", "é\u{301}",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        // Every escaped range's ends and a neighbour each side.
        for &(lo, hi) in almide_base::err_text::ALMIDE_RT_ERRTEXT_ESCAPED {
            for c in [lo.saturating_sub(1), lo, hi, hi + 1] {
                if let Some(ch) = char::from_u32(c) {
                    cases.push(format!("x{ch}y"));
                }
            }
        }
        for s in cases {
            h.put(IN, s.as_bytes());
            let end = h.esc.call(&mut h.store, (OUT as i32, IN as i32, s.len() as i32)).expect("esc");
            assert_eq!(h.get(OUT, end), almide_base::err_text::almide_rt_errtext_escape(&s), "{s:?}");
        }
    }

    /// Decimal both ways, the ends of i64 included.
    #[test]
    fn decimals_render_and_read_as_rust_does() {
        let mut h = harness();
        for v in [0i64, 1, -1, 9, 10, 65535, 70000, -7654321, i64::MAX, i64::MIN, 1 << 40] {
            let end = h.dec.call(&mut h.store, (OUT as i32, v)).expect("dec");
            let text = h.get(OUT, end);
            assert_eq!(text, v.to_string());
            if v != i64::MIN {
                h.put(IN, text.as_bytes());
                assert_eq!(h.int.call(&mut h.store, (IN as i32, text.len() as i32)).expect("int"), v);
            }
        }
    }

    /// The whole net block — the shim, `$resolve` and every helper — over
    /// the sockets imports as the WIT derives them, validated as one module
    /// (the helpers alone are run above; this catches a stack-shape slip in
    /// the code only a real artifact reaches).
    #[test]
    fn the_net_block_validates_against_its_derived_imports() {
        use wasm_encoder::{EntityType, GlobalSection, ImportSection, MemorySection, MemoryType};
        let abi = net_abi(&resolve()).expect("abi");
        let texts = NetTexts::new(0, &abi);
        let mut types: Vec<(Vec<ValType>, Vec<ValType>)> = Vec::new();
        let imp = NetImports { base: 0 };
        let list = imp.import_list(&abi, &mut types);
        let t_fs = type_index(&mut types, &[ValType::I32; 5], &[ValType::I64]);
        let t_realloc = type_index(&mut types, &[ValType::I32; 4], &[ValType::I32]);
        let t_reserve = type_index(&mut types, &[ValType::I32], &[]);
        let helper = NetFns::types(&mut types);
        let mut m = Module::new();
        let mut ts = TypeSection::new();
        for (p, r) in &types {
            ts.ty().function(p.iter().copied(), r.iter().copied());
        }
        m.section(&ts);
        let mut is = ImportSection::new();
        for (_, md, n, t) in &list {
            is.import(md, n, EntityType::Function(*t));
        }
        m.section(&is);
        let f_alloc = NET_IMPORTS;
        let f_reserve = NET_IMPORTS + 1;
        let fns = NetFns::at(NET_IMPORTS + 2);
        let mut funcs = FunctionSection::new();
        funcs.function(t_realloc);
        funcs.function(t_reserve);
        NetFns::declare(&mut funcs, t_fs, &helper);
        m.section(&funcs);
        let mut mems = MemorySection::new();
        mems.memory(MemoryType { minimum: 8, maximum: None, memory64: false, shared: false, page_size_log2: None });
        m.section(&mems);
        // plen, ppos, heap, then the net globals.
        let mut gs = GlobalSection::new();
        let t = GlobalType { val_type: ValType::I32, mutable: true, shared: false };
        for _ in 0..3 {
            gs.global(t, &ConstExpr::i32_const(0));
        }
        NetGlobals::emit(&mut gs);
        m.section(&gs);
        let g = P3Globals {
            park: 0, f_alloc, g_plen: 0, g_ppos: 1, g_in_rx: 0, g_in_fut: 0, g_out_tx: 0, g_out_fut: 0, g_err_tx: 0,
            g_err_fut: 0, g_wset: 0, g_slots: 0, g_slotn: 0, f_reserve, f_await: 0, g_env: 0, g_envn: 0, heap: 2,
        };
        let mut code = CodeSection::new();
        let mut alloc = Function::new([]);
        alloc.instructions().i32_const(0).end();
        code.function(&alloc);
        let mut reserve = Function::new([]);
        reserve.instructions().end();
        code.function(&reserve);
        push_net_shims(&mut code, g, &abi, &texts, fns, NetGlobals::at(3), imp);
        m.section(&code);
        let bytes = m.finish();
        if let Err(e) = wasmparser::validate(&bytes) {
            panic!("the net block does not validate: {e}");
        }
    }

    /// The texts are cut out of what the net core answers; spot-check the
    /// cuts against the texts C-443's fixture prints.
    #[test]
    fn the_texts_are_the_net_cores() {
        let abi = net_abi(&resolve()).expect("abi");
        let t = NetTexts::new(0, &abi);
        let piece = |p: NetPiece| {
            let at = p.at as usize - t.base as usize;
            String::from_utf8(t.blob[at..at + p.len as usize].to_vec()).expect("utf-8")
        };
        assert_eq!(piece(t.bind_open), "udp_bind(");
        assert_eq!(piece(t.send_open), "udp_send_to(");
        assert_eq!((piece(t.colon), piece(t.close)), (":".to_string(), ")".to_string()));
        assert_eq!(piece(t.port_range), ": port out of range 0..=65535");
        assert_eq!((piece(t.unres[0]), piece(t.unres[1])), (": cannot resolve host \"".to_string(), "\"".to_string()));
        assert_eq!(piece(t.class_sep), ": ");
        assert_eq!((piece(t.invalid[0]), piece(t.invalid[1])), ("invalid UdpSocket handle: ".to_string(), String::new()));
        assert_eq!((piece(t.closed[0]), piece(t.closed[1])), ("UdpSocket handle ".to_string(), " is closed".to_string()));
        assert_eq!(piece(t.negative[0]), "udp_recv_from: negative length ");
        assert_eq!(piece(t.recv_head), "udp_recv_from");
        assert_eq!(abi.ec_class.len(), NET_EC_NAMES.len(), "every error-code case has its class");
    }
}
