//! Decoding a module: the sections the structural emitter's shipped artifact
//! carries, in order, each checked against the closed feature set (REQ-VM-2).
//! A start section, a passive or declarative segment, an import this VM does
//! not serve, a second memory or table, a shared memory, an f32 global — each
//! is refused here, before any instance exists.

use crate::error::LoadError;
use crate::reader::Reader;
use crate::types::{FuncType, ValType};
use crate::validate::{self, Body, Context};

/// The host functions this VM knows: the five WASI preview-1 imports every
/// `to_wasi` artifact declares. A Critical-profile program, granted no Time
/// or Rand capability, calls only `fd_write`, `fd_read` and `proc_exit`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostFn {
    FdWrite,
    ProcExit,
    RandomGet,
    ClockTimeGet,
    FdRead,
}

impl HostFn {
    pub const MODULE: &'static str = "wasi_snapshot_preview1";

    fn by_name(name: &str) -> Option<HostFn> {
        Some(match name {
            "fd_write" => HostFn::FdWrite,
            "proc_exit" => HostFn::ProcExit,
            "random_get" => HostFn::RandomGet,
            "clock_time_get" => HostFn::ClockTimeGet,
            "fd_read" => HostFn::FdRead,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            HostFn::FdWrite => "fd_write",
            HostFn::ProcExit => "proc_exit",
            HostFn::RandomGet => "random_get",
            HostFn::ClockTimeGet => "clock_time_get",
            HostFn::FdRead => "fd_read",
        }
    }

    /// The exact WASI preview-1 signature.
    fn signature(self) -> FuncType {
        use ValType::{I32, I64};
        let (params, results): (&[ValType], &[ValType]) = match self {
            HostFn::FdWrite | HostFn::FdRead => (&[I32, I32, I32, I32], &[I32]),
            HostFn::ProcExit => (&[I32], &[]),
            HostFn::RandomGet => (&[I32, I32], &[I32]),
            HostFn::ClockTimeGet => (&[I32, I64, I32], &[I32]),
        };
        FuncType { params: params.to_vec(), results: results.to_vec() }
    }
}

pub struct Global {
    pub ty: ValType,
    pub mutable: bool,
    pub init: u64,
}

pub struct Segment<T> {
    pub offset: u32,
    pub items: T,
}

pub struct Module {
    pub types: Vec<FuncType>,
    /// The imported functions, in index order (they come first in the index space).
    pub imports: Vec<HostFn>,
    /// The type index of every function in the index space.
    pub func_types: Vec<u32>,
    /// The table's size (its declared minimum: nothing in the closed set
    /// grows it), when there is one.
    pub table: Option<u32>,
    /// Memory limits in 64 KiB pages.
    pub memory: Option<(u32, Option<u32>)>,
    pub globals: Vec<Global>,
    pub elements: Vec<Segment<Vec<u32>>>,
    pub data: Vec<Segment<Vec<u8>>>,
    /// Bodies of the defined functions (index = function index − imports).
    pub bodies: Vec<Body>,
    /// The function `_start` names.
    pub entry: u32,
}

pub const PAGE: usize = 65536;
/// A memory declaring more than this is refused (4 GiB is the 32-bit ceiling).
pub const MAX_PAGES: u32 = 65536;

type R<T> = Result<T, LoadError>;

#[derive(Default)]
struct Draft {
    types: Vec<FuncType>,
    imports: Vec<HostFn>,
    func_types: Vec<u32>,
    table: Option<u32>,
    memory: Option<(u32, Option<u32>)>,
    globals: Vec<Global>,
    entry: Option<u32>,
    elements: Vec<Segment<Vec<u32>>>,
    data: Vec<Segment<Vec<u8>>>,
    data_count: Option<u32>,
    bodies: Vec<Body>,
    code_seen: bool,
}

/// Decode and validate a whole module.
pub fn decode(bytes: &[u8]) -> R<Module> {
    let mut r = Reader::new(bytes);
    if r.bytes(4)? != b"\0asm" {
        return Err(LoadError::new(0, "not a wasm binary (bad magic)"));
    }
    if r.bytes(4)? != [1, 0, 0, 0] {
        return Err(LoadError::new(4, "not a version-1 wasm binary"));
    }
    let mut d = Draft::default();
    let mut last = 0u8;
    while !r.at_end() {
        let id = r.byte()?;
        let size = r.u32()? as usize;
        let mut s = r.sub(size)?;
        if id != 0 {
            let rank = section_rank(id).ok_or_else(|| LoadError::new(s.offset(), format!("section {id} is not accepted")))?;
            if rank <= last {
                return s.fail(format!("section {id} is out of order or repeated"));
            }
            last = rank;
        }
        d.section(id, &mut s)?;
        if !s.at_end() {
            return s.fail(format!("section {id} has bytes past its contents"));
        }
    }
    d.finish()
}

/// The order sections must appear in; `None` refuses the section outright
/// (the start section: the emitter never writes one).
fn section_rank(id: u8) -> Option<u8> {
    Some(match id {
        1 => 1,
        2 => 2,
        3 => 3,
        4 => 4,
        5 => 5,
        6 => 6,
        7 => 7,
        9 => 9,
        12 => 10,
        10 => 11,
        11 => 12,
        _ => return None,
    })
}

impl Draft {
    fn section(&mut self, id: u8, s: &mut Reader) -> R<()> {
        match id {
            0 => {
                // a custom section (names, producers) carries no semantics
                s.name()?;
                s.skip_rest();
                Ok(())
            }
            1 => self.type_section(s),
            2 => self.import_section(s),
            3 => self.function_section(s),
            4 => self.table_section(s),
            5 => self.memory_section(s),
            6 => self.global_section(s),
            7 => self.export_section(s),
            9 => self.element_section(s),
            12 => {
                self.data_count = Some(s.u32()?);
                Ok(())
            }
            10 => self.code_section(s),
            11 => self.data_section(s),
            _ => s.fail(format!("section {id} is not accepted")),
        }
    }

