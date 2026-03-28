//! `.almdi` file format: Almide Module Interface artifacts.
//!
//! A `.almdi` file contains two sections:
//! 1. **Interface** — public API surface (types, functions, constants)
//! 2. **IR** — full type-checked intermediate representation
//!
//! External tools (binding generators, IDEs) read only the interface section.
//! `almide build` reads the IR section to skip re-parsing and re-checking.

use std::io::Write;
use std::path::Path;
use std::collections::HashSet;

use crate::interface::ModuleInterface;
use crate::ir::IrProgram;
use crate::intern::Sym;

const MAGIC: &[u8; 6] = b"ALMDI\0";
const FORMAT_VERSION: u16 = 1;

/// Read a `.almdi` file and return both the interface and IR.
pub fn read_almdi(path: &Path) -> Result<(ModuleInterface, IrProgram), String> {
    let data = std::fs::read(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
    let (source_hash, iface_len, ir_len) = read_header(&data)?;
    let _ = source_hash; // caller can check freshness separately

    let iface_start = header_size();
    let iface_end = iface_start + iface_len as usize;
    let ir_start = iface_end;
    let ir_end = ir_start + ir_len as usize;

    if data.len() < ir_end {
        return Err("truncated .almdi file".to_string());
    }

    let iface: ModuleInterface = serde_json::from_slice(&data[iface_start..iface_end])
        .map_err(|e| format!("failed to parse interface section: {}", e))?;
    let mut ir: IrProgram = serde_json::from_slice(&data[ir_start..ir_end])
        .map_err(|e| format!("failed to parse IR section: {}", e))?;

    rebuild_transient_fields(&mut ir);
    Ok((iface, ir))
}

/// Read only the interface section (skip IR deserialization).
pub fn read_interface_only(path: &Path) -> Result<ModuleInterface, String> {
    let data = std::fs::read(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
    let (_source_hash, iface_len, _ir_len) = read_header(&data)?;

    let iface_start = header_size();
    let iface_end = iface_start + iface_len as usize;

    if data.len() < iface_end {
        return Err("truncated .almdi file".to_string());
    }

    serde_json::from_slice(&data[iface_start..iface_end])
        .map_err(|e| format!("failed to parse interface section: {}", e))
}

/// Write a `.almdi` file.
pub fn write_almdi(
    path: &Path,
    iface: &ModuleInterface,
    ir: &IrProgram,
    source_hash: u64,
) -> Result<(), String> {
    let iface_json = serde_json::to_vec(iface)
        .map_err(|e| format!("failed to serialize interface: {}", e))?;
    let ir_json = serde_json::to_vec(ir)
        .map_err(|e| format!("failed to serialize IR: {}", e))?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
    }

    let mut file = std::fs::File::create(path)
        .map_err(|e| format!("failed to create {}: {}", path.display(), e))?;

    // Header
    file.write_all(MAGIC).map_err(|e| e.to_string())?;
    file.write_all(&FORMAT_VERSION.to_le_bytes()).map_err(|e| e.to_string())?;
    file.write_all(&source_hash.to_le_bytes()).map_err(|e| e.to_string())?;
    file.write_all(&(iface_json.len() as u64).to_le_bytes()).map_err(|e| e.to_string())?;
    file.write_all(&(ir_json.len() as u64).to_le_bytes()).map_err(|e| e.to_string())?;

    // Sections
    file.write_all(&iface_json).map_err(|e| e.to_string())?;
    file.write_all(&ir_json).map_err(|e| e.to_string())?;

    Ok(())
}

/// Check if a `.almdi` file exists and matches the given source hash.
pub fn is_fresh(path: &Path, source_hash: u64) -> bool {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(_) => return false,
    };
    match read_header(&data) {
        Ok((stored_hash, _, _)) => stored_hash == source_hash,
        Err(_) => false,
    }
}

/// Compute a source hash for staleness detection.
pub fn source_hash(source: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

// ── Header layout ──
// MAGIC (6) + FORMAT_VERSION (2) + SOURCE_HASH (8) + IFACE_LEN (8) + IR_LEN (8) = 32 bytes

fn header_size() -> usize { 6 + 2 + 8 + 8 + 8 }

fn read_header(data: &[u8]) -> Result<(u64, u64, u64), String> {
    if data.len() < header_size() {
        return Err("file too small to be a valid .almdi".to_string());
    }
    if &data[0..6] != MAGIC {
        return Err("not a valid .almdi file (bad magic)".to_string());
    }
    let version = u16::from_le_bytes([data[6], data[7]]);
    if version != FORMAT_VERSION {
        return Err(format!("unsupported .almdi format version {} (expected {})", version, FORMAT_VERSION));
    }
    let source_hash = u64::from_le_bytes(data[8..16].try_into().unwrap());
    let iface_len = u64::from_le_bytes(data[16..24].try_into().unwrap());
    let ir_len = u64::from_le_bytes(data[24..32].try_into().unwrap());
    Ok((source_hash, iface_len, ir_len))
}

/// Rebuild transient fields that are `#[serde(skip)]` on IrProgram.
/// These are populated during lowering but lost during serialization.
fn rebuild_transient_fields(ir: &mut IrProgram) {
    // Rebuild effect_fn_names from function declarations
    let mut effect_names: HashSet<Sym> = HashSet::new();
    for func in &ir.functions {
        if func.is_effect {
            effect_names.insert(func.name);
        }
    }
    for module in &ir.modules {
        for func in &module.functions {
            if func.is_effect {
                let qualified = crate::intern::sym(&format!("{}.{}", module.name, func.name));
                effect_names.insert(qualified);
            }
        }
    }
    ir.effect_fn_names = effect_names;

    // type_registry: rebuild from type_decls
    for td in &ir.type_decls {
        let name = td.name.to_string();
        let arity = td.generics.as_ref().map_or(0, |g| g.len());
        ir.type_registry.register_user_type(&name, arity);
    }

    // effect_map and codegen_annotations are populated by the nanopass pipeline
    // during codegen, so they don't need pre-population here.
}
