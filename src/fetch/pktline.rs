//! git pkt-line framing: a 4-hex-digit length (including itself) and a
//! payload, or the special packets `0000` flush, `0001` delim, `0002`
//! response-end.

use std::io::Read;

/// The largest pkt-line git allows (65520 bytes including the length).
pub const MAX_PKT: usize = 65520;

#[derive(Debug, PartialEq, Eq)]
pub enum Pkt {
    Flush,
    Delim,
    ResponseEnd,
    Data(Vec<u8>),
}

pub struct PktReader<R: Read> {
    inner: R,
}

impl<R: Read> PktReader<R> {
    pub fn new(inner: R) -> Self {
        PktReader { inner }
    }

    /// The next packet; `Ok(None)` at a clean end of stream.
    pub fn next(&mut self) -> Result<Option<Pkt>, String> {
        let mut len = [0u8; 4];
        let mut got = 0;
        while got < 4 {
            match self.inner.read(&mut len[got..]) {
                Ok(0) if got == 0 => return Ok(None),
                Ok(0) => return Err("the remote closed the connection inside a packet".to_string()),
                Ok(n) => got += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(format!("reading from the remote failed: {e}")),
            }
        }
        let text = std::str::from_utf8(&len).map_err(|_| "the remote sent a malformed packet length".to_string())?;
        let n = usize::from_str_radix(text, 16).map_err(|_| format!("the remote sent a malformed packet length `{}`", text.escape_debug()))?;
        match n {
            0 => Ok(Some(Pkt::Flush)),
            1 => Ok(Some(Pkt::Delim)),
            2 => Ok(Some(Pkt::ResponseEnd)),
            3 => Err("the remote sent a packet of length 3".to_string()),
            n if n > MAX_PKT => Err(format!("the remote sent a packet of {n} bytes (max {MAX_PKT})")),
            n => {
                let mut buf = vec![0u8; n - 4];
                self.inner.read_exact(&mut buf).map_err(|e| format!("reading from the remote failed: {e}"))?;
                Ok(Some(Pkt::Data(buf)))
            }
        }
    }

    /// The next packet, which must exist.
    pub fn expect(&mut self) -> Result<Pkt, String> {
        self.next()?.ok_or_else(|| "the remote closed the connection early".to_string())
    }

    /// Data lines (trailing `\n` removed) up to the next flush.
    pub fn lines_to_flush(&mut self) -> Result<Vec<Vec<u8>>, String> {
        let mut out = Vec::new();
        loop {
            match self.expect()? {
                Pkt::Flush => return Ok(out),
                Pkt::Data(mut d) => {
                    if d.last() == Some(&b'\n') {
                        d.pop();
                    }
                    out.push(d);
                }
                Pkt::Delim | Pkt::ResponseEnd => return Err("the remote sent an unexpected delimiter".to_string()),
            }
        }
    }
}

/// Append one data packet holding `line` to `out`.
pub fn line(out: &mut Vec<u8>, line: &str) {
    out.extend_from_slice(format!("{:04x}", line.len() + 4).as_bytes());
    out.extend_from_slice(line.as_bytes());
}

pub fn flush(out: &mut Vec<u8>) {
    out.extend_from_slice(b"0000");
}

pub fn delim(out: &mut Vec<u8>) {
    out.extend_from_slice(b"0001");
}

/// Text the remote sent, safe to print: control and bidi characters escaped.
pub fn shown(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .flat_map(|c| {
            if c.is_control() || crate::git_source::is_bidi_control(c) {
                c.escape_unicode().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect::<String>()
        .trim()
        .chars()
        .take(400)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_round_trips() {
        let mut out = Vec::new();
        line(&mut out, "command=ls-refs\n");
        delim(&mut out);
        flush(&mut out);
        let mut r = PktReader::new(&out[..]);
        assert_eq!(r.expect().unwrap(), Pkt::Data(b"command=ls-refs\n".to_vec()));
        assert_eq!(r.expect().unwrap(), Pkt::Delim);
        assert_eq!(r.expect().unwrap(), Pkt::Flush);
        assert_eq!(r.next().unwrap(), None);
        assert!(PktReader::new(&b"zzzz"[..]).next().is_err());
        assert!(PktReader::new(&b"fff1"[..]).next().is_err());
        assert_eq!(shown(b"evil\x1b[2J\xe2\x80\xaeok"), "evil\\u{1b}[2J\\u{202e}ok");
    }
}
