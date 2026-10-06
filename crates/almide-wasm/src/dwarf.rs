//! #1315: the recorded lines as DWARF custom sections of the SHIPPED module.
//!
//! The "DWARF for WebAssembly" convention (the one LLVM/Emscripten emit and
//! Chrome DevTools, wasmtime and lldb read): 32-bit addresses, and an
//! address is a byte offset from the start of the Code section's CONTENTS
//! (the function-count LEB included). A function's `DW_AT_low_pc` is the
//! start of its body (its locals vector, right after the body-size LEB),
//! `DW_AT_high_pc` its length; the line rows address instructions.
//!
//! The record names instructions by ordinal in the EMITTED module; the
//! bytes that ship may be a re-encoding of it (the stock-WASI transform
//! renumbers calls, puts `proc_exit` in front of every `unreachable`, prunes
//! dead functions). Each recorded function is found in the shipped module
//! through `def_map` (the transform's defined-function map), and its
//! instructions are aligned to the shipped ones in order, an inserted
//! instruction taking the line of the instruction it was inserted before. A
//! function whose alignment fails ships without rows (counted in
//! [`DwarfStats::skipped`]) — a row that names the wrong line is worse than
//! none.

use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use gimli::write::{
    Address, AttributeValue, Dwarf, EndianVec, LineProgram, LineString, Range, RangeList, Sections,
};
use gimli::{constants, Encoding, Format, LineEncoding, LittleEndian};

use super::{FnLines, LineTable, Loc};

/// The DWARF version written: 4 is what every wasm consumer reads.
const DWARF_VERSION: u16 = 4;

/// What [`attach_dwarf`] reads.
pub struct DwarfIn<'a> {
    /// The module the emitter produced (the record's ordinals refer to it).
    pub emitted: &'a [u8],
    /// The module that ships (the emitted one, or its WASI form).
    pub shipped: &'a [u8],
    /// Emitted defined-function position → shipped defined-function
    /// position (`None`: pruned); `None` when `shipped` is `emitted`.
    pub def_map: Option<&'a [Option<u32>]>,
    pub table: &'a LineTable,
    /// The entry file as the build named it.
    pub entry_file: &'a str,
    /// `DW_AT_comp_dir`: the directory relative paths resolve against.
    pub comp_dir: &'a str,
    /// `DW_AT_producer` (the compiler and its version).
    pub producer: &'a str,
}

/// What a line table covers.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DwarfStats {
    /// Functions given a subprogram and a line sequence.
    pub functions: usize,
    /// Recorded functions dropped (pruned, or the alignment failed).
    pub skipped: usize,
    /// Line rows written.
    pub rows: usize,
    /// Bytes of custom sections appended.
    pub bytes: usize,
}

struct Body {
    /// Absolute offsets: the body's start (its locals) and end.
    start: usize,
    end: usize,
    /// (absolute offset, operator kind) per instruction.
    ops: Vec<(usize, u64)>,
}

struct Parsed {
    func_imports: u32,
    code_start: usize,
    bodies: Vec<Body>,
}

fn kind(op: &wasmparser::Operator<'_>) -> u64 {
    let mut h = DefaultHasher::new();
    std::mem::discriminant(op).hash(&mut h);
    h.finish()
}

fn parse(bytes: &[u8]) -> Result<Parsed, String> {
    use wasmparser::{Payload, TypeRef};
    let mut p = Parsed { func_imports: 0, code_start: 0, bodies: Vec::new() };
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        match payload.map_err(|e| e.to_string())? {
            Payload::ImportSection(r) => {
                for i in r.into_imports() {
                    if matches!(i.map_err(|e| e.to_string())?.ty, TypeRef::Func(_)) {
                        p.func_imports += 1;
                    }
                }
            }
            Payload::CodeSectionStart { range, .. } => p.code_start = range.start as usize,
            Payload::CodeSectionEntry(body) => {
                // `range()` is the body after its size LEB: the locals
                // vector first, which is the function's address.
                let range = body.range();
                let mut ops = body.get_operators_reader().map_err(|e| e.to_string())?;
                let mut v = Vec::new();
                while !ops.eof() {
                    let at = ops.original_position() as usize;
                    v.push((at, kind(&ops.read().map_err(|e| e.to_string())?)));
                }
                p.bodies.push(Body { start: range.start as usize, end: range.end as usize, ops: v });
            }
            _ => {}
        }
    }
    Ok(p)
}

/// `src` instructions in order inside `dst` (which may hold more): the
/// position of each, or `None` when `dst` is not such a superset.
fn align(src: &[u64], dst: &[u64]) -> Option<Vec<usize>> {
    let mut j = 0;
    let mut out = Vec::with_capacity(src.len());
    for k in src {
        while j < dst.len() && dst[j] != *k {
            j += 1;
        }
        if j == dst.len() {
            return None;
        }
        out.push(j);
        j += 1;
    }
    Some(out)
}

/// One shipped function's rows: (offset from its body start, position).
struct Seq<'a> {
    f: &'a FnLines,
    start: u64,
    len: u64,
    rows: Vec<(u64, Option<Loc>)>,
}

