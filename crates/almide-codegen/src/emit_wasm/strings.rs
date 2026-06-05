//! String literal interning into the WASM data section.
//!
//! Strings are stored as [len:i32 LE][utf8 data...] in linear memory.
//! The returned offset points to the start of the [len] prefix.

use super::WasmEmitter;

impl WasmEmitter {
    /// Intern a string literal, returning its memory offset.
    /// Deduplicates: the same string always returns the same offset.
    pub fn intern_string(&mut self, s: &str) -> u32 {
        if let Some(&offset) = self.strings.get(s) {
            return offset;
        }

        // data_bytes is placed at memory[NEWLINE_OFFSET], so the next free offset is:
        let offset = super::NEWLINE_OFFSET + self.data_bytes.len() as u32;
        let bytes = s.as_bytes();
        let len = bytes.len() as u32;

        // Write [len:i32 LE][cap:i32 LE][data:u8...]
        self.data_bytes.extend_from_slice(&len.to_le_bytes());
        self.data_bytes.extend_from_slice(&len.to_le_bytes()); // cap = len for interned strings
        self.data_bytes.extend_from_slice(bytes);

        self.strings.insert(s.to_string(), offset);
        offset
    }

    /// Intern a raw byte blob into the data section, returning its memory offset.
    ///
    /// Used for static lookup tables (e.g. Unicode property range tables) whose
    /// bytes are NOT valid UTF-8 and so cannot go through `intern_string`. The
    /// blob is framed identically to an interned string — `[len:i32 LE][cap:i32
    /// LE][data:u8...]` — so the dead-data eliminator (`dce::eliminate_dead_data`)
    /// parses it as one entry and keeps it iff a live function references its
    /// offset via `i32.const`. No padding is inserted between entries (the DCE
    /// walk steps by `8 + len` and would desync otherwise); WASM permits the
    /// resulting unaligned loads. `key` must be unique and disjoint from any
    /// source string literal (callers prefix a NUL) so the two interning tables
    /// never collide on the offset map.
    pub fn intern_bytes(&mut self, key: &str, bytes: &[u8]) -> u32 {
        if let Some(&offset) = self.strings.get(key) {
            return offset;
        }
        let offset = super::NEWLINE_OFFSET + self.data_bytes.len() as u32;
        let len = bytes.len() as u32;
        self.data_bytes.extend_from_slice(&len.to_le_bytes());
        self.data_bytes.extend_from_slice(&len.to_le_bytes()); // cap = len
        self.data_bytes.extend_from_slice(bytes);
        self.strings.insert(key.to_string(), offset);
        offset
    }

    /// Memory offset of the heap start (after all data).
    pub fn heap_start(&self) -> u32 {
        // Align to 8 bytes for clean allocation
        let raw = super::NEWLINE_OFFSET + self.data_bytes.len() as u32;
        (raw + 7) & !7
    }
}
