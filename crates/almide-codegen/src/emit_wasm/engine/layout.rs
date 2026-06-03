//! LayoutRegistry — single source of truth for all WASM heap layouts.
//!
//! Every collection type (String, List, Map, Set) and composite type (Record,
//! Variant, Tuple) has its memory layout defined here. No hardcoded offsets
//! exist outside this module.
//!
//! Layout changes are made ONCE here and propagate to all emission sites.

/// Identifies a registered layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayoutId(pub u16);

/// Identifies a field within a layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldId(pub u16);

/// Memory type of a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemType {
    I32,
    I64,
    F32,
    F64,
    U8,
}

impl MemType {
    /// WASM alignment exponent (log2 of natural alignment in bytes).
    /// I32/F32 → 2 (4 bytes), I64/F64 → 3 (8 bytes), U8 → 0 (1 byte).
    pub const fn align_exp(self) -> u32 {
        match self {
            Self::I32 | Self::F32 => 2,
            Self::I64 | Self::F64 => 3,
            Self::U8 => 0,
        }
    }

    /// Size in bytes.
    pub const fn byte_size(self) -> u32 {
        match self {
            Self::I32 | Self::F32 => 4,
            Self::I64 | Self::F64 => 8,
            Self::U8 => 1,
        }
    }
}

/// How to compute a field's offset from the base pointer.
#[derive(Debug, Clone)]
pub enum FieldOffset {
    /// Fixed byte offset from base.
    Fixed(u32),
    /// Offset = fixed_base + runtime value of another field.
    /// Used for Swiss Table entries: offset = TAGS_OFFSET + cap.
    AfterDynamic { base: u32, size_field: FieldId },
}

/// A single field in a memory layout.
#[derive(Debug, Clone)]
pub struct MemField {
    pub name: &'static str,
    pub offset: FieldOffset,
    pub ty: MemType,
    /// For array fields: element stride in bytes. 0 for scalars.
    pub elem_stride: u32,
}

/// A complete memory layout for a heap object.
#[derive(Debug, Clone)]
pub struct MemLayout {
    pub name: &'static str,
    pub fields: Vec<MemField>,
    pub header_size: u32,
}

/// Central registry of all layouts. Constructed once at WASM emission start.
pub struct LayoutRegistry {
    layouts: Vec<MemLayout>,
}

// Well-known layout IDs — compile-time constants.
pub const STRING: LayoutId = LayoutId(0);
pub const LIST: LayoutId = LayoutId(1);
pub const SWISS_MAP: LayoutId = LayoutId(2);
pub const SET: LayoutId = LayoutId(3);
pub const ALLOC_HEADER: LayoutId = LayoutId(4);
pub const CLOSURE_PAIR: LayoutId = LayoutId(5);
pub const VARIANT: LayoutId = LayoutId(6);
pub const OPTION: LayoutId = LayoutId(7);
pub const RESULT: LayoutId = LayoutId(8);

// Well-known field IDs for String.
pub mod string {
    use super::FieldId;
    pub const LEN: FieldId = FieldId(0);
    pub const CAP: FieldId = FieldId(1);
    pub const DATA: FieldId = FieldId(2);
}

// Well-known field IDs for List.
pub mod list {
    use super::FieldId;
    pub const LEN: FieldId = FieldId(0);
    pub const CAP: FieldId = FieldId(1);
    pub const DATA: FieldId = FieldId(2);
}

// Well-known field IDs + compact-ordered-dict tuning for the Map.
//
// The Map is a compact-ordered-dict (Python-dict / indexmap style): a hash INDEX
// over a DENSE insertion-ordered entries array. Heap layout:
//   [len:i32 @0][cap:i32 @4][tags:u8[cap] @8][index:i32[cap] @8+cap][entries:(K,V)[cap] @8+cap+cap*INDEX_SLOT_SIZE]
// `LEN`/`CAP` are fixed-offset; the three array regions are cap-relative and are
// addressed through the centralized `dict_*` helpers (never inline byte math) so
// there is no layout magic number at any call site.
pub mod map {
    use super::FieldId;
    pub const LEN: FieldId = FieldId(0);
    pub const CAP: FieldId = FieldId(1);
    /// h2 fast-reject tag array, `cap` bytes @ header (0 = empty slot).
    pub const TAGS: FieldId = FieldId(2);
    /// Dense `(key,val)` entries in INSERTION ORDER, `entries[0..len]`. (Field id
    /// is nominal — its actual base is `8 + cap + cap*INDEX_SLOT_SIZE`, computed by
    /// the `dict_*` helpers, not by `fixed_offset`.)
    pub const ENTRIES: FieldId = FieldId(3);

