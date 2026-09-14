//! The emitted module's static byte runs may not overlap (#2090).
//!
//! Every `(data (i32.const N) "...")` address in the wasm preamble was assigned
//! by hand, and the ranges recorded next to them (`// 100..112`, `// 240..376`)
//! are prose. Nothing read them. Two strings placed on top of each other produce
//! no error at any stage — the second `(data)` simply overwrites the first's
//! bytes, and the failure surfaces as a truncated or spliced ERROR MESSAGE at
//! runtime, in the arm a test is least likely to cover.
//!
//! That is the risk that made #2090's wasm half worth refusing to rush, so it is
//! retired here rather than navigated. `static_data_regions()` is the map as
//! DATA; this asserts the properties the comments used to assert informally.

use almide_mir::render_wasm::{static_data_ceiling, static_data_regions, PRINT_NL_SCRATCH_ADDR};

#[test]
fn no_two_static_regions_overlap() {
    let mut regions = static_data_regions();
    regions.sort_by_key(|(_, addr, _)| *addr);

    let mut clashes = Vec::new();
    for pair in regions.windows(2) {
        let (ref a_name, a_addr, a_len) = pair[0];
        let (ref b_name, b_addr, _) = pair[1];
        let a_end = a_addr + a_len;
        if a_end > b_addr {
            clashes.push(format!(
                "  {a_name} [{a_addr}..{a_end}) runs into {b_name} at {b_addr} \
                 — {} byte(s) of {b_name} are overwritten",
                a_end - b_addr
            ));
        }
    }
    assert!(
        clashes.is_empty(),
        "static data regions overlap; the later (data) silently overwrites the earlier:\n{}",
        clashes.join("\n")
    );
}

/// The whole map has to stay below the allocator, or the first allocation
/// scribbles over the message bytes.
#[test]
fn every_static_region_stays_below_the_heap() {
    let ceiling = static_data_ceiling();
    let over: Vec<String> = static_data_regions()
        .into_iter()
        .filter(|(_, addr, len)| addr + len > ceiling)
        .map(|(name, addr, len)| format!("  {name} [{addr}..{}) >= {ceiling}", addr + len))
        .collect();
    assert!(
        over.is_empty(),
        "static data runs into the heap/mutable-global region:\n{}",
        over.join("\n")
    );
}

/// The #2090 pieces are derived from one base rather than hand-placed, so this
/// pins the property that makes that worth doing: each starts 4-byte aligned and
/// the run is contiguous and ascending. A row inserted in the middle of
/// `FS_MSG_PREFIXES` (instead of appended) still satisfies this — it is the
/// overlap test above that catches a stale hard-coded address, which is why both
/// exist.
#[test]
fn the_message_pieces_are_packed_in_order() {
    let regions: Vec<_> = static_data_regions()
        .into_iter()
        .filter(|(name, _, _)| name.starts_with("FS_MSG "))
        .collect();
    assert!(
        regions.len() >= 8,
        "expected the prefix table plus the shared mid, got {}",
        regions.len()
    );
    for pair in regions.windows(2) {
        let (ref a, a_addr, a_len) = pair[0];
        let (ref b, b_addr, _) = pair[1];
        assert!(b_addr > a_addr, "{b} is not after {a}");
        assert_eq!(b_addr % 4, 0, "{b} is not 4-byte aligned");
        assert_eq!(
            b_addr,
            a_addr + ((a_len + 3) & !3),
            "{b} is not packed directly after {a}"
        );
    }
}

/// The preamble's data segments must not contain a backslash-escaped quote.
///
/// `render_wasm_dce::match_paren` skips string literals by toggling on every
/// `"`, and its header states the invariant: *"this codebase's WAT output has no
/// backslash-escaped quotes"*. The #2090 prefixes are the first preamble strings
/// that CONTAIN a quote, and rendering them with `{:?}` broke that invariant.
///
/// The failure mode is why this is gated rather than remembered: nothing errored.
/// The scan desynced, every later data segment was dropped as unreachable, the
/// message copied zeroed memory, and the program printed
/// `err=              nope-missing.txt    No such file...` — right lengths,
/// blank content — on the stock-WASI artifact only. A quote must assemble as the
/// `\22` hex byte instead.
#[test]
fn no_preamble_data_segment_escapes_a_quote_with_a_backslash() {
    let wat = almide_mir::render_wasm::preamble_text_for_gates();
    let mut bad = Vec::new();
    for (n, line) in wat.lines().enumerate() {
        let t = line.trim_start();
        if t.starts_with("(data (i32.const ") && t.contains("\\\"") {
            bad.push(format!("  line {}: {t}", n + 1));
        }
    }
    assert!(
        bad.is_empty(),
        "a data segment escapes a quote as \\\" — render_wasm_dce's paren scan \
         desyncs there and silently drops the segments after it. Use the \\\\22 hex \
         byte (wat_data_literal):\n{}",
        bad.join("\n")
    );
}

