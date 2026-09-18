//! A bounds-checked cursor over the module bytes. Every read either returns a
//! value or a `LoadError` naming the offset; nothing indexes out of range, and
//! LEB128 decoding rejects over-long and out-of-range encodings as the binary
//! format requires.

use crate::error::LoadError;

pub struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
    /// Offset of `bytes[0]` in the whole module, so errors name absolute bytes.
    base: usize,
}

type R<T> = Result<T, LoadError>;

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Reader { bytes, pos: 0, base: 0 }
    }

    pub fn offset(&self) -> usize {
        self.base + self.pos
    }

    pub fn at_end(&self) -> bool {
        self.pos == self.bytes.len()
    }

    pub fn fail<T>(&self, reason: impl Into<String>) -> R<T> {
        Err(LoadError::new(self.offset(), reason))
    }

    pub fn byte(&mut self) -> R<u8> {
        match self.bytes.get(self.pos) {
            Some(&b) => {
                self.pos += 1;
                Ok(b)
            }
            None => self.fail("unexpected end of input"),
        }
    }

    pub fn bytes(&mut self, n: usize) -> R<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.bytes.len());
        match end {
            Some(end) => {
                let out = &self.bytes[self.pos..end];
                self.pos = end;
                Ok(out)
            }
            None => self.fail("a length runs past the end of its section"),
        }
    }

    /// A sub-reader over the next `n` bytes; the parent skips past them.
    pub fn sub(&mut self, n: usize) -> R<Reader<'a>> {
        let base = self.offset();
        let bytes = self.bytes(n)?;
        Ok(Reader { bytes, pos: 0, base })
    }

    /// Unsigned LEB128 of at most `bits` bits.
    fn uleb(&mut self, bits: u32) -> R<u64> {
        let max_bytes = bits.div_ceil(7);
        let mut result: u64 = 0;
        for i in 0..max_bytes {
            let b = self.byte()?;
            let shift = 7 * i;
            let payload = u64::from(b & 0x7f);
            if i == max_bytes - 1 {
                // the last byte may only carry the bits that fit
                let used = bits - shift;
                if used < 7 && payload >> used != 0 {
                    return self.fail("an integer encoding overflows its width");
                }
            }
            result |= payload << shift;
            if b & 0x80 == 0 {
                return Ok(result);
            }
        }
        self.fail("an integer encoding is longer than its width allows")
    }

    /// Signed LEB128 of at most `bits` bits, sign-extended to i64.
    fn sleb(&mut self, bits: u32) -> R<i64> {
        let max_bytes = bits.div_ceil(7);
        let mut result: i64 = 0;
        for i in 0..max_bytes {
            let b = self.byte()?;
            let shift = 7 * i;
            result |= i64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                let total = shift + 7;
                if i == max_bytes - 1 && total > bits {
                    // the unused high bits of the last byte must equal the sign
                    let unused = total - bits;
                    let sign_and_unused = (b as i8) << 1 >> (7 - unused);
                    if sign_and_unused != 0 && sign_and_unused != -1 {
                        return self.fail("a signed integer encoding overflows its width");
                    }
                }
                if total < 64 && b & 0x40 != 0 {
                    result |= -1i64 << total;
                }
                return Ok(result);
            }
        }
        self.fail("a signed integer encoding is longer than its width allows")
    }

    pub fn u32(&mut self) -> R<u32> {
        self.uleb(32).map(|v| v as u32)
    }

    pub fn i32(&mut self) -> R<i32> {
        self.sleb(32).map(|v| v as i32)
    }

    pub fn i64(&mut self) -> R<i64> {
        self.sleb(64)
    }

    pub fn f64_bits(&mut self) -> R<u64> {
        let b = self.bytes(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(u64::from_le_bytes(a))
    }

    /// A vector length, bounded by what the remaining bytes could hold (each
    /// element takes at least one byte), so a forged count cannot make the
    /// decoder reserve memory it never fills.
    pub fn vec_len(&mut self) -> R<usize> {
        let n = self.u32()? as usize;
        if n > self.bytes.len() - self.pos {
            return self.fail("a vector length exceeds the bytes that remain");
        }
        Ok(n)
    }

    /// Skip whatever remains (a custom section's payload).
    pub fn skip_rest(&mut self) {
        self.pos = self.bytes.len();
    }

    pub fn name(&mut self) -> R<&'a str> {
        let n = self.vec_len()?;
        let at = self.offset();
        let b = self.bytes(n)?;
        std::str::from_utf8(b).map_err(|_| LoadError::new(at, "a name is not valid UTF-8"))
    }
}

#[cfg(test)]
mod tests {
    use super::Reader;

    #[test]
    fn unsigned_leb_round_values_and_rejects_overlong() {
        assert_eq!(Reader::new(&[0x00]).u32().unwrap(), 0);
        assert_eq!(Reader::new(&[0xe5, 0x8e, 0x26]).u32().unwrap(), 624485);
        assert_eq!(Reader::new(&[0xff, 0xff, 0xff, 0xff, 0x0f]).u32().unwrap(), u32::MAX);
        assert!(Reader::new(&[0xff, 0xff, 0xff, 0xff, 0x1f]).u32().is_err(), "bits past 32");
        assert!(Reader::new(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x00]).u32().is_err(), "six bytes");
    }

    #[test]
    fn signed_leb_sign_extends_and_rejects_overflow() {
        assert_eq!(Reader::new(&[0x7f]).i32().unwrap(), -1);
        assert_eq!(Reader::new(&[0xc0, 0xbb, 0x78]).i32().unwrap(), -123456);
        assert_eq!(Reader::new(&[0xff, 0xff, 0xff, 0xff, 0x07]).i32().unwrap(), i32::MAX);
        assert_eq!(Reader::new(&[0x80, 0x80, 0x80, 0x80, 0x78]).i32().unwrap(), i32::MIN);
        assert!(Reader::new(&[0xff, 0xff, 0xff, 0xff, 0x4f]).i32().is_err(), "unused bits disagree with the sign");
        assert_eq!(
            Reader::new(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x7f]).i64().unwrap(),
            i64::MIN
        );
    }

    #[test]
    fn a_length_past_the_end_is_refused() {
        assert!(Reader::new(&[0x05, 0x01]).vec_len().is_err());
        assert!(Reader::new(&[0x02, 0x61]).name().is_err());
    }
}