    /// Bytes per INDEX slot (a 1-based `i32` pointer into the dense entries).
    pub const INDEX_SLOT_SIZE: u32 = 4;
    /// INDEX slot value meaning "empty" (entries are 1-based: slot `v` → `entries[v-1]`).
    pub const EMPTY_INDEX: i32 = 0;
    /// Initial capacity (always a power of two).
    pub const INITIAL_CAP: u32 = 16;
    /// Growth on overflow: `new_cap = cap << GROWTH_SHIFT` (4×).
    pub const GROWTH_SHIFT: u32 = 2;
    /// Load factor: grow when `len * LOAD_NUM > cap * LOAD_DEN` (75%).
    pub const LOAD_NUM: u32 = 3;
    pub const LOAD_DEN: u32 = 4;
    /// Hash split: `h2` tag = `(hash >> H2_SHIFT) & H2_MASK`, coerced to ≥ 1.
    pub const H2_SHIFT: u32 = 25;
    pub const H2_MASK: u32 = 0x7F;
}

// Well-known field IDs for alloc header.
pub mod alloc {
    use super::FieldId;
    /// Block size (at ptr - 8).
    pub const SIZE: FieldId = FieldId(0);
    /// Reference count (at ptr - 4).
    pub const RC: FieldId = FieldId(1);
}

// Well-known field IDs for ClosurePair: [table_idx:i32 @ 0][env_ptr:i32 @ 4]
pub mod closure {
    use super::FieldId;
    pub const TABLE_IDX: FieldId = FieldId(0);
    pub const ENV_PTR: FieldId = FieldId(1);
}

// Well-known field IDs for tagged unions (Variant/Option/Result).
// All share: [tag:i32 @ 0][payload @ 4...]
pub mod tagged {
    use super::FieldId;
    pub const TAG: FieldId = FieldId(0);
    pub const PAYLOAD: FieldId = FieldId(1);
}

impl LayoutRegistry {
    /// Create the registry with all built-in layouts.
    pub fn new() -> Self {
        let mut layouts = Vec::new();

        // ── String: [len:i32 @ 0][cap:i32 @ 4][data:u8... @ 8] ──
        layouts.push(MemLayout {
            name: "String",
            header_size: 8,
            fields: vec![
                MemField { name: "len", offset: FieldOffset::Fixed(0), ty: MemType::I32, elem_stride: 0 },
                MemField { name: "cap", offset: FieldOffset::Fixed(4), ty: MemType::I32, elem_stride: 0 },
                MemField { name: "data", offset: FieldOffset::Fixed(8), ty: MemType::U8, elem_stride: 1 },
            ],
        });

        // ── List: [len:i32 @ 0][cap:i32 @ 4][data:T... @ 8] ──
        layouts.push(MemLayout {
            name: "List",
            header_size: 8,
            fields: vec![
                MemField { name: "len", offset: FieldOffset::Fixed(0), ty: MemType::I32, elem_stride: 0 },
                MemField { name: "cap", offset: FieldOffset::Fixed(4), ty: MemType::I32, elem_stride: 0 },
                MemField { name: "data", offset: FieldOffset::Fixed(8), ty: MemType::I32, elem_stride: 0 }, // stride set per-use
            ],
        });

        // ── SwissMap: [len:i32 @ 0][cap:i32 @ 4][tags:u8[cap] @ 8][entries:(K,V)[cap] @ 8+cap] ──
        layouts.push(MemLayout {
            name: "SwissMap",
            header_size: 8,
            fields: vec![
                MemField { name: "len", offset: FieldOffset::Fixed(0), ty: MemType::I32, elem_stride: 0 },
                MemField { name: "cap", offset: FieldOffset::Fixed(4), ty: MemType::I32, elem_stride: 0 },
                MemField { name: "tags", offset: FieldOffset::Fixed(8), ty: MemType::U8, elem_stride: 1 },
                MemField {
                    name: "entries",
                    offset: FieldOffset::AfterDynamic {
                        base: 8, // TAGS start
                        size_field: map::CAP, // entries start at 8 + cap
                    },
                    ty: MemType::I32,
                    elem_stride: 0, // set per-use (key_size + val_size)
                },
            ],
        });

        // ── Set: same as List ──
        layouts.push(MemLayout {
            name: "Set",
            header_size: 8,
            fields: vec![
                MemField { name: "len", offset: FieldOffset::Fixed(0), ty: MemType::I32, elem_stride: 0 },
                MemField { name: "cap", offset: FieldOffset::Fixed(4), ty: MemType::I32, elem_stride: 0 },
                MemField { name: "data", offset: FieldOffset::Fixed(8), ty: MemType::I32, elem_stride: 0 },
            ],
        });

        // ── Alloc header: [size:i32 @ -8][rc:i32 @ -4][data @ 0] ──
        // Offsets are negative from the data pointer.
        layouts.push(MemLayout {
            name: "AllocHeader",
            header_size: 8,
            fields: vec![
                MemField { name: "size", offset: FieldOffset::Fixed(0), ty: MemType::I32, elem_stride: 0 },
                MemField { name: "rc", offset: FieldOffset::Fixed(4), ty: MemType::I32, elem_stride: 0 },
            ],
        });

        // ── ClosurePair: [table_idx:i32 @ 0][env_ptr:i32 @ 4] ──
        layouts.push(MemLayout {
            name: "ClosurePair",
            header_size: 0,
            fields: vec![
                MemField { name: "table_idx", offset: FieldOffset::Fixed(0), ty: MemType::I32, elem_stride: 0 },
                MemField { name: "env_ptr", offset: FieldOffset::Fixed(4), ty: MemType::I32, elem_stride: 0 },
            ],
        });

        // ── Variant: [tag:i32 @ 0][payload... @ 4] ──
        layouts.push(MemLayout {
            name: "Variant",
            header_size: 4,
            fields: vec![
                MemField { name: "tag", offset: FieldOffset::Fixed(0), ty: MemType::I32, elem_stride: 0 },
                MemField { name: "payload", offset: FieldOffset::Fixed(4), ty: MemType::I32, elem_stride: 0 },
            ],
        });

        // ── Option: [tag:i32 @ 0][value @ 4] ── (tag 0=None, 1=Some)
        layouts.push(MemLayout {
            name: "Option",
            header_size: 4,
            fields: vec![
                MemField { name: "tag", offset: FieldOffset::Fixed(0), ty: MemType::I32, elem_stride: 0 },
                MemField { name: "payload", offset: FieldOffset::Fixed(4), ty: MemType::I32, elem_stride: 0 },
            ],
        });

        // ── Result: [tag:i32 @ 0][value @ 4] ── (tag 0=Ok, 1=Err)
        layouts.push(MemLayout {
            name: "Result",
            header_size: 4,
            fields: vec![
                MemField { name: "tag", offset: FieldOffset::Fixed(0), ty: MemType::I32, elem_stride: 0 },
                MemField { name: "payload", offset: FieldOffset::Fixed(4), ty: MemType::I32, elem_stride: 0 },
            ],
        });

        Self { layouts }
    }

