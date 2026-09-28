// `include!`d part of wasi_p3.rs (ADR-0023 step 2): the http shim's error
// classes. Every text piece is taken from almide-rt-core's
// http_error_core.rs — the table native and the embedded host render from
// — and laid out once as a data segment; `$http_quote` twins
// `http_quote` byte for byte. So a classified failure reads the same on
// native, the embedded lane and this component (C-328, C-370).

/// Where the texts live: the park span's fifth page, which the p1 layout
/// gives the env overlay log (OVL) and the p3 shim leaves unused.
const HTTP_TEXT: u64 = 4 * 65536;

/// A text piece's absolute address and length.
#[derive(Clone, Copy, Default)]
struct Piece {
    at: i32,
    len: i32,
}

/// Bytes of one entry of the error-code table: four pieces (head, tail,
/// the case name, the closing text), each (address, length).
const EC_ENTRY: i32 = 32;

/// The texts and tables the http shim renders from, laid out at
/// `park + HTTP_TEXT`.
struct HttpErrTexts {
    blob: Vec<u8>,
    base: u64,
    url_head: Piece,
    url_no_scheme: Piece,
    url_slashes: [Piece; 2],
    url_scheme: [Piece; 2],
    /// `[name, value, forbidden]` × `[head, tail]`.
    hdr: [[Piece; 2]; 3],
    too_large: [Piece; 3],
    key_timeout: Piece,
    key_max: Piece,
    /// 128 bytes: 1 where the ASCII byte is an RFC 9110 tchar.
    tchar: i32,
    /// 9 × (address, length) of the managed header names.
    forbidden: i32,
    /// One `EC_ENTRY` per error-code case, in discriminant order.
    ec_table: i32,
    /// The entry an unusable request line answers with.
    ec_uri_invalid: i32,
}

impl HttpErrTexts {
    fn new(park: u64, h: &HttpAbi) -> Self {
        use almide_rt_core::http_client_core as hc;
        let base = park + HTTP_TEXT;
        let mut blob: Vec<u8> = Vec::new();
        let mut put = |bytes: &[u8]| -> Piece {
            let at = (base + blob.len() as u64) as i32;
            blob.extend_from_slice(bytes);
            Piece { at, len: bytes.len() as i32 }
        };
        let url_head = put(hc::HTTP_ERR_URL_HEAD.as_bytes());
        let url_no_scheme = put(format!(": {}", hc::HTTP_ERR_URL_NO_SCHEME).as_bytes());
        let [sh, st] = hc::HTTP_ERR_URL_SLASHES;
        let url_slashes = [put(format!(": {sh}").as_bytes()), put(st.as_bytes())];
        let [ch, ct] = hc::HTTP_ERR_URL_SCHEME;
        let url_scheme = [put(format!(": {ch}").as_bytes()), put(ct.as_bytes())];
        let hdr = [hc::HTTP_ERR_HEADER_NAME, hc::HTTP_ERR_HEADER_VALUE, hc::HTTP_ERR_HEADER_FORBIDDEN]
            .map(|(head, tail)| [put(head.as_bytes()), put(tail.as_bytes())]);
        let too_large = hc::HTTP_ERR_TOO_LARGE.map(|t| put(t.as_bytes()));
        let key_timeout = put(b"ALMIDE_HTTP_TIMEOUT_SECS");
        let key_max = put(b"ALMIDE_HTTP_MAX_RESPONSE_BYTES");
        let tchar_bytes: Vec<u8> =
            (0u8..128).map(|b| u8::from(b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))).collect();
        let tchar = put(&tchar_bytes).at;
        let names: Vec<Piece> = hc::HTTP_FORBIDDEN_HEADERS.iter().map(|n| put(n.as_bytes())).collect();
        let classes = [
            hc::HttpErrorClass::Dns,
            hc::HttpErrorClass::Connect,
            hc::HttpErrorClass::Timeout,
            hc::HttpErrorClass::Tls,
            hc::HttpErrorClass::Protocol,
        ]
        .map(|c| {
            let (head, tail) = hc::http_error_parts(c);
            (c, put(head.as_bytes()), put(tail.as_bytes()))
        });
        let [uh, um, ut] = hc::HTTP_ERR_UNCLASSIFIED.map(|t| put(t.as_bytes()));
        let mut entries: Vec<[Piece; 4]> = Vec::new();
        for name in &h.ec_names {
            let entry = match http_ec_class(name) {
                Some(class) => {
                    let (_, head, tail) = classes.iter().find(|(c, _, _)| *c == class).expect("every class laid out");
                    [*head, *tail, Piece::default(), Piece::default()]
                }
                None => [uh, um, put(name.as_bytes()), ut],
            };
            entries.push(entry);
        }
        let mut table: Vec<u8> = Vec::new();
        for p in names.iter() {
            table.extend_from_slice(&p.at.to_le_bytes());
            table.extend_from_slice(&p.len.to_le_bytes());
        }
        let forbidden = put(&table).at;
        let mut table: Vec<u8> = Vec::new();
        for e in &entries {
            for p in e {
                table.extend_from_slice(&p.at.to_le_bytes());
                table.extend_from_slice(&p.len.to_le_bytes());
            }
        }
        let ec_table = put(&table).at;
        let uri = h.ec_names.iter().position(|n| n == "HTTP-request-URI-invalid").expect("wasi:http names the case");
        assert!(blob.len() as u64 <= PARK_SPAN - HTTP_TEXT, "the http texts overrun the park span");
        HttpErrTexts {
            blob,
            base,
            url_head,
            url_no_scheme,
            url_slashes,
            url_scheme,
            hdr,
            too_large,
            key_timeout,
            key_max,
            tchar,
            forbidden,
            ec_table,
            ec_uri_invalid: ec_table + uri as i32 * EC_ENTRY,
        }
    }
}

