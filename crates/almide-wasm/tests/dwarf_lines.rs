//! #1315: a `--debug` build's DWARF line table maps a code offset of the
//! SHIPPED (stock-WASI) module back to the `.almd` file:line it came from.
//!
//! The probe is independent of the table's own bookkeeping: the call site is
//! found by scanning the shipped bytes for a `call` to an exported function,
//! its address is computed from the Code section, and the row covering it is
//! read back through gimli's reader — the same parse a debugger does.

use std::collections::BTreeMap;

use almide::wasm_route::{route_wasm, ModuleSource, RouteOptions};
use almide_wasm::debug_lines::{attach_dwarf, DebugLinesGuard, DwarfIn, LineTable};
use gimli::{EndianSlice, LittleEndian};

const PROBE: &str = r#"pub fn fib(n: Int) -> Int = if n < 2 then n else fib(n - 1) + fib(n - 2)

pub fn sum_to(n: Int) -> Int = {
  if n < 0 then panic("negative n") else ()
  var acc = 0
  for i in 0..<n {
    acc = acc + fib(i + 1)
  }
  acc
}

effect fn main() -> Unit = {
  let total = sum_to(5)
  println("total ${total}")
}
"#;

/// The call to `fib` in the loop of `sum_to`: (line, column). It sits behind
/// a `panic` whose `unreachable` the stock-WASI transform puts a `proc_exit`
/// call in front of, so its shipped offset is NOT its emitted one.
const LOOP_CALL: (u64, u64) = (7, 17);
/// `fib`'s own line: the accumulator rewrite (#2577) keeps one of its two
/// recursive calls, carrying the span of the `if` it rebuilt.
const FIB_LINE: u64 = 1;

struct Built {
    plain: Vec<u8>,
    shipped: Vec<u8>,
    table: LineTable,
}

/// Emit `path` twice — plain, and under the recording guard — and ship the
/// recorded one in its stock-WASI form with the line table attached.
fn build(path: &str) -> Built {
    let src = std::fs::read_to_string(path).expect("fixture reads");
    let route = |src: &str| {
        let opts = RouteOptions { library: true, ..RouteOptions::default() };
        route_wasm(path, src, ModuleSource::Disk { dep_paths: &[] }, None, opts, &mut |_| {}).expect("routes")
    };
    let plain = route(&src);
    let plain = plain.stock_wasi().expect("plain wasi");
    let (emitted, table) = {
        let _g = DebugLinesGuard::set();
        let m = route(&src);
        (m, almide_wasm::debug_lines::take_table().expect("a table under the guard"))
    };
    let (wasi, def_map) = almide_wasm_run::wasi::to_wasi_mapped(&emitted.bytes, &emitted.host_ops).expect("wasi");
    assert_eq!(wasi, plain, "recording lines must not change the emitted code");
    let input = DwarfIn {
        emitted: &emitted.bytes,
        shipped: &wasi,
        def_map: Some(&def_map),
        table: &table,
        entry_file: path,
        comp_dir: "/",
        producer: "almide test",
    };
    let (shipped, stats) = attach_dwarf(&input).expect("dwarf attaches");
    assert_eq!(stats.skipped, 0, "every recorded, shipped function aligns");
    wasmparser::validate(&shipped).expect("the debug module validates");
    Built { plain, shipped, table }
}

fn custom_sections(bytes: &[u8]) -> BTreeMap<String, &[u8]> {
    let mut out = BTreeMap::new();
    for p in wasmparser::Parser::new(0).parse_all(bytes) {
        if let wasmparser::Payload::CustomSection(c) = p.expect("parses") {
            out.insert(c.name().to_string(), c.data());
        }
    }
    out
}

/// The code-section offset of every `call` to the function exported as
/// `callee`.
fn call_sites(bytes: &[u8], callee: &str) -> Vec<u64> {
    use wasmparser::{Operator, Payload};
    let (mut imports, mut target, mut code_start, mut sites) = (0u32, None, 0u64, Vec::new());
    for p in wasmparser::Parser::new(0).parse_all(bytes) {
        match p.expect("parses") {
            Payload::ImportSection(r) => imports += r.into_imports().count() as u32,
            Payload::ExportSection(r) => {
                for e in r {
                    let e = e.expect("export");
                    if e.name == callee {
                        target = Some(e.index);
                    }
                }
            }
            Payload::CodeSectionStart { range, .. } => code_start = range.start,
            Payload::CodeSectionEntry(body) => {
                let mut ops = body.get_operators_reader().expect("ops");
                while !ops.eof() {
                    let at = ops.original_position();
                    if let Operator::Call { function_index } = ops.read().expect("op")
                        && Some(function_index) == target
                    {
                        sites.push(at - code_start);
                    }
                }
            }
            _ => {}
        }
    }
    assert!(target.is_some_and(|t| t >= imports), "`{callee}` is an exported defined function");
    sites
}

