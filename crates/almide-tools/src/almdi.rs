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
use almide_ir::IrProgram;
use almide_base::intern::Sym;

const MAGIC: &[u8; 6] = b"ALMDI\0";
const FORMAT_VERSION: u16 = 1;

/// Read a `.almdi` file and return both the interface and IR.
pub fn read_almdi(path: &Path) -> Result<(ModuleInterface, IrProgram), String> {
    let data = std::fs::read(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
    let (_content_hash, iface_len, ir_len) = read_header(&data)?;

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
    let (_content_hash, iface_len, _ir_len) = read_header(&data)?;

    let iface_start = header_size();
    let iface_end = iface_start + iface_len as usize;

    if data.len() < iface_end {
        return Err("truncated .almdi file".to_string());
    }

    serde_json::from_slice(&data[iface_start..iface_end])
        .map_err(|e| format!("failed to parse interface section: {}", e))
}

/// The complete bytes of the `.almdi` artifact for `iface` + `ir`.
///
/// The header's hash field is a digest of the two sections, so it names the
/// artifact's content.
pub fn encode_almdi(iface: &ModuleInterface, ir: &IrProgram) -> Result<Vec<u8>, String> {
    let iface_json = serde_json::to_vec(iface)
        .map_err(|e| format!("failed to serialize interface: {}", e))?;
    let ir_json = serde_json::to_vec(ir)
        .map_err(|e| format!("failed to serialize IR: {}", e))?;
    let mut digest: u64 = 0xcbf2_9ce4_8422_2325;
    for b in iface_json.iter().chain(ir_json.iter()) {
        digest ^= *b as u64;
        digest = digest.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let mut bytes = Vec::with_capacity(header_size() + iface_json.len() + ir_json.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&digest.to_le_bytes());
    bytes.extend_from_slice(&(iface_json.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&(ir_json.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&iface_json);
    bytes.extend_from_slice(&ir_json);
    Ok(bytes)
}

/// Write the `.almdi` artifact for `iface` + `ir` to `path`, unless the file
/// there already holds exactly these bytes. Returns whether it wrote.
///
/// Freshness is the artifact's own content (#3096). It used to be a hash of
/// the module's source text alone, while the artifact also depends on the
/// modules it imports, the package version written into its interface, and
/// the compiler that lowered it — so bumping `version`, changing an imported
/// type, or upgrading almide printed "is up to date" over a stale file.
/// Comparing the encoded bytes covers every input by construction: whatever
/// shapes the artifact shapes those bytes.
pub fn write_almdi(path: &Path, iface: &ModuleInterface, ir: &IrProgram) -> Result<bool, String> {
    let bytes = encode_almdi(iface, ir)?;
    if std::fs::read(path).is_ok_and(|existing| existing == bytes) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
    }
    let mut file = std::fs::File::create(path)
        .map_err(|e| format!("failed to create {}: {}", path.display(), e))?;
    file.write_all(&bytes).map_err(|e| e.to_string())?;
    Ok(true)
}

// ── Header layout ──
// MAGIC (6) + FORMAT_VERSION (2) + CONTENT_HASH (8) + IFACE_LEN (8) + IR_LEN (8) = 32 bytes

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
    // The `data.len() < header_size()` guard above already proves every window
    // below is exactly 8 bytes, but say so with `?` rather than `unwrap` so a
    // future header-layout edit surfaces as an error instead of a panic on a
    // truncated file.
    let le_u64 = |lo: usize| -> Result<u64, String> {
        data.get(lo..lo + 8)
            .and_then(|w| <[u8; 8]>::try_from(w).ok())
            .map(u64::from_le_bytes)
            .ok_or_else(|| format!("truncated .almdi header at byte {}", lo))
    };
    Ok((le_u64(8)?, le_u64(16)?, le_u64(24)?))
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
                let qualified = almide_base::intern::sym(&format!("{}.{}", module.name, func.name));
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