/// The class a `wasi:http` `error-code` case belongs to (ADR-0023 §4.2);
/// `None` = unclassified (named by its case, or `internal-error`'s text).
fn http_ec_class(name: &str) -> Option<almide_rt_core::http_client_core::HttpErrorClass> {
    use almide_rt_core::http_client_core::HttpErrorClass as C;
    Some(match name {
        "DNS-timeout" | "DNS-error" | "destination-not-found" => C::Dns,
        "destination-unavailable" | "destination-IP-prohibited" | "destination-IP-unroutable" | "connection-refused"
        | "connection-limit-reached" => C::Connect,
        "connection-timeout" | "connection-read-timeout" | "connection-write-timeout" | "HTTP-response-timeout" => {
            C::Timeout
        }
        "TLS-protocol-error" | "TLS-certificate-error" | "TLS-alert-received" => C::Tls,
        "connection-terminated"
        | "HTTP-response-incomplete"
        | "HTTP-response-header-section-size"
        | "HTTP-response-header-size"
        | "HTTP-response-body-size"
        | "HTTP-response-trailer-section-size"
        | "HTTP-response-trailer-size"
        | "HTTP-response-transfer-coding"
        | "HTTP-response-content-coding"
        | "HTTP-upgrade-failed"
        | "HTTP-protocol-error" => C::Protocol,
        _ => return None,
    })
}

/// The http error helpers' function indices.
#[derive(Clone, Copy)]
struct HttpErrFns {
    quote: u32,
    err: u32,
    check: u32,
    num: u32,
}

/// `(src, len, dst) -> dst_end`: write `src[..len]` at `dst` quoted by
/// `http_quote`'s rule — `"` and `\` escaped, `\t` `\r` `\n` `\0`, any
/// other ASCII control or DEL as `\u{<lowercase hex>}`, every other byte
/// copied — between double quotes.
fn shim_http_quote() -> Function {
    let (src, len, dst) = (0u32, 1u32, 2u32);
    let (k, b) = (3u32, 4u32);
    // Local 5 is `http_hex_digit`'s scratch.
    let mut f = Function::new([(3, ValType::I32)]);
    let mut i = f.instructions();
    let put = |i: &mut wasm_encoder::InstructionSink<'_>, byte: i32| {
        i.local_get(dst).i32_const(byte).i32_store8(mem8(0));
        i.local_get(dst).i32_const(1).i32_add().local_set(dst);
    };
    put(&mut i, '"' as i32);
    i.i32_const(0).local_set(k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).local_get(len).i32_ge_u().br_if(1);
    i.local_get(src).local_get(k).i32_add().i32_load8_u(mem8(0)).local_set(b);
    i.block(BlockType::Empty);
    for (byte, esc) in [(b'"', b'"'), (b'\\', b'\\'), (b'\t', b't'), (b'\r', b'r'), (b'\n', b'n'), (0u8, b'0')] {
        i.local_get(b).i32_const(byte as i32).i32_eq().if_(BlockType::Empty);
        put(&mut i, '\\' as i32);
        put(&mut i, esc as i32);
        i.br(1);
        i.end();
    }
    i.local_get(b).i32_const(0x20).i32_lt_u();
    i.local_get(b).i32_const(0x7f).i32_eq();
    i.i32_or().if_(BlockType::Empty);
    for c in b"\\u{" {
        put(&mut i, *c as i32);
    }
    i.local_get(b).i32_const(16).i32_ge_u().if_(BlockType::Empty);
    http_hex_digit(&mut i, b, 4, dst);
    i.end();
    http_hex_digit(&mut i, b, 0, dst);
    put(&mut i, '}' as i32);
    i.br(1);
    i.end();
    i.local_get(dst).local_get(b).i32_store8(mem8(0));
    i.local_get(dst).i32_const(1).i32_add().local_set(dst);
    i.end();
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    put(&mut i, '"' as i32);
    i.local_get(dst);
    i.end();
    f
}

