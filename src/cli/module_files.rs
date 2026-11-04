//! A test build's per-module Rust modules, each in a file of its own (#4441).
//!
//! The renderer groups a test program's fns, and a test run's tests, into one
//! Rust module per Almide module, and marks each group with a line before and
//! after it. Written into one `main.rs`, any edit that adds or drops a line
//! moved every later item: their panic locations changed, and a rebuild
//! re-optimised every module after the edit. Here each marked module's body
//! goes to `<name>.rs` beside the crate root, `include!`d in place, so a line
//! added in one module moves that module's lines only. The marked form is
//! valid Rust as it stands; a group whose markers do not frame exactly
//! `mod <name> {` … `}` stays inline.

use std::path::Path;

use almide_base::names::{MODULE_FILE_BEGIN as BEGIN, MODULE_FILE_END as END};

/// `rs` with every marked module's body moved to `<dir>/<name>.rs` (written
/// only when its bytes change, so an unchanged module's file keeps its
/// mtime) and `include!`d where it was.
pub(super) fn split_module_files(rs: &str, dir: &Path) -> Result<String, String> {
    let lines: Vec<&str> = rs.split('\n').collect();
    let mut out = String::with_capacity(rs.len() / 4);
    let mut i = 0;
    while i < lines.len() {
        if let Some((name, end)) = marked_group(&lines, i) {
            let open = lines[i + 1];
            let indent = &open[..open.len() - open.trim_start().len()];
            let mut body = lines[i + 2..end - 1].join("\n");
            body.push('\n');
            let path = dir.join(format!("{name}.rs"));
            if std::fs::read(&path).ok().as_deref() != Some(body.as_bytes()) {
                std::fs::write(&path, &body).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
            }
            out.push_str(&format!("{indent}mod {name} {{ include!(\"{name}.rs\"); }}"));
            i = end + 1;
        } else {
            out.push_str(lines[i]);
            i += 1;
        }
        if i < lines.len() {
            out.push('\n');
        }
    }
    Ok(out)
}

/// The group whose begin marker is line `at`: its name and its end marker's
/// line, when the markers frame exactly `mod <name> {` … `}`.
fn marked_group<'a>(lines: &[&'a str], at: usize) -> Option<(&'a str, usize)> {
    let name = lines[at].trim_start().strip_prefix(BEGIN)?;
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return None;
    }
    let end_marker = format!("{END}{name}");
    let end = (at + 1..lines.len()).find(|&j| lines[j].trim_start() == end_marker)?;
    let framed = end >= at + 3
        && lines[at + 1].trim_start() == format!("mod {name} {{")
        && lines[end - 1].trim_start() == "}";
    framed.then_some((name, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marked_module_moves_to_its_file_and_the_rest_stays() {
        let dir = tempfile::tempdir().unwrap();
        let rs = "fn root() {}\n// almide:module-file almide_fns_a\nmod almide_fns_a {\nuse super::*;\npub fn f() {}\n}\n// almide:end-module-file almide_fns_a\npub(crate) use almide_fns_a::*;\n";
        let got = split_module_files(rs, dir.path()).unwrap();
        assert_eq!(got, "fn root() {}\nmod almide_fns_a { include!(\"almide_fns_a.rs\"); }\npub(crate) use almide_fns_a::*;\n");
        assert_eq!(std::fs::read_to_string(dir.path().join("almide_fns_a.rs")).unwrap(), "use super::*;\npub fn f() {}\n");
    }

    #[test]
    fn an_indented_group_keeps_its_indent() {
        let dir = tempfile::tempdir().unwrap();
        let rs = "mod tests {\n    // almide:module-file almide_tests_a\n    mod almide_tests_a {\n    use super::*;\n    }\n    // almide:end-module-file almide_tests_a\n}";
        let got = split_module_files(rs, dir.path()).unwrap();
        assert_eq!(got, "mod tests {\n    mod almide_tests_a { include!(\"almide_tests_a.rs\"); }\n}");
    }

    #[test]
    fn markers_that_do_not_frame_the_module_leave_the_text_alone() {
        let dir = tempfile::tempdir().unwrap();
        for rs in [
            "// almide:module-file x\nmod y {\n}\n// almide:end-module-file x\n",
            "// almide:module-file x\nmod x {\n}\n",
            "// almide:module-file ../x\nmod ../x {\n}\n// almide:end-module-file ../x\n",
        ] {
            assert_eq!(split_module_files(rs, dir.path()).unwrap(), rs);
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