fn sequence<'a>(f: &'a FnLines, em: &Parsed, sh: &Parsed, def_map: Option<&[Option<u32>]>) -> Option<Seq<'a>> {
    let def = f.index.checked_sub(em.func_imports)? as usize;
    let src = em.bodies.get(def)?;
    let to = match def_map {
        Some(m) => (*m.get(def)?)? as usize,
        None => def,
    };
    let dst = sh.bodies.get(to)?;
    if src.ops.len() != f.ops.len() {
        return None;
    }
    let src_kinds: Vec<u64> = src.ops.iter().map(|o| o.1).collect();
    let dst_kinds: Vec<u64> = dst.ops.iter().map(|o| o.1).collect();
    let at = align(&src_kinds, &dst_kinds)?;
    let mut rows: Vec<(u64, Option<Loc>)> = Vec::new();
    for (k, loc) in f.ops.iter().enumerate() {
        // An instruction inserted in front of `k` takes `k`'s line.
        let first = if k == 0 { 0 } else { at[k - 1] + 1 };
        let off = (dst.ops[first].0 - dst.start) as u64;
        let same = rows.last().is_some_and(|(_, l)| same_line(l, loc));
        if !same {
            rows.push((off, *loc));
        }
    }
    Some(Seq { f, start: (dst.start - sh.code_start) as u64, len: (dst.end - dst.start) as u64, rows })
}

fn same_line(a: &Option<Loc>, b: &Option<Loc>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.line == b.line && a.col == b.col,
        (None, None) => true,
        _ => false,
    }
}

/// The file a var space's source lives in.
fn file_of(t: &LineTable, space: u32, entry: &str) -> String {
    match t.units.get(space as usize).map(String::as_str) {
        None | Some("") => entry.to_string(),
        Some(m) => t.files.get(m).cloned().unwrap_or_else(|| format!("stdlib/{m}.almd")),
    }
}

fn s(v: &str) -> LineString {
    LineString::String(v.as_bytes().to_vec())
}

/// `shipped` with `.debug_info` / `.debug_abbrev` / `.debug_line` /
/// `.debug_str` / `.debug_ranges` appended as custom sections: one compile
/// unit per source file, one subprogram per recorded function.
pub fn attach_dwarf(input: &DwarfIn<'_>) -> Result<(Vec<u8>, DwarfStats), String> {
    let em = parse(input.emitted)?;
    let sh = if std::ptr::eq(input.emitted, input.shipped) { None } else { Some(parse(input.shipped)?) };
    let sh_ref = sh.as_ref().unwrap_or(&em);
    let mut stats = DwarfStats::default();
    let mut by_file: BTreeMap<String, Vec<Seq<'_>>> = BTreeMap::new();
    for f in &input.table.fns {
        match sequence(f, &em, sh_ref, input.def_map) {
            Some(seq) => by_file.entry(file_of(input.table, f.space, input.entry_file)).or_default().push(seq),
            None => stats.skipped += 1,
        }
    }
    let encoding = Encoding { format: Format::Dwarf32, version: DWARF_VERSION, address_size: 4 };
    let mut dwarf = Dwarf::new();
    let producer = dwarf.strings.add(input.producer);
    for (file, seqs) in &by_file {
        let mut lp = LineProgram::new(encoding, LineEncoding::default(), s(input.comp_dir), None, s(file), None);
        let file_id = lp.add_file(s(file), lp.default_directory(), None);
        for seq in seqs {
            lp.begin_sequence(Some(Address::Constant(seq.start)));
            for (off, loc) in &seq.rows {
                let row = lp.row();
                row.address_offset = *off;
                row.file = file_id;
                row.line = loc.map_or(0, |l| u64::from(l.line));
                row.column = loc.map_or(0, |l| u64::from(l.col));
                row.is_statement = loc.is_some_and(|l| l.own);
                lp.generate_row();
                stats.rows += 1;
            }
            lp.end_sequence(seq.len);
        }
        let unit_id = dwarf.units.add(gimli::write::Unit::new(encoding, lp));
        let unit = dwarf.units.get_mut(unit_id);
        let ranges = RangeList(
            seqs.iter().map(|q| Range::StartLength { begin: Address::Constant(q.start), length: q.len }).collect(),
        );
        let ranges = unit.ranges.add(ranges);
        let root = unit.root();
        let cu = unit.get_mut(root);
        cu.set(constants::DW_AT_producer, AttributeValue::StringRef(producer));
        cu.set(constants::DW_AT_name, AttributeValue::String(file.as_bytes().to_vec()));
        cu.set(constants::DW_AT_comp_dir, AttributeValue::String(input.comp_dir.as_bytes().to_vec()));
        cu.set(constants::DW_AT_low_pc, AttributeValue::Address(Address::Constant(0)));
        cu.set(constants::DW_AT_ranges, AttributeValue::RangeListRef(ranges));
        for seq in seqs {
            let sp = unit.add(root, constants::DW_TAG_subprogram);
            let e = unit.get_mut(sp);
            e.set(constants::DW_AT_name, AttributeValue::String(seq.f.name.as_bytes().to_vec()));
            e.set(constants::DW_AT_low_pc, AttributeValue::Address(Address::Constant(seq.start)));
            e.set(constants::DW_AT_high_pc, AttributeValue::Udata(seq.len));
            if let Some(l) = seq.f.ops.iter().flatten().map(|l| l.line).min() {
                e.set(constants::DW_AT_decl_file, AttributeValue::FileIndex(Some(file_id)));
                e.set(constants::DW_AT_decl_line, AttributeValue::Udata(u64::from(l)));
            }
            stats.functions += 1;
        }
    }
    let mut sections = Sections::new(EndianVec::new(LittleEndian));
    dwarf.write(&mut sections).map_err(|e| format!("dwarf write: {e}"))?;
    let mut out = input.shipped.to_vec();
    sections
        .for_each(|id, data| {
            if !data.slice().is_empty() {
                let section = wasm_encoder::CustomSection { name: id.name().into(), data: data.slice().into() };
                out.push(wasm_encoder::Section::id(&section));
                wasm_encoder::Encode::encode(&section, &mut out);
            }
            Ok::<(), ()>(())
        })
        .ok();
    stats.bytes = out.len() - input.shipped.len();
    Ok((out, stats))
}
