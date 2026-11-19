//! A git packfile, read into objects whose ids are recomputed from their
//! content: the commit a lock names is bound to every byte of its tree by
//! hashing, not by trusting the remote's word.

use std::collections::HashMap;

/// The repository's hash function (protocol `object-format`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectFormat {
    Sha1,
    Sha256,
}

impl ObjectFormat {
    pub fn len(self) -> usize {
        match self {
            ObjectFormat::Sha1 => 20,
            ObjectFormat::Sha256 => 32,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ObjectFormat::Sha1 => "sha1",
            ObjectFormat::Sha256 => "sha256",
        }
    }

    fn hash(self, parts: &[&[u8]]) -> Vec<u8> {
        match self {
            ObjectFormat::Sha1 => {
                use sha1::Digest;
                let mut h = sha1::Sha1::new();
                for p in parts {
                    h.update(p);
                }
                h.finalize().to_vec()
            }
            ObjectFormat::Sha256 => {
                use sha2::Digest;
                let mut h = sha2::Sha256::new();
                for p in parts {
                    h.update(p);
                }
                h.finalize().to_vec()
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Commit,
    Tree,
    Blob,
    Tag,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Commit => "commit",
            Kind::Tree => "tree",
            Kind::Blob => "blob",
            Kind::Tag => "tag",
        }
    }
}

/// Every object of a pack, by id.
pub struct Objects {
    pub format: ObjectFormat,
    by_id: HashMap<Vec<u8>, (Kind, Vec<u8>)>,
}

impl Objects {
    pub fn get(&self, id: &[u8]) -> Option<&(Kind, Vec<u8>)> {
        self.by_id.get(id)
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }
}

/// Inflated bytes one object may have: a blob is a package file, and a tree
/// or commit is far smaller than one.
const MAX_OBJECT: usize = crate::package_tree::MAX_FILE_BYTES as usize;
/// Inflated bytes of every object of one pack together.
const MAX_INFLATED: usize = 2 * crate::package_tree::MAX_PACKAGE_BYTES as usize;
/// Objects one pack may hold: the package's files plus its trees and commit.
const MAX_OBJECTS: usize = 4 * crate::package_tree::MAX_FILES;
/// Delta chain length.
const MAX_CHAIN: usize = 1000;

enum Raw {
    Full(Kind, Vec<u8>),
    OfsDelta(usize, Vec<u8>),
    RefDelta(Vec<u8>, Vec<u8>),
}

fn err<T>(what: impl Into<String>) -> Result<T, String> {
    Err(format!("the pack the remote sent is malformed: {}", what.into()))
}

/// Inflate one zlib stream of `size` bytes starting at `data[at..]`; returns
/// the bytes and how much input it took.
fn inflate(data: &[u8], at: usize, size: usize) -> Result<(Vec<u8>, usize), String> {
    use miniz_oxide::inflate::TINFLStatus;
    use miniz_oxide::inflate::core::{DecompressorOxide, decompress, inflate_flags};
    if size > MAX_OBJECT {
        return err(format!("an object of {size} bytes (max {MAX_OBJECT})"));
    }
    let mut out = vec![0u8; size];
    let mut d = DecompressorOxide::new();
    let flags = inflate_flags::TINFL_FLAG_PARSE_ZLIB_HEADER | inflate_flags::TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF;
    let (status, used, written) = decompress(&mut d, &data[at..], &mut out, 0, flags);
    match status {
        TINFLStatus::Done if written == size => Ok((out, used)),
        TINFLStatus::Done => err(format!("an object inflated to {written} bytes, not the {size} its header says")),
        TINFLStatus::HasMoreOutput => err(format!("an object inflated past the {size} bytes its header says")),
        s => err(format!("a zlib stream failed ({s:?})")),
    }
}

fn varint_size(data: &[u8], mut at: usize, mut value: usize, mut shift: u32, mut byte: u8) -> Result<(usize, usize), String> {
    while byte & 0x80 != 0 {
        byte = *data.get(at).ok_or_else(|| "truncated object header".to_string())?;
        at += 1;
        if shift > 56 {
            return err("an object size that overflows");
        }
        value |= ((byte & 0x7f) as usize) << shift;
        shift += 7;
    }
    Ok((value, at))
}

/// Apply a git delta to `base`.
fn apply_delta(base: &[u8], delta: &[u8]) -> Result<Vec<u8>, String> {
    let read_size = |at: &mut usize| -> Result<usize, String> {
        let (mut v, mut shift) = (0usize, 0u32);
        loop {
            let b = *delta.get(*at).ok_or_else(|| "truncated delta".to_string())?;
            *at += 1;
            if shift > 56 {
                return err("a delta size that overflows");
            }
            v |= ((b & 0x7f) as usize) << shift;
            shift += 7;
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
    };
    let mut at = 0;
    let src = read_size(&mut at)?;
    let dst = read_size(&mut at)?;
    if src != base.len() {
        return err("a delta whose base has another size");
    }
    if dst > MAX_OBJECT {
        return err(format!("a delta result of {dst} bytes (max {MAX_OBJECT})"));
    }
    let mut out = Vec::with_capacity(dst);
    while at < delta.len() {
        let op = delta[at];
        at += 1;
        if op & 0x80 != 0 {
            let (mut off, mut len) = (0usize, 0usize);
            for i in 0..4 {
                if op & (1 << i) != 0 {
                    off |= (*delta.get(at).ok_or_else(|| "truncated delta copy".to_string())? as usize) << (8 * i);
                    at += 1;
                }
            }
            for i in 0..3 {
                if op & (0x10 << i) != 0 {
                    len |= (*delta.get(at).ok_or_else(|| "truncated delta copy".to_string())? as usize) << (8 * i);
                    at += 1;
                }
            }
            if len == 0 {
                len = 0x10000;
            }
            let end = off.checked_add(len).filter(|e| *e <= base.len()).ok_or_else(|| "a delta copy outside its base".to_string())?;
            out.extend_from_slice(&base[off..end]);
        } else if op != 0 {
            let n = op as usize;
            let bytes = delta.get(at..at + n).ok_or_else(|| "truncated delta insert".to_string())?;
            out.extend_from_slice(bytes);
            at += n;
        } else {
            return err("a delta opcode 0");
        }
        if out.len() > dst {
            return err("a delta that writes past its result size");
        }
    }
    if out.len() != dst {
        return err("a delta whose result has another size");
    }
    Ok(out)
}

/// Read a whole pack. Every object's id is computed from its content.
pub fn read(data: &[u8], format: ObjectFormat) -> Result<Objects, String> {
    if data.len() < 12 + format.len() || &data[..4] != b"PACK" {
        return err("no PACK header");
    }
    let version = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    if version != 2 && version != 3 {
        return err(format!("pack version {version}"));
    }
    let count = u32::from_be_bytes([data[8], data[9], data[10], data[11]]) as usize;
    if count > MAX_OBJECTS {
        return err(format!("{count} objects (max {MAX_OBJECTS})"));
    }
    let body_end = data.len() - format.len();
    if format.hash(&[&data[..body_end]]) != data[body_end..] {
        return err("its checksum does not match");
    }
    let mut at = 12;
    let mut raws: Vec<(usize, Raw)> = Vec::with_capacity(count);
    let mut inflated = 0usize;
    for _ in 0..count {
        let start = at;
        let first = *data.get(at).ok_or_else(|| "truncated".to_string())?;
        at += 1;
        let ty = (first >> 4) & 7;
        let (size, next) = varint_size(data, at, (first & 0x0f) as usize, 4, first)?;
        at = next;
        let raw = match ty {
            1..=4 => {
                let kind = [Kind::Commit, Kind::Tree, Kind::Blob, Kind::Tag][ty as usize - 1];
                let (bytes, used) = inflate(&data[..body_end], at, size)?;
                at += used;
                Raw::Full(kind, bytes)
            }
            6 => {
                let mut b = *data.get(at).ok_or_else(|| "truncated".to_string())?;
                at += 1;
                let mut off = (b & 0x7f) as usize;
                while b & 0x80 != 0 {
                    b = *data.get(at).ok_or_else(|| "truncated".to_string())?;
                    at += 1;
                    off = off.checked_add(1).and_then(|o| o.checked_mul(128)).ok_or_else(|| "an offset that overflows".to_string())?
                        | (b & 0x7f) as usize;
                }
                let base = start.checked_sub(off).filter(|_| off > 0).ok_or_else(|| "a delta base before the pack".to_string())?;
                let (bytes, used) = inflate(&data[..body_end], at, size)?;
                at += used;
                Raw::OfsDelta(base, bytes)
            }
            7 => {
                let id = data.get(at..at + format.len()).ok_or_else(|| "truncated".to_string())?.to_vec();
                at += format.len();
                let (bytes, used) = inflate(&data[..body_end], at, size)?;
                at += used;
                Raw::RefDelta(id, bytes)
            }
            t => return err(format!("object type {t}")),
        };
        inflated += size;
        if inflated > MAX_INFLATED {
            return err(format!("more than {MAX_INFLATED} inflated bytes"));
        }
        raws.push((start, raw));
    }
    if at != body_end {
        return err("bytes after the last object");
    }
    // Resolve deltas: repeat until nothing changes; a chain is bounded by
    // the passes, and a cycle leaves objects unresolved.
    let index_of: HashMap<usize, usize> = raws.iter().enumerate().map(|(i, (s, _))| (*s, i)).collect();
    let mut done: Vec<Option<(Kind, Vec<u8>, Vec<u8>)>> = vec![None; raws.len()];
    let mut by_id_idx: HashMap<Vec<u8>, usize> = HashMap::new();
    let id_of = |kind: Kind, bytes: &[u8]| format.hash(&[format!("{} {}\0", kind.name(), bytes.len()).as_bytes(), bytes]);
    for (i, (_, raw)) in raws.iter().enumerate() {
        if let Raw::Full(kind, bytes) = raw {
            let id = id_of(*kind, bytes);
            by_id_idx.insert(id.clone(), i);
            done[i] = Some((*kind, bytes.clone(), id));
        }
    }
    for _ in 0..MAX_CHAIN {
        let mut progress = false;
        for i in 0..raws.len() {
            if done[i].is_some() {
                continue;
            }
            let base = match &raws[i].1 {
                Raw::OfsDelta(off, _) => index_of.get(off).copied(),
                Raw::RefDelta(id, _) => by_id_idx.get(id).copied(),
                Raw::Full(..) => None,
            };
            let Some(b) = base else {
                if let Raw::OfsDelta(..) = raws[i].1 {
                    return err("a delta base offset that is no object");
                }
                continue;
            };
            let Some((kind, base_bytes, _)) = &done[b] else { continue };
            let delta = match &raws[i].1 {
                Raw::OfsDelta(_, d) | Raw::RefDelta(_, d) => d,
                Raw::Full(..) => unreachable!(),
            };
            let bytes = apply_delta(base_bytes, delta)?;
            inflated += bytes.len();
            if inflated > MAX_INFLATED {
                return err(format!("more than {MAX_INFLATED} inflated bytes"));
            }
            let kind = *kind;
            let id = id_of(kind, &bytes);
            by_id_idx.insert(id.clone(), i);
            done[i] = Some((kind, bytes, id));
            progress = true;
        }
        if !progress {
            break;
        }
    }
    let mut by_id = HashMap::new();
    for d in done {
        let Some((kind, bytes, id)) = d else { return err("a delta whose base is not in the pack") };
        by_id.insert(id, (kind, bytes));
    }
    Ok(Objects { format, by_id })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deltas_apply_and_bad_ones_are_refused() {
        // src 5, dst 7: copy base[0..5], insert "!!".
        let delta = [5u8, 7, 0x80 | 0x01 | 0x10, 0, 5, 2, b'!', b'!'];
        assert_eq!(apply_delta(b"hello", &delta).unwrap(), b"hello!!");
        assert!(apply_delta(b"hell", &delta).is_err());
        let outside = [5u8, 5, 0x80 | 0x01 | 0x10, 3, 5];
        assert!(apply_delta(b"hello", &outside).is_err());
        assert!(read(b"PACK", ObjectFormat::Sha1).is_err());
    }
}
