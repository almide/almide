//! `almide remove <name>` (#4590): the twin of `almide add`. It drops the
//! dependency's entry from almide.toml `[dependencies]` and from
//! almide.lock, so `add X` then `remove X` leaves both files as they were.
//! `[trust] owners` is left alone: an owner may vouch for other packages,
//! and `trust` is its own verb.

use crate::{err, project};
use std::path::Path;

/// The manifest text without `name`'s dependency entry, or why it cannot be
/// removed. Handles the two spellings a manifest holds: the one-line
/// `name = { ... }` that `add` writes inside `[dependencies]`, and a
/// `[dependencies.name]` table. A `[dependencies]` table left empty is
/// dropped with the blank line `add` put before it, so `add` then `remove`
/// on a manifest without dependencies returns its exact bytes.
pub(crate) fn remove_from_manifest(content: &str, name: &str) -> Result<String, String> {
    let lines: Vec<&str> = content.split_inclusive('\n').collect();
    let header = |l: &str| -> Option<String> {
        let t = l.trim();
        (t.starts_with('[') && t.ends_with(']')).then(|| t.trim_matches(|c| c == '[' || c == ']').trim().to_string())
    };
    let key_of = |l: &str| -> Option<String> {
        let (k, _) = l.split_once('=')?;
        Some(k.trim().trim_matches('"').to_string())
    };
    let mut section = String::new();
    let mut keep: Vec<bool> = vec![true; lines.len()];
    let mut found = false;
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i];
        if let Some(h) = header(l) {
            section = h.clone();
            if h == format!("dependencies.{name}") {
                // The whole `[dependencies.name]` table, up to the next header.
                found = true;
                keep[i] = false;
                let mut j = i + 1;
                while j < lines.len() && header(lines[j]).is_none() {
                    keep[j] = false;
                    j += 1;
                }
                i = j;
                continue;
            }
        } else if section == "dependencies" && key_of(l).as_deref() == Some(name) {
            if l.contains('{') && !l.contains('}') {
                return Err(format!(
                    "the entry for `{name}` in almide.toml spans several lines; remove it by hand"
                ));
            }
            found = true;
            keep[i] = false;
        }
        i += 1;
    }
    if !found {
        return Err(format!("dependency `{name}` is not in almide.toml"));
    }
    let mut kept: Vec<&str> = lines.iter().zip(&keep).filter(|(_, k)| **k).map(|(l, _)| *l).collect();
    drop_empty_dependencies_table(&mut kept, &header);
    Ok(kept.concat())
}

/// Drop a `[dependencies]` header with no entry under it (blank lines and
/// comments do not count). When it was the last table, the blank line before
/// it goes too — the pair `add` appends to a manifest without dependencies.
fn drop_empty_dependencies_table(lines: &mut Vec<&str>, header: &dyn Fn(&str) -> Option<String>) {
    let Some(pos) = lines.iter().position(|l| header(l).as_deref() == Some("dependencies")) else { return };
    let mut end = pos + 1;
    while end < lines.len() && header(lines[end]).is_none() {
        let t = lines[end].trim();
        if !t.is_empty() && !t.starts_with('#') {
            return;
        }
        end += 1;
    }
    let only_blank_after = lines[pos + 1..end].iter().all(|l| l.trim().is_empty());
    if !only_blank_after {
        return;
    }
    let mut start = pos;
    if end == lines.len() && start > 0 && lines[start - 1].trim().is_empty() {
        start -= 1;
    }
    lines.drain(start..end);
}

