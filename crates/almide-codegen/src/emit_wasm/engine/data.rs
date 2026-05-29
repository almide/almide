//! Data-segment interning for constant string literals.
//!
//! String literals are immutable, so they live in the module's passive/active
//! data segment rather than the heap. Each unique literal is laid out as a
//! complete String object matching the `STRING` layout
//! (`[len:i32][cap:i32][utf8 bytes]`) and the interner returns the absolute
//! linear-memory offset of that object's header.
//!
//! Because these objects sit *below* `heap_start`, the RC runtime skips them
//! (a literal has no alloc header and must never be freed). The module builder
//! computes `heap_start` from `DataInterner::end()` after lowering.

use std::collections::HashMap;

/// Accumulates interned string literals into a single data segment.
pub struct DataInterner {
    /// Absolute offset where the segment begins in linear memory.
    base: u32,
    /// Segment contents (the bytes written at `base`).
    bytes: Vec<u8>,
    /// Literal → absolute offset of its String header (dedup).
    cache: HashMap<String, u32>,
}

impl DataInterner {
    /// Create an interner whose segment starts at `base` (must be > 0 to keep
    /// the null pointer free, and 8-aligned for clean string headers).
    pub fn new(base: u32) -> Self {
        DataInterner { base, bytes: Vec::new(), cache: HashMap::new() }
    }

    /// Intern a string literal, returning the absolute offset of its header.
    /// The pointer addresses the `len` field; data begins 8 bytes later.
    pub fn intern(&mut self, s: &str) -> u32 {
        if let Some(&off) = self.cache.get(s) {
            return off;
        }
        // String headers are i32-aligned; align the cursor to 4 bytes.
        while self.bytes.len() % 4 != 0 {
            self.bytes.push(0);
        }
        let off = self.base + self.bytes.len() as u32;
        let len = s.len() as u32; // byte length
        self.bytes.extend_from_slice(&len.to_le_bytes()); // len @ 0
        self.bytes.extend_from_slice(&len.to_le_bytes()); // cap @ 4
        self.bytes.extend_from_slice(s.as_bytes()); // data @ 8
        self.cache.insert(s.to_string(), off);
        off
    }

    /// The interned bytes, to be emitted as a data segment at `base`.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The base offset of the segment.
    pub fn base(&self) -> u32 {
        self.base
    }

    /// First free offset after the segment (exclusive end).
    pub fn end(&self) -> u32 {
        self.base + self.bytes.len() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_dedups_and_lays_out_header() {
        let mut d = DataInterner::new(16);
        let a = d.intern("hello");
        let b = d.intern("hello"); // same literal → same offset
        assert_eq!(a, b);
        assert_eq!(a, 16);
        // len(5) little-endian at offset 0 of the object.
        assert_eq!(&d.bytes()[0..4], &5u32.to_le_bytes());
        assert_eq!(&d.bytes()[4..8], &5u32.to_le_bytes());
        assert_eq!(&d.bytes()[8..13], b"hello");

        // A second distinct literal starts after the first, 4-aligned.
        let c = d.intern("x");
        assert_eq!(c % 4, 0);
        assert!(c > a);
    }
}