    fn type_section(&mut self, s: &mut Reader) -> R<()> {
        for _ in 0..s.vec_len()? {
            if s.byte()? != 0x60 {
                return s.fail("a type is not a function type");
            }
            let params = value_types(s)?;
            let results = value_types(s)?;
            if results.len() > 1 {
                return s.fail("a function type returns more than one value (multi-value is not accepted)");
            }
            self.types.push(FuncType { params, results });
        }
        Ok(())
    }

    fn import_section(&mut self, s: &mut Reader) -> R<()> {
        for _ in 0..s.vec_len()? {
            let module = s.name()?;
            let name = s.name()?;
            if s.byte()? != 0x00 {
                return s.fail(format!("import `{module}.{name}` is not a function"));
            }
            let ty = s.u32()?;
            let host = match HostFn::by_name(name) {
                Some(h) if module == HostFn::MODULE => h,
                _ => return s.fail(format!("import `{module}.{name}` is not served by this VM")),
            };
            if self.types.get(ty as usize) != Some(&host.signature()) {
                return s.fail(format!("import `{name}` does not have its WASI signature"));
            }
            self.imports.push(host);
            self.func_types.push(ty);
        }
        Ok(())
    }

    fn function_section(&mut self, s: &mut Reader) -> R<()> {
        for _ in 0..s.vec_len()? {
            let ty = s.u32()?;
            if ty as usize >= self.types.len() {
                return s.fail("a function names a type that does not exist");
            }
            self.func_types.push(ty);
        }
        Ok(())
    }

    fn table_section(&mut self, s: &mut Reader) -> R<()> {
        if s.vec_len()? != 1 || s.byte()? != 0x70 {
            return s.fail("only one funcref table is accepted");
        }
        // no instruction of the closed set grows or writes a table, so its
        // size is its minimum for the whole run, whatever maximum it declares
        let (min, max) = limits(s)?;
        if max.is_some_and(|m| m < min) {
            return s.fail("the table's maximum is below its minimum");
        }
        self.table = Some(min);
        Ok(())
    }

    fn memory_section(&mut self, s: &mut Reader) -> R<()> {
        if s.vec_len()? != 1 {
            return s.fail("exactly one memory is accepted");
        }
        let (min, max) = limits(s)?;
        if min > MAX_PAGES || max.is_some_and(|m| m > MAX_PAGES || m < min) {
            return s.fail("memory limits are out of range");
        }
        self.memory = Some((min, max));
        Ok(())
    }

    fn global_section(&mut self, s: &mut Reader) -> R<()> {
        for _ in 0..s.vec_len()? {
            let at = s.offset();
            let ty = ValType::declared(s.byte()?).ok_or_else(|| LoadError::new(at, "a global's type is outside i32/i64/f64"))?;
            let mutable = match s.byte()? {
                0 => false,
                1 => true,
                _ => return s.fail("a global's mutability flag is invalid"),
            };
            let init = const_expr(s, ty)?;
            self.globals.push(Global { ty, mutable, init });
        }
        Ok(())
    }

    fn export_section(&mut self, s: &mut Reader) -> R<()> {
        for _ in 0..s.vec_len()? {
            let name = s.name()?;
            let kind = s.byte()?;
            let index = s.u32()?;
            if kind > 3 {
                return s.fail("an export's kind is invalid");
            }
            if name == "_start" {
                if kind != 0 {
                    return s.fail("`_start` is not a function");
                }
                self.entry = Some(index);
            }
        }
        Ok(())
    }