/// The #2090 pieces actually reach the emitted preamble. The layout tests above
/// prove the ADDRESSES are sane; this proves the BYTES are present, which is the
/// half the silent-drop bug broke.
#[test]
fn every_message_piece_is_emitted_as_a_data_segment() {
    let wat = almide_mir::render_wasm::preamble_text_for_gates();
    let missing: Vec<String> = static_data_regions()
        .into_iter()
        .filter(|(name, _, _)| name.starts_with("FS_MSG "))
        .filter(|(_, addr, _)| !wat.contains(&format!("(data (i32.const {addr}) ")))
        .map(|(name, addr, _)| format!("  {name} at {addr}"))
        .collect();
    assert!(
        missing.is_empty(),
        "message pieces have addresses but no bytes — the copy would read zeros:\n{}",
        missing.join("\n")
    );
}

/// #2206: every errno the table spells is emitted as a data segment with its
/// EXACT text, so the incumbent's `err(e)` bytes are the table's — the same
/// bytes `crates/almide-base/tests/fs_errno_table.rs` pins against the host's
/// `std::io::Error` `Display`.
#[test]
fn every_fs_errno_row_is_emitted_as_a_data_segment() {
    let wat = almide_mir::render_wasm::preamble_text_for_gates();
    for row in almide_base::fs_errno::FS_ERRNOS {
        let needle = format!("{:?}", row.text);
        assert!(
            wat.contains(&needle),
            "{}: the preamble carries no data segment for {needle}",
            row.name
        );
    }
    let errno_rows = static_data_regions()
        .into_iter()
        .filter(|(name, _, _)| name.starts_with("FS_ERR_"))
        .count();
    assert_eq!(
        errno_rows,
        almide_base::fs_errno::FS_ERRNOS.len() + 2,
        "the layout map lists one region per table row plus WRITEZERO and UTF8"
    );
}

/// The self-hosted stdlib pokes linear memory at LITERAL addresses (`prim.store8(768, 10)`
/// is println's newline). Almide source cannot read the preamble's constants, so
/// every such literal must land inside a REGISTERED region of the map — the
/// print scratch is `PRINT_NL_SCRATCH`, the iovec/nwritten slots are `IOVEC` /
/// `NWRITTEN`. A literal outside the map is the #2206 defect class: a byte the
/// overlap gate cannot see, which a derived row will one day be laid over.
#[test]
fn every_literal_address_the_stdlib_pokes_is_a_registered_region() {
    let regions = static_data_regions();
    let covered = |addr: u32| regions.iter().any(|(_, a, l)| addr >= *a && addr < a + l);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib");
    let mut seen_scratch = 0;
    let mut stray = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("stdlib dir") {
        let path = entry.expect("entry").path();
        if path.extension().is_none_or(|e| e != "almd") {
            continue;
        }
        let src = std::fs::read_to_string(&path).expect("read");
        for (ln, line) in src.lines().enumerate() {
            let Some(pos) = line.find("prim.store") else { continue };
            let Some(open) = line[pos..].find('(') else { continue };
            let args = &line[pos + open + 1..];
            let Some(comma) = args.find(',') else { continue };
            let Ok(addr) = args[..comma].trim().parse::<u32>() else { continue };
            if addr == PRINT_NL_SCRATCH_ADDR && args[comma + 1..].trim().starts_with("10)") {
                seen_scratch += 1;
            }
            if !covered(addr) {
                stray.push(format!("  {}:{}: {}", path.display(), ln + 1, line.trim()));
            }
        }
    }
    assert!(
        stray.is_empty(),
        "stdlib literal addresses outside the static-data map:\n{}",
        stray.join("\n")
    );
    assert_eq!(
        seen_scratch, 2,
        "print_str and eprintln each store their newline at PRINT_NL_SCRATCH ({PRINT_NL_SCRATCH_ADDR})"
    );
}

/// The derived errno rows sit above EVERY fixed region (the property that makes
/// a new fixed row move the table instead of colliding with it).
#[test]
fn the_errno_rows_are_laid_out_above_every_fixed_region() {
    let regions = static_data_regions();
    let fixed_end = regions
        .iter()
        .filter(|(n, _, _)| !n.starts_with("FS_ERR_") || n == "FS_ERR_WRITEZERO" || n == "FS_ERR_UTF8")
        .map(|(_, a, l)| a + l)
        .max()
        .expect("fixed rows");
    let first_row = regions
        .iter()
        .filter(|(n, _, _)| {
            n.starts_with("FS_ERR_") && n != "FS_ERR_WRITEZERO" && n != "FS_ERR_UTF8"
        })
        .map(|(_, a, _)| *a)
        .min()
        .expect("errno rows");
    assert!(first_row >= fixed_end, "errno rows start at {first_row}, fixed regions end at {fixed_end}");
    assert!(first_row > PRINT_NL_SCRATCH_ADDR, "the rows must clear the print scratch");
}