/// The (file, line, column) the line program assigns to `addr`.
fn line_of(bytes: &[u8], addr: u64) -> Option<(String, u64, u64)> {
    let sections = custom_sections(bytes);
    let load = |id: gimli::SectionId| -> Result<EndianSlice<'_, LittleEndian>, gimli::Error> {
        Ok(EndianSlice::new(sections.get(id.name()).copied().unwrap_or(&[]), LittleEndian))
    };
    let dwarf = gimli::Dwarf::load(load).expect("dwarf loads");
    let mut units = dwarf.units();
    while let Some(header) = units.next().expect("unit header") {
        let unit = dwarf.unit(header).expect("unit");
        let Some(program) = unit.line_program.clone() else { continue };
        let mut rows = program.rows();
        let mut prev: Option<(u64, u64, u64, u64)> = None; // (address, file, line, column)
        while let Some((header, row)) = rows.next_row().expect("row") {
            if let Some((a, file, line, col)) = prev
                && a <= addr
                && addr < row.address()
            {
                let entry = header.file(file).expect("file entry");
                let name = dwarf.attr_string(&unit, entry.path_name()).expect("file name");
                return Some((name.to_string_lossy().into_owned(), line, col));
            }
            let col = match row.column() {
                gimli::ColumnType::LeftEdge => 0,
                gimli::ColumnType::Column(c) => c.get(),
            };
            prev = (!row.end_sequence()).then(|| (row.address(), row.file_index(), row.line().map_or(0, |l| l.get()), col));
        }
    }
    None
}

#[test]
fn a_call_offset_in_the_shipped_module_maps_to_its_almd_line() {
    let dir = std::env::temp_dir().join(format!("almide-dwarf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("probe.almd");
    std::fs::write(&path, PROBE).expect("probe writes");
    let path = path.to_string_lossy().into_owned();
    let built = build(&path);

    let sections = custom_sections(&built.shipped);
    for name in [".debug_info", ".debug_abbrev", ".debug_line", ".debug_str"] {
        assert!(sections.contains_key(name), "{name} ships: {:?}", sections.keys().collect::<Vec<_>>());
    }
    assert!(custom_sections(&built.plain).is_empty(), "the default build carries no debug sections");
    // The sections are appended: every byte of the plain module is where it was.
    assert_eq!(&built.shipped[..built.plain.len()], &built.plain[..]);

    let sites = call_sites(&built.shipped, "fib");
    let lines: Vec<(String, u64, u64)> = sites.iter().map(|&a| line_of(&built.shipped, a).expect("a row covers the call")).collect();
    assert!(lines.iter().all(|(file, ..)| *file == path), "{lines:?}");
    let mut distinct: Vec<u64> = lines.iter().map(|l| l.1).collect();
    distinct.dedup();
    assert_eq!(distinct, [FIB_LINE, LOOP_CALL.0], "calls to fib at {sites:x?} map to {lines:?}");
    let in_loop: Vec<_> = lines.iter().filter(|l| l.1 == LOOP_CALL.0).map(|l| (l.1, l.2)).collect();
    assert_eq!(in_loop, [LOOP_CALL], "the loop's call is `fib(` itself, to the column");
    let named: Vec<&str> = built.table.fns.iter().map(|f| f.name.as_str()).collect();
    for f in ["fib", "sum_to", "main"] {
        assert!(named.contains(&f), "{f} recorded: {named:?}");
    }

    // The interpreter skips the custom sections and runs the module as before.
    let run = |bytes: &[u8]| {
        let (mut input, mut out, mut err) = (&b""[..], Vec::new(), Vec::new());
        let exit = almide_wasm_vm::run_program(bytes, almide_wasm_vm::Limits::default(), &mut input, &mut out, &mut err)
            .expect("the VM loads it")
            .exit;
        (exit, out, err)
    };
    assert_eq!(run(&built.shipped), run(&built.plain));
    assert_eq!(run(&built.shipped).1, b"total 12\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The span resolution the line table can have: per recorded function, the
/// share of its instructions with a known line, and of those, the share whose
/// innermost IR node carried the span itself (not inherited). A report:
/// `cargo test -p almide-wasm --test dwarf_lines -- --ignored --nocapture`,
/// with `ALMIDE_DWARF_FIXTURES` naming the files (comma-separated).
#[test]
#[ignore = "a measurement, not a gate"]
fn span_resolution_report() {
    let list = std::env::var("ALMIDE_DWARF_FIXTURES").unwrap_or_else(|_| "spec/wasm_cross/binary_operators.almd".into());
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let (mut all, mut all_known, mut all_own) = ([0usize; 2], [0usize; 2], [0usize; 2]);
    for rel in list.split(',') {
        let built = build(&format!("{root}/{rel}"));
        println!("== {rel}");
        for f in &built.table.fns {
            let n = f.ops.len();
            let known = f.ops.iter().flatten().count();
            let own = f.ops.iter().flatten().filter(|l| l.own).count();
            let unit = if f.space == 0 { 0 } else { 1 };
            all[unit] += n;
            all_known[unit] += known;
            all_own[unit] += own;
            println!(
                "  {:<40} {:>6} ops  known {:>5.1}%  own {:>5.1}%  [{}]",
                f.name,
                n,
                100.0 * known as f64 / n.max(1) as f64,
                100.0 * own as f64 / n.max(1) as f64,
                built.table.units[f.space as usize]
            );
        }
    }
    for (u, label) in ["entry file", "linked modules"].iter().enumerate() {
        println!(
            "TOTAL {label}: {} ops, known {:.1}%, own {:.1}%",
            all[u],
            100.0 * all_known[u] as f64 / all[u].max(1) as f64,
            100.0 * all_own[u] as f64 / all[u].max(1) as f64
        );
    }
}