/// Store the lowercase hex digit of `(b >> shift) & 15` at `dst`, advance.
fn http_hex_digit(i: &mut wasm_encoder::InstructionSink<'_>, b: u32, shift: i32, dst: u32) {
    i.local_get(dst);
    i.local_get(b).i32_const(shift).i32_shr_u().i32_const(15).i32_and();
    i.local_tee(b + 1); // the quote fn's scratch: its only local past `b`
    i.i32_const(48).i32_add();
    i.local_get(b + 1).i32_const(87).i32_add();
    i.local_get(b + 1).i32_const(10).i32_lt_u();
    i.select();
    i.i32_store8(mem8(0));
    i.local_get(dst).i32_const(1).i32_add().local_set(dst);
}

/// `(p1, p1n, q, qn, p2, p2n, r, rn, p3, p3n) -> i64`: the err answer of an
/// fs_call op — `p1 ++ quote(q) ++ p2 ++ r ++ p3` in a fresh buffer, parked
/// (`g_ppos` / `g_plen`), answered `pack(1, len)`. `qn < 0` quotes nothing.
fn shim_http_err(g: P3Globals, quote: u32) -> Function {
    let P3Globals { g_plen, g_ppos, f_alloc, .. } = g;
    let (p1, p1n, q, qn, p2, p2n, r, rn, p3, p3n) = (0u32, 1, 2, 3, 4, 5, 6, 7, 8, 9);
    let (buf, dst) = (10u32, 11u32);
    let mut f = Function::new([(2, ValType::I32)]);
    let mut i = f.instructions();
    i.i32_const(0).i32_const(0).i32_const(1);
    i.local_get(p1n).local_get(p2n).i32_add().local_get(rn).i32_add().local_get(p3n).i32_add();
    i.local_get(qn).i32_const(0).i32_ge_s().if_(BlockType::Result(ValType::I32));
    i.local_get(qn).i32_const(6).i32_mul().i32_const(2).i32_add();
    i.else_().i32_const(0).end();
    i.i32_add();
    i.call(f_alloc).local_tee(buf).local_set(dst);
    let copy = |i: &mut wasm_encoder::InstructionSink<'_>, p: u32, n: u32| {
        i.local_get(dst).local_get(p).local_get(n).memory_copy(0, 0);
        i.local_get(dst).local_get(n).i32_add().local_set(dst);
    };
    copy(&mut i, p1, p1n);
    i.local_get(qn).i32_const(0).i32_ge_s().if_(BlockType::Empty);
    i.local_get(q).local_get(qn).local_get(dst).call(quote).local_set(dst);
    i.end();
    copy(&mut i, p2, p2n);
    copy(&mut i, r, rn);
    copy(&mut i, p3, p3n);
    i.local_get(buf).global_set(g_ppos);
    i.local_get(dst).local_get(buf).i32_sub().global_set(g_plen);
    i.i64_const(1).i64_const(32).i64_shl();
    i.global_get(g_plen).i64_extend_i32_u().i64_or();
    i.end();
    f
}