    fn element_section(&mut self, s: &mut Reader) -> R<()> {
        for _ in 0..s.vec_len()? {
            // flag 0 (table 0 implied) or flag 2 naming table 0 and funcref
            // elements: the same active segment, spelled two ways
            let explicit = match s.u32()? {
                0 => false,
                2 => true,
                _ => return s.fail("only active function-index element segments on table 0 are accepted"),
            };
            if explicit && s.u32()? != 0 {
                return s.fail("an element segment names a table other than 0");
            }
            let offset = const_expr(s, ValType::I32)? as u32;
            if explicit && s.byte()? != 0x00 {
                return s.fail("an element segment's kind is not funcref");
            }
            let mut items = Vec::new();
            for _ in 0..s.vec_len()? {
                items.push(s.u32()?);
            }
            self.elements.push(Segment { offset, items });
        }
        Ok(())
    }

    fn code_section(&mut self, s: &mut Reader) -> R<()> {
        let count = s.vec_len()?;
        let defined = self.func_types.len() - self.imports.len();
        if count != defined {
            return s.fail("the code section's count differs from the function section's");
        }
        let globals: Vec<(ValType, bool)> = self.globals.iter().map(|g| (g.ty, g.mutable)).collect();
        let ctx = Context {
            types: &self.types,
            func_types: &self.func_types,
            globals: &globals,
            has_table: self.table.is_some(),
            has_memory: self.memory.is_some(),
        };
        for i in 0..count {
            let size = s.u32()? as usize;
            let mut body = s.sub(size)?;
            let ty = &self.types[self.func_types[self.imports.len() + i] as usize];
            self.bodies.push(validate::translate(&ctx, ty, &mut body)?);
        }
        self.code_seen = true;
        Ok(())
    }

    fn data_section(&mut self, s: &mut Reader) -> R<()> {
        let count = s.vec_len()?;
        if self.data_count.is_some_and(|c| c as usize != count) {
            return s.fail("the data section's count differs from the data count section");
        }
        for _ in 0..count {
            match s.u32()? {
                0 => {}
                2 if s.u32()? == 0 => {}
                _ => return s.fail("only active data segments on memory 0 are accepted"),
            }
            let offset = const_expr(s, ValType::I32)? as u32;
            let n = s.vec_len()?;
            let items = s.bytes(n)?.to_vec();
            self.data.push(Segment { offset, items });
        }
        Ok(())
    }

    /// Cross-section checks, then the module.
    fn finish(self) -> R<Module> {
        let fail = |why: &str| Err(LoadError::whole(why));
        if self.func_types.len() > self.imports.len() && !self.code_seen {
            return fail("functions are declared but the code section is missing");
        }
        let Some(entry) = self.entry else { return fail("the module exports no `_start`") };
        match self.func_types.get(entry as usize).map(|&t| &self.types[t as usize]) {
            Some(t) if t.params.is_empty() && t.results.is_empty() => {}
            _ => return fail("`_start` must be a function taking and returning nothing"),
        }
        let table = self.table.unwrap_or(0) as u64;
        for seg in &self.elements {
            if u64::from(seg.offset) + seg.items.len() as u64 > table {
                return fail("an element segment does not fit the table");
            }
            if seg.items.iter().any(|&f| f as usize >= self.func_types.len()) {
                return fail("an element segment names a function that does not exist");
            }
        }
        let memory = self.memory.map_or(0, |(min, _)| min as u64 * PAGE as u64);
        for seg in &self.data {
            if u64::from(seg.offset) + seg.items.len() as u64 > memory {
                return fail("a data segment does not fit the initial memory");
            }
        }
        Ok(Module {
            types: self.types,
            imports: self.imports,
            func_types: self.func_types,
            table: self.table,
            memory: self.memory,
            globals: self.globals,
            elements: self.elements,
            data: self.data,
            bodies: self.bodies,
            entry,
        })
    }
}

fn value_types(s: &mut Reader) -> R<Vec<ValType>> {
    let mut out = Vec::new();
    for _ in 0..s.vec_len()? {
        let at = s.offset();
        out.push(ValType::declared(s.byte()?).ok_or_else(|| LoadError::new(at, "a type is outside i32/i64/f64"))?);
    }
    Ok(out)
}

fn limits(s: &mut Reader) -> R<(u32, Option<u32>)> {
    match s.byte()? {
        0 => Ok((s.u32()?, None)),
        1 => Ok((s.u32()?, Some(s.u32()?))),
        _ => s.fail("shared, 64-bit and custom-page memories are not accepted"),
    }
}

/// A constant expression: one `*.const` of the expected type, then `end`.
fn const_expr(s: &mut Reader, ty: ValType) -> R<u64> {
    let value = match (s.byte()?, ty) {
        (0x41, ValType::I32) => u64::from(s.i32()? as u32),
        (0x42, ValType::I64) => s.i64()? as u64,
        (0x44, ValType::F64) => s.f64_bits()?,
        _ => return s.fail("a constant expression is not a single constant of the expected type"),
    };
    if s.byte()? != 0x0B {
        return s.fail("a constant expression does not end after its constant");
    }
    Ok(value)
}
