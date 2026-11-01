//! The net shim's text helpers against what native runs: Rust's own
//! `IpAddr` parser and `Display`, almide-base's C-418 escaper and i64
//! formatting — the helpers instantiated alone on wasmtime and fed a
//! corpus of valid, near-valid and random text.
use super::*;
use wasm_encoder::{DataSection, ExportSection, Module, TypeSection};

fn resolve() -> wit_parser::Resolve {
    let mut r = wit_parser::Resolve::default();
    for (name, text) in [
        ("clocks.wit", include_str!("../../wit/p3/deps/clocks/package.wit")),
        ("random.wit", include_str!("../../wit/p3/deps/random/package.wit")),
        ("cli.wit", include_str!("../../wit/p3/deps/cli/package.wit")),
        ("filesystem.wit", include_str!("../../wit/p3/deps/filesystem/package.wit")),
        ("http.wit", include_str!("../../wit/p3/deps/http/package.wit")),
        ("sockets.wit", include_str!("../../wit/p3/deps/sockets/package.wit")),
    ] {
        r.push_str(name, text).unwrap_or_else(|e| panic!("wit {name}: {e}"));
    }
    let pkg = r.push_str("world.wit", include_str!("../../wit/p3/world.wit")).expect("world.wit");
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
    code.function(&stand_in());
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
    use wasm_encoder::{GlobalSection, MemorySection, MemoryType};
    let abi = net_abi(&resolve()).expect("abi");
    let texts = NetTexts::new(0, &abi);
    let mut types: Vec<(Vec<ValType>, Vec<ValType>)> = Vec::new();
    // The real p3 import table first (the shims call its fixed indices:
    // the waitable sets, the subtasks), then the net block after it.
    let t = P3Types::new(&mut types);
    let base = base_import_list(&t);
    let imp = NetImports { base: IMPORTS };
    let list = imp.import_list(&abi, &mut types);
    let helper = NetFns::types(&mut types);
    let mut m = Module::new();
    let mut ts = TypeSection::new();
    for (p, r) in &types {
        ts.ty().function(p.iter().copied(), r.iter().copied());
    }
    m.section(&ts);
    let blocks: Vec<&[(u32, &str, &str, u32)]> = vec![&base, &list];
    m.section(&build_import_section(&blocks));
    // `$alloc`, `$reserve`, `$await`, then the net block.
    let f_alloc = IMPORTS + NET_IMPORTS;
    let f_reserve = f_alloc + 1;
    let f_await = f_alloc + 2;
    let fns = NetFns::at(f_alloc + 3);
    let mut funcs = FunctionSection::new();
    funcs.function(t.realloc);
    funcs.function(t.exit);
    funcs.function(t.fut_read);
    NetFns::declare(&mut funcs, t.fs, &helper);
    m.section(&funcs);
    let mut mems = MemorySection::new();
    mems.memory(MemoryType { minimum: 8, maximum: None, memory64: false, shared: false, page_size_log2: None });
    m.section(&mems);
    // plen, ppos, heap, then the net globals.
    let mut gs = GlobalSection::new();
    let gt = GlobalType { val_type: ValType::I32, mutable: true, shared: false };
    for _ in 0..3 {
        gs.global(gt, &ConstExpr::i32_const(0));
    }
    NetGlobals::emit(&mut gs);
    m.section(&gs);
    let g = P3Globals {
        park: 0, f_alloc, g_plen: 0, g_ppos: 1, g_in_rx: 0, g_in_fut: 0, g_out_tx: 0, g_out_fut: 0, g_err_tx: 0,
        g_err_fut: 0, g_wset: 0, g_slots: 0, g_slotn: 0, f_reserve, f_await, g_env: 0, g_envn: 0, heap: 2,
    };
    let mut code = CodeSection::new();
    let mut alloc = Function::new([]);
    alloc.instructions().i32_const(0).end();
    code.function(&alloc);
    let mut reserve = Function::new([]);
    reserve.instructions().end();
    code.function(&reserve);
    let mut wait = Function::new([]);
    wait.instructions().local_get(1).end();
    code.function(&wait);
    push_net_shims(&mut code, g, &abi, &texts, NetParts { fns, ng: NetGlobals::at(3), imp });
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
