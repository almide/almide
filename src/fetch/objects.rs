//! From a verified pack to a [`PackageTree`]: the commit's tree, walked
//! entry by entry. A git tree mode IS the entry kind, so a symlink or a
//! submodule is refused here by type, before anything is written.

use super::pack::{Kind, Objects};
use crate::package_tree::{EntryKind, PackageFormatError, PackageTree};

fn hex_to_bytes(s: &str) -> Option<Vec<u8>> {
    (0..s.len()).step_by(2).map(|i| s.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok())).collect()
}

/// Why a fetched commit could not become a package tree.
#[derive(Debug)]
pub enum TreeError {
    /// The remote's data does not hold together (missing object, bad tree).
    Remote(String),
    /// The content is not a valid package (D3).
    Format(PackageFormatError),
}

/// The package tree of `commit` (lowercase hex), from `objs`.
pub fn tree_of_commit(objs: &Objects, commit: &str) -> Result<PackageTree, TreeError> {
    let remote = |s: String| TreeError::Remote(s);
    let id = hex_to_bytes(commit).ok_or_else(|| remote(format!("`{commit}` is not hex")))?;
    let Some((Kind::Commit, body)) = objs.get(&id) else {
        return Err(remote(format!("the remote did not send commit {commit}")));
    };
    let first = body.split(|b| *b == b'\n').next().unwrap_or_default();
    let tree_hex = first
        .strip_prefix(b"tree ")
        .and_then(|h| std::str::from_utf8(h).ok())
        .filter(|h| h.len() == 2 * objs.format.len())
        .ok_or_else(|| remote(format!("commit {commit} names no tree")))?;
    let tree_id = hex_to_bytes(tree_hex).ok_or_else(|| remote(format!("commit {commit} names no tree")))?;
    let mut out = PackageTree::new();
    walk(objs, &tree_id, &mut Vec::new(), &mut out)?;
    Ok(out)
}

fn walk(objs: &Objects, id: &[u8], at: &mut Vec<Vec<u8>>, out: &mut PackageTree) -> Result<(), TreeError> {
    let Some((Kind::Tree, body)) = objs.get(id) else {
        return Err(TreeError::Remote(format!("the remote did not send tree {}", crate::package_tree::hex(id))));
    };
    let n = objs.format.len();
    let mut i = 0;
    while i < body.len() {
        let sp = body[i..].iter().position(|b| *b == b' ').ok_or_else(|| TreeError::Remote("a malformed tree".into()))?;
        let mode = &body[i..i + sp];
        let nul = body[i + sp..].iter().position(|b| *b == 0).ok_or_else(|| TreeError::Remote("a malformed tree".into()))?;
        let name = body[i + sp + 1..i + sp + nul].to_vec();
        let child = body.get(i + sp + nul + 1..i + sp + nul + 1 + n).ok_or_else(|| TreeError::Remote("a malformed tree".into()))?;
        i += sp + nul + 1 + n;
        at.push(name);
        let segs: Vec<&[u8]> = at.iter().map(Vec::as_slice).collect();
        match mode {
            b"40000" | b"040000" => {
                PackageTree::check_dir(&segs).map_err(TreeError::Format)?;
                walk(objs, child, at, out)?;
            }
            b"100644" | b"100755" | b"100664" => {
                let Some((Kind::Blob, bytes)) = objs.get(child) else {
                    return Err(TreeError::Remote(format!("the remote did not send the blob of `{}`", String::from_utf8_lossy(&at.join(&b'/')))));
                };
                out.add(&segs, EntryKind::Regular, bytes.clone()).map_err(TreeError::Format)?;
            }
            b"120000" => out.add(&segs, EntryKind::Symlink, Vec::new()).map_err(TreeError::Format)?,
            b"160000" => out.add(&segs, EntryKind::Submodule, Vec::new()).map_err(TreeError::Format)?,
            _ => out.add(&segs, EntryKind::Other, Vec::new()).map_err(TreeError::Format)?,
        }
        at.pop();
    }
    Ok(())
}
