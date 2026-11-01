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
        while !(base + blob.len() as u64).is_multiple_of(4) {
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

// The helper bodies and the shim: wasi_p3_net_fns.rs, wasi_p3_net_shim.rs
// (included by wasi_p3.rs next to this file). Tests: tests/p3_net_test.rs.