/// `(name, name_len, value, value_len) -> code`: rt-core's
/// `http_check_request` for one header, in its order — 1 = the name is not
/// a token, 2 = the value holds a control but HTAB (or DEL), 3 = a managed
/// name (ASCII case-insensitive), 0 = sendable.
fn shim_http_hdr_check(t: &HttpErrTexts) -> Function {
    let (np, nn, vp, vn) = (0u32, 1u32, 2u32, 3u32);
    let (k, b, j, fp, fln) = (4u32, 5u32, 6u32, 7u32, 8u32);
    let mut f = Function::new([(5, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(nn).i32_eqz().if_(BlockType::Empty);
    i.i32_const(1).return_();
    i.end();
    i.i32_const(0).local_set(k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).local_get(nn).i32_ge_u().br_if(1);
    i.local_get(np).local_get(k).i32_add().i32_load8_u(mem8(0)).local_set(b);
    i.local_get(b).i32_const(128).i32_ge_u();
    i.local_get(b).i32_const(127).i32_and().i32_const(t.tchar).i32_add().i32_load8_u(mem8(0)).i32_eqz();
    i.i32_or().if_(BlockType::Empty);
    i.i32_const(1).return_();
    i.end();
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    i.i32_const(0).local_set(k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).local_get(vn).i32_ge_u().br_if(1);
    i.local_get(vp).local_get(k).i32_add().i32_load8_u(mem8(0)).local_set(b);
    i.local_get(b).i32_const(0x20).i32_lt_u().local_get(b).i32_const(9).i32_ne().i32_and();
    i.local_get(b).i32_const(0x7f).i32_eq().i32_or().if_(BlockType::Empty);
    i.i32_const(2).return_();
    i.end();
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    // The managed names: a token's bytes lowercase by `| 0x20` exactly.
    i.i32_const(0).local_set(j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).i32_const(9).i32_ge_u().br_if(1);
    i.local_get(j).i32_const(8).i32_mul().i32_const(t.forbidden).i32_add().local_tee(fp).i32_load(mem(4)).local_set(fln);
    i.local_get(fp).i32_load(mem(0)).local_set(fp);
    i.local_get(fln).local_get(nn).i32_eq().if_(BlockType::Empty);
    i.i32_const(0).local_set(k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).local_get(nn).i32_ge_u().if_(BlockType::Empty);
    i.i32_const(3).return_();
    i.end();
    i.local_get(np).local_get(k).i32_add().i32_load8_u(mem8(0)).i32_const(0x20).i32_or();
    i.local_get(fp).local_get(k).i32_add().i32_load8_u(mem8(0));
    i.i32_ne().br_if(1);
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    i.end();
    i.local_get(j).i32_const(1).i32_add().local_set(j);
    i.br(0).end().end();
    i.i32_const(0);
    i.end();
    f
}

/// `(key, key_len, default) -> i64`: rt-core's reading of a numeric
/// environment variable — the value trimmed of ASCII whitespace, an
/// optional `+`, then decimal digits only; anything else (or unset, or
/// past i64) is `default`. Through the env service's op 26, whose cached
/// `get-environment` list the program's own `env.get` shares.
fn shim_http_env_num(g: P3Globals, f_env: u32) -> Function {
    let P3Globals { g_plen, g_ppos, .. } = g;
    let (key, key_len, default) = (0u32, 1u32, 2u32);
    let (p, start, end, b) = (3u32, 4u32, 5u32, 6u32);
    let acc = 7u32;
    let mut f = Function::new([(4, ValType::I32), (1, ValType::I64)]);
    let mut i = f.instructions();
    i.i32_const(26).local_get(key).local_get(key_len).i32_const(0).i32_const(0).call(f_env);
    i.i64_const(32).i64_shr_u().i64_const(0).i64_ne().if_(BlockType::Empty);
    i.local_get(default).return_();
    i.end();
    i.global_get(g_ppos).local_set(p);
    i.i32_const(0).local_set(start);
    i.global_get(g_plen).local_set(end);
    let is_ws = |i: &mut wasm_encoder::InstructionSink<'_>| {
        // ASCII whitespace as `str::trim` sees it: TAB..CR and SPACE.
        i.local_get(b).i32_const(9).i32_sub().i32_const(5).i32_lt_u();
        i.local_get(b).i32_const(32).i32_eq().i32_or();
    };
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(start).local_get(end).i32_ge_u().br_if(1);
    i.local_get(p).local_get(start).i32_add().i32_load8_u(mem8(0)).local_set(b);
    is_ws(&mut i);
    i.i32_eqz().br_if(1);
    i.local_get(start).i32_const(1).i32_add().local_set(start);
    i.br(0).end().end();
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(end).local_get(start).i32_le_u().br_if(1);
    i.local_get(p).local_get(end).i32_add().i32_const(1).i32_sub().i32_load8_u(mem8(0)).local_set(b);
    is_ws(&mut i);
    i.i32_eqz().br_if(1);
    i.local_get(end).i32_const(1).i32_sub().local_set(end);
    i.br(0).end().end();
    i.local_get(start).local_get(end).i32_lt_u();
    i.local_get(p).local_get(start).i32_add().i32_load8_u(mem8(0)).i32_const('+' as i32).i32_eq();
    i.i32_and().if_(BlockType::Empty);
    i.local_get(start).i32_const(1).i32_add().local_set(start);
    i.end();
    i.local_get(start).local_get(end).i32_ge_u().if_(BlockType::Empty);
    i.local_get(default).return_();
    i.end();
    i.i64_const(0).local_set(acc);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(start).local_get(end).i32_ge_u().br_if(1);
    i.local_get(p).local_get(start).i32_add().i32_load8_u(mem8(0)).i32_const('0' as i32).i32_sub().local_tee(b);
    i.i32_const(10).i32_ge_u();
    i.local_get(acc).i64_const((i64::MAX - 9) / 10).i64_gt_s();
    i.i32_or().if_(BlockType::Empty);
    i.local_get(default).return_();
    i.end();
    i.local_get(acc).i64_const(10).i64_mul().local_get(b).i64_extend_i32_u().i64_add().local_set(acc);
    i.local_get(start).i32_const(1).i32_add().local_set(start);
    i.br(0).end().end();
    i.local_get(acc);
    i.end();
    f
}

/// Answer the err of `fns.err` from four pieces around the quoted URL
/// (`a_ptr`, `a_len`) and a raw run (`r_ptr`, `r_len` locals, or none).
fn http_err_call(
    i: &mut wasm_encoder::InstructionSink<'_>,
    fns: HttpErrFns,
    url: (u32, u32),
    p1: Piece,
    p2: Piece,
    raw: Option<(u32, u32)>,
    p3: Piece,
) {
    i.i32_const(p1.at).i32_const(p1.len);
    i.local_get(url.0).local_get(url.1);
    i.i32_const(p2.at).i32_const(p2.len);
    match raw {
        Some((rp, rn)) => {
            i.local_get(rp).local_get(rn);
        }
        None => {
            i.i32_const(0).i32_const(0);
        }
    }
    i.i32_const(p3.at).i32_const(p3.len);
    i.call(fns.err).return_();
}

/// Answer the err of an error-code table entry at `entry` (a local):
/// head, the quoted URL, tail, the case name, the closing text.
fn http_err_entry(i: &mut wasm_encoder::InstructionSink<'_>, fns: HttpErrFns, url: (u32, u32), entry: u32) {
    i.local_get(entry).i32_load(mem(0)).local_get(entry).i32_load(mem(4));
    i.local_get(url.0).local_get(url.1);
    for off in [8u64, 12, 16, 20, 24, 28] {
        i.local_get(entry).i32_load(mem(off));
    }
    i.call(fns.err).return_();
}

/// The locals `http_url_scheme` works in (the URL is params 1 and 2).
#[derive(Clone, Copy)]
struct UrlLocals {
    sch: u32,
    rest: u32,
    colon: u32,
    valid: u32,
    b: u32,
    k: u32,
    /// The lowercased scheme's buffer (an unsupported scheme's text).
    low: u32,
}

/// The scheme half of rt-core's `http_parse_url`, with its three reasons:
/// the first `:` ends the scheme; a token (a letter, then letters, digits,
/// `+`, `-`, `.`) followed by `//` is a scheme, and must be http or https
/// (ASCII case-insensitive) — `rest` then starts past the `//`; an http /
/// https spelling without the `//` is `expected "//" after "<it>:"`; and
/// anything else has no scheme. Authority reasons are rt-core's alone.
fn http_url_scheme(
    i: &mut wasm_encoder::InstructionSink<'_>,
    h: &HttpAbi,
    t: &HttpErrTexts,
    fns: HttpErrFns,
    f_alloc: u32,
    l: UrlLocals,
) {
    let (a_ptr, a_len) = (1u32, 2u32);
    let UrlLocals { sch, rest, colon, valid, b, k, low } = l;
    let byte = |i: &mut wasm_encoder::InstructionSink<'_>, at: u32| {
        i.local_get(a_ptr).local_get(at).i32_add().i32_load8_u(mem8(0));
    };
    i.i32_const(0).local_set(k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).local_get(a_len).i32_ge_u().br_if(1);
    byte(i, k);
    i.i32_const(':' as i32).i32_eq().br_if(1);
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    i.local_get(k).local_set(colon);
    // valid = a token before the colon, then `//`.
    i.local_get(colon).i32_const(0).i32_gt_u();
    i.local_get(a_ptr).i32_load8_u(mem8(0)).i32_const(0x20).i32_or().i32_const('a' as i32).i32_sub();
    i.i32_const(26).i32_lt_u().i32_and().local_set(valid);
    i.i32_const(1).local_set(k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).local_get(colon).i32_ge_u().br_if(1);
    byte(i, k);
    i.local_set(b);
    i.local_get(b).i32_const('0' as i32).i32_sub().i32_const(10).i32_lt_u();
    i.local_get(b).i32_const(0x20).i32_or().i32_const('a' as i32).i32_sub().i32_const(26).i32_lt_u();
    i.i32_or();
    for c in b"+-." {
        i.local_get(b).i32_const(*c as i32).i32_eq().i32_or();
    }
    i.i32_eqz().if_(BlockType::Empty);
    i.i32_const(0).local_set(valid);
    i.end();
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    i.local_get(colon).i32_const(3).i32_add().local_get(a_len).i32_le_u().if_(BlockType::Result(ValType::I32));
    i.local_get(a_ptr).local_get(colon).i32_add().i32_load8_u(mem8(1)).i32_const('/' as i32).i32_eq();
    i.local_get(a_ptr).local_get(colon).i32_add().i32_load8_u(mem8(2)).i32_const('/' as i32).i32_eq();
    i.i32_and();
    i.else_().i32_const(0).end();
    i.local_get(valid).i32_and().local_set(valid);
    // b = 1 for an http spelling, 2 for https, 0 otherwise (colon found).
    i.i32_const(0).local_set(b);
    for (word, tag) in [(&b"http"[..], 1), (&b"https"[..], 2)] {
        i.local_get(colon).i32_const(word.len() as i32).i32_eq();
        i.local_get(colon).local_get(a_len).i32_lt_u().i32_and();
        for (j, c) in word.iter().enumerate() {
            i.local_get(a_ptr).i32_load8_u(mem8(j as u64)).i32_const(0x20).i32_or();
            i.i32_const(*c as i32).i32_eq().i32_and();
        }
        i.if_(BlockType::Empty);
        i.i32_const(tag).local_set(b);
        i.end();
    }
    i.local_get(valid).if_(BlockType::Empty);
    i.local_get(b).i32_eqz().if_(BlockType::Empty);
    // An unsupported scheme, named lowercased.
    i.i32_const(0).i32_const(0).i32_const(1).local_get(colon).call(f_alloc).local_set(low);
    i.i32_const(0).local_set(k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).local_get(colon).i32_ge_u().br_if(1);
    i.local_get(low).local_get(k).i32_add();
    byte(i, k);
    i.local_set(b);
    i.local_get(b).i32_const(32).i32_add();
    i.local_get(b);
    i.local_get(b).i32_const('A' as i32).i32_sub().i32_const(26).i32_lt_u();
    i.select().i32_store8(mem8(0));
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    http_err_call(i, fns, (a_ptr, a_len), t.url_head, t.url_scheme[0], Some((low, colon)), t.url_scheme[1]);
    i.end();
    i.local_get(b).i32_const(2).i32_eq().if_(BlockType::Result(ValType::I32));
    i.i32_const(h.sch_https);
    i.else_().i32_const(h.sch_http).end();
    i.local_set(sch);
    i.local_get(colon).i32_const(3).i32_add().local_set(rest);
    i.else_();
    i.local_get(b).if_(BlockType::Empty);
    http_err_call(i, fns, (a_ptr, a_len), t.url_head, t.url_slashes[0], Some((a_ptr, colon)), t.url_slashes[1]);
    i.end();
    http_err_call(i, fns, (a_ptr, a_len), t.url_head, t.url_no_scheme, None, Piece::default());
    i.end();
}

/// Refuse the framed family's headers before any resource exists (C-370):
/// each key/value pair of the frame through `$http_hdr_check`, the first
/// refusal answered in its text with the name quoted.
#[allow(clippy::too_many_arguments)]
fn http_check_headers(
    i: &mut wasm_encoder::InstructionSink<'_>,
    t: &HttpErrTexts,
    fns: HttpErrFns,
    (hdr_ptr, frame_end): (u32, u32),
    (cur, cell_len, tmp, digit): (u32, u32, u32, u32),
    (key_ptr, key_len): (u32, u32),
    n: u32,
) {
    i.local_get(0).i32_const(48).i32_ge_s().if_(BlockType::Empty);
    i.local_get(hdr_ptr).local_set(cur);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(cur).local_get(frame_end).i32_ge_u().br_if(1);
    http_frame_cell(i, cur, cell_len, frame_end, tmp, digit);
    i.local_get(tmp).local_set(key_ptr);
    i.local_get(cur).local_get(tmp).i32_sub().local_set(key_len);
    i.local_get(cur).local_get(frame_end).i32_ge_u().br_if(1);
    http_frame_cell(i, cur, cell_len, frame_end, tmp, digit);
    i.local_get(key_ptr).local_get(key_len).local_get(tmp).local_get(cur).local_get(tmp).i32_sub();
    i.call(fns.check).local_set(n);
    for (code, [head, tail]) in t.hdr.iter().enumerate() {
        i.local_get(n).i32_const(code as i32 + 1).i32_eq().if_(BlockType::Empty);
        http_err_call(i, fns, (key_ptr, key_len), *head, *tail, None, Piece::default());
        i.end();
    }
    i.br(0).end().end();
    i.end();
}

/// A failed `send`: its `error-code` in the SENDRET slot, classified —
/// `internal-error(some(msg))` answers `msg` as is, every other case its
/// table entry (a class's text, or the unclassified one naming the case).
fn http_send_err(i: &mut wasm_encoder::InstructionSink<'_>, park: u64, h: &HttpAbi, t: &HttpErrTexts, fns: HttpErrFns, ent: u32) {
    let ec = park + SENDRET + h.send_payload;
    let opt = ec + h.ec_payload;
    i.i32_const(ec as i32).i32_load8_u(mem8(0)).local_set(ent);
    i.local_get(ent).i32_const(h.ec_internal).i32_eq();
    i.i32_const(opt as i32).i32_load8_u(mem8(0)).i32_const(0).i32_ne().i32_and();
    i.if_(BlockType::Empty);
    i.i32_const(0).i32_const(0).i32_const(0).i32_const(-1).i32_const(0).i32_const(0);
    i.i32_const((opt + h.ec_opt_str) as i32).i32_load(mem(0));
    i.i32_const((opt + h.ec_opt_str) as i32).i32_load(mem(4));
    i.i32_const(0).i32_const(0);
    i.call(fns.err).return_();
    i.end();
    i.local_get(ent).i32_const(EC_ENTRY).i32_mul().i32_const(t.ec_table).i32_add().local_set(ent);
    http_err_entry(i, fns, (1, 2), ent);
}

/// Write the decimal of the i64 local `v` at `at` (back to front, the
/// content-length rendering's shape), its digit count in `len`; `scratch`
/// is an i64 local and `cur` an i32 one.
fn http_decimal_i64(i: &mut wasm_encoder::InstructionSink<'_>, at: i32, v: u32, scratch: u32, len: u32, cur: u32) {
    i.i32_const(1).local_set(len);
    i.local_get(v).local_set(scratch);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(scratch).i64_const(10).i64_lt_u().br_if(1);
    i.local_get(scratch).i64_const(10).i64_div_u().local_set(scratch);
    i.local_get(len).i32_const(1).i32_add().local_set(len);
    i.br(0).end().end();
    i.local_get(v).local_set(scratch);
    i.local_get(len).local_set(cur);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(cur).i32_eqz().br_if(1);
    i.local_get(cur).i32_const(1).i32_sub().local_set(cur);
    i.i32_const(at).local_get(cur).i32_add();
    i.local_get(scratch).i64_const(10).i64_rem_u().i32_wrap_i64().i32_const(48).i32_add();
    i.i32_store8(mem8(0));
    i.local_get(scratch).i64_const(10).i64_div_u().local_set(scratch);
    i.br(0).end().end();
}