/// `almide remove <name>`. Writes the manifest, then the lock without the
/// entry (deleting the lock when it held only that entry). A failure exits 1
/// with both files as they were.
pub fn cmd_remove(name: &str) {
    let fail = |msg: &str| -> ! {
        err(msg);
        std::process::exit(1);
    };
    let manifest = Path::new("almide.toml");
    if !manifest.exists() {
        fail("almide.toml not found. Run 'almide init' first.");
    }
    let proj = project::parse_toml(manifest).unwrap_or_else(|e| fail(&e));
    if !proj.dependencies.iter().any(|d| d.name == name) {
        let names: Vec<&str> = proj.dependencies.iter().map(|d| d.name.as_str()).collect();
        let listed = if names.is_empty() { "none".to_string() } else { names.join(", ") };
        fail(&format!("error: dependency `{name}` is not in almide.toml (its dependencies: {listed})"));
    }
    let content = project::read_project_file(manifest).unwrap_or_else(|e| fail(&format!("error: {e}")));
    let updated = remove_from_manifest(&content, name).unwrap_or_else(|e| fail(&format!("error: {e}")));
    let lock = proj.root.join("almide.lock");
    let locked = if lock.exists() {
        Some(project::parse_lock_file(&lock).unwrap_or_else(|e| fail(&format!("error: {e}"))))
    } else {
        None
    };
    std::fs::write(manifest, &updated).unwrap_or_else(|e| fail(&format!("error: failed to write almide.toml: {e}")));
    // The manifest must still read, and without the dependency.
    match project::parse_toml(manifest) {
        Ok(p) if !p.dependencies.iter().any(|d| d.name == name) => {}
        _ => {
            let _ = std::fs::write(manifest, &content);
            fail(&format!("error: could not remove `{name}` from almide.toml cleanly; edit it by hand\n  note: almide.toml is unchanged"));
        }
    }
    if let Some(entries) = locked {
        let rest: Vec<project::LockedDep> = entries.into_iter().filter(|d| d.name != name).collect();
        let written = if rest.is_empty() {
            std::fs::remove_file(&lock).map_err(|e| format!("failed to remove {}: {e}", lock.display()))
        } else {
            project::write_lock_file(&lock, &rest)
        };
        if let Err(e) = written {
            let _ = std::fs::write(manifest, &content);
            fail(&format!("error: {e}\n  note: almide.toml is unchanged"));
        }
    }
    err(&format!("Removed {name} from almide.toml"));
}

#[cfg(test)]
mod tests {
    use super::remove_from_manifest;

    const BASE: &str = "[package]\nname = \"app\"\nversion = \"0.1.0\"\n";

    #[test]
    fn add_then_remove_on_a_manifest_without_dependencies_is_the_identity() {
        // What `add_dep_to_toml` appends to a manifest with no [dependencies].
        let added = format!("{BASE}\n[dependencies]\njson = {{ git = \"https://github.com/almide/json\", tag = \"v1\" }}\n");
        assert_eq!(remove_from_manifest(&added, "json").unwrap(), BASE);
    }

    #[test]
    fn add_then_remove_next_to_another_dependency_is_the_identity() {
        let before = format!("{BASE}\n[dependencies]\nhttp = {{ git = \"https://github.com/almide/http\" }}\n");
        // `add` inserts its line right under the header.
        let added = before.replacen("[dependencies]\n", "[dependencies]\njson = { git = \"https://github.com/almide/json\" }\n", 1);
        assert_eq!(remove_from_manifest(&added, "json").unwrap(), before);
    }

    #[test]
    fn a_table_spelled_dependency_is_removed_whole() {
        let m = format!("{BASE}\n[dependencies.json]\ngit = \"https://github.com/almide/json\"\ntag = \"v1\"\n\n[trust]\nowners = []\n");
        assert_eq!(remove_from_manifest(&m, "json").unwrap(), format!("{BASE}\n[trust]\nowners = []\n"));
    }

    #[test]
    fn a_name_that_is_not_declared_is_an_error() {
        assert!(remove_from_manifest(BASE, "json").is_err());
        // A key of the same name in another table is not the dependency.
        let m = format!("{BASE}\n[native-deps]\njson = \"1\"\n");
        assert!(remove_from_manifest(&m, "json").is_err());
    }
}