    /// Get a layout by ID.
    pub fn get(&self, id: LayoutId) -> &MemLayout {
        &self.layouts[id.0 as usize]
    }

    /// Resolve a fixed field offset. Panics if the field is dynamic.
    pub fn fixed_offset(&self, layout: LayoutId, field: FieldId) -> u32 {
        let f = &self.layouts[layout.0 as usize].fields[field.0 as usize];
        match &f.offset {
            FieldOffset::Fixed(n) => *n,
            FieldOffset::AfterDynamic { .. } => {
                panic!("field `{}::{}` has dynamic offset — use emit_dynamic_offset()",
                    self.layouts[layout.0 as usize].name, f.name)
            }
        }
    }

    /// Get the header size for a layout.
    pub fn header_size(&self, layout: LayoutId) -> u32 {
        self.layouts[layout.0 as usize].header_size
    }

    /// Get a field definition.
    pub fn field(&self, layout: LayoutId, field: FieldId) -> &MemField {
        &self.layouts[layout.0 as usize].fields[field.0 as usize]
    }

    /// Negative offset from data pointer to an alloc header field.
    ///
    /// Alloc header sits *before* the data pointer:
    /// `[size:4][rc:4][data...]`
    ///                 ^ ptr
    ///
    /// `ptr - neg_offset(ALLOC_HEADER, alloc::RC)` → address of RC field.
    pub fn alloc_header_neg_offset(&self, field: FieldId) -> u32 {
        let hdr = self.header_size(ALLOC_HEADER);
        let field_off = self.fixed_offset(ALLOC_HEADER, field);
        hdr - field_off
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_offsets() {
        let reg = LayoutRegistry::new();
        assert_eq!(reg.fixed_offset(STRING, string::LEN), 0);
        assert_eq!(reg.fixed_offset(STRING, string::CAP), 4);
        assert_eq!(reg.fixed_offset(STRING, string::DATA), 8);
        assert_eq!(reg.header_size(STRING), 8);
    }

    #[test]
    fn list_offsets() {
        let reg = LayoutRegistry::new();
        assert_eq!(reg.fixed_offset(LIST, list::LEN), 0);
        assert_eq!(reg.fixed_offset(LIST, list::CAP), 4);
        assert_eq!(reg.fixed_offset(LIST, list::DATA), 8);
    }

    #[test]
    fn map_fixed_offsets() {
        let reg = LayoutRegistry::new();
        assert_eq!(reg.fixed_offset(SWISS_MAP, map::LEN), 0);
        assert_eq!(reg.fixed_offset(SWISS_MAP, map::CAP), 4);
        assert_eq!(reg.fixed_offset(SWISS_MAP, map::TAGS), 8);
    }

    #[test]
    #[should_panic(expected = "dynamic offset")]
    fn map_entries_is_dynamic() {
        let reg = LayoutRegistry::new();
        reg.fixed_offset(SWISS_MAP, map::ENTRIES); // should panic
    }
}
