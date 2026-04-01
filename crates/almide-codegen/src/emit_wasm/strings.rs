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

        // Write [len:i32 LE][data:u8...]
        self.data_bytes.extend_from_slice(&len.to_le_bytes());
        self.data_bytes.extend_from_slice(bytes);

        self.strings.insert(s.to_string(), offset);
        offset
    }

    /// Memory offset of the heap start (after all data).
    pub fn heap_start(&self) -> u32 {
        // Align to 8 bytes for clean allocation
        let raw = super::NEWLINE_OFFSET + self.data_bytes.len() as u32;
        (raw + 7) & !7
    }
}
