//! The proposed edit of `almide survive` / `almide apply` (#2147): read it,
//! decide whether it is a unified diff or the file's full new text, and turn it
//! into the after-text. Pure string work — nothing here touches the tree.

/// How `--with` content is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    /// A unified diff (`--- a/x` / `+++ b/x` / `@@ -l,n +l,n @@` hunks).
    Patch,
    /// The file's complete new contents.
    FullText,
}

impl EditKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EditKind::Patch => "patch",
            EditKind::FullText => "full_text",
        }
    }

    /// `--as` spelling → kind. `auto` is `None` (detect from the content).
    pub fn parse_flag(s: &str) -> Result<Option<EditKind>, String> {
        match s {
            "auto" => Ok(None),
            "patch" => Ok(Some(EditKind::Patch)),
            "text" | "full" | "full_text" | "full-text" => Ok(Some(EditKind::FullText)),
            other => Err(format!("--as expects auto | patch | text, got `{}`", other)),
        }
    }
}

/// Auto-detection: a unified diff opens (after optional `diff`/`index`
/// preamble lines) with `--- `, or is a bare hunk `@@ `. No well-formed `.almd`
/// file starts with either — `--` and `@@` are not Almide at the start of a
/// file — so the guess is unambiguous for real inputs; `--as` overrides it.
pub fn detect(content: &str) -> EditKind {
    for line in content.lines() {
        if line.starts_with("diff ") || line.starts_with("index ") {
            continue;
        }
        return if line.starts_with("--- ") || line.starts_with("@@ ") {
            EditKind::Patch
        } else {
            EditKind::FullText
        };
    }
    EditKind::FullText
}

/// Read `--with`: a path, or `-` for stdin.
pub fn read_with(src: &str) -> Result<String, String> {
    if src == "-" {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)
            .map_err(|e| format!("cannot read the edit from stdin: {}", e))?;
        Ok(s)
    } else {
        std::fs::read_to_string(src).map_err(|e| format!("cannot read the edit `{}`: {}", src, e))
    }
}

/// The after-text: `content` itself for a full text, `before` with the patch
/// applied for a diff.
pub fn after_text(before: &str, content: &str, kind: EditKind) -> Result<String, String> {
    match kind {
        EditKind::FullText => Ok(content.to_string()),
        EditKind::Patch => apply_patch(before, content),
    }
}

/// One `@@` hunk: where it claims to start (1-based old line) and its lines.
struct Hunk {
    old_start: usize,
    /// Context and removed lines, in order — what must be found in `before`.
    old: Vec<String>,
    /// Context and added lines, in order — what replaces it.
    new: Vec<String>,
    /// `\ No newline at end of file` followed the last new-side line.
    new_no_eol: bool,
}

/// `@@ -12,3 +12,4 @@ section` → (old start, old count, new count); a count
/// left out is 1.
fn parse_hunk_header(line: &str) -> Option<(usize, usize, usize)> {
    let rest = line.strip_prefix("@@ -")?;
    let mut parts = rest.split_whitespace();
    let range = |s: &str| -> Option<(usize, usize)> {
        let mut it = s.split(',');
        let start = it.next()?.parse().ok()?;
        let count = match it.next() { Some(c) => c.parse().ok()?, None => 1 };
        Some((start, count))
    };
    let (old_start, old_count) = range(parts.next()?)?;
    let (_, new_count) = range(parts.next()?.strip_prefix('+')?)?;
    Some((old_start, old_count, new_count))
}

fn parse_hunks(patch: &str) -> Result<Vec<Hunk>, String> {
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut files_named = 0usize;
    // What the previous body line was, for the `\ No newline` marker.
    let mut last_side: Option<char> = None;
    // Body lines still owed by the open hunk (old side, new side). While any
    // are owed, a line is body even when it reads like a header — removing a
    // line `-- x` spells `--- x`.
    let (mut owe_old, mut owe_new) = (0usize, 0usize);
    for line in patch.lines() {
        if line.starts_with('\\') {
            if let Some(h) = hunks.last_mut()
                && matches!(last_side, Some('+') | Some(' '))
            {
                h.new_no_eol = true;
            }
            continue;
        }
        if owe_old == 0 && owe_new == 0 {
            if line.starts_with("+++ ") {
                files_named += 1;
                if files_named > 1 {
                    return Err("the patch names more than one file; `--with` takes an edit to exactly the file being judged".into());
                }
                continue;
            }
            if let Some((start, oc, nc)) = parse_hunk_header(line) {
                hunks.push(Hunk { old_start: start, old: Vec::new(), new: Vec::new(), new_no_eol: false });
                (owe_old, owe_new) = (oc, nc);
                last_side = None;
                continue;
            }
            // Preamble (`diff`, `index`, `---`) or trailing text between hunks.
            if line.trim().is_empty() || line.starts_with("--- ") || line.starts_with("diff ") || line.starts_with("index ") || hunks.is_empty() {
                continue;
            }
            return Err(format!("patch line outside any hunk: {:?}", line));
        }
        let Some(h) = hunks.last_mut() else { continue };
        let (tag, body) = match line.chars().next() {
            Some(c @ (' ' | '-' | '+')) => (c, &line[1..]),
            // Some tools drop the lone space of an empty context line.
            None => (' ', ""),
            Some(_) => return Err(format!("unrecognised patch line (expected ' ', '-', '+' or '@@'): {:?}", line)),
        };
        match tag {
            ' ' => {
                h.old.push(body.to_string());
                h.new.push(body.to_string());
                owe_old = owe_old.saturating_sub(1);
                owe_new = owe_new.saturating_sub(1);
            }
            '-' => {
                h.old.push(body.to_string());
                owe_old = owe_old.saturating_sub(1);
            }
            _ => {
                h.new.push(body.to_string());
                owe_new = owe_new.saturating_sub(1);
            }
        }
        last_side = Some(tag);
    }
    if hunks.is_empty() {
        return Err("the patch has no `@@` hunk".into());
    }
    if owe_old != 0 || owe_new != 0 {
        return Err("the patch ends inside a hunk (its `@@` line counts more lines than follow)".into());
    }
    Ok(hunks)
}

/// Where `needle` occurs in `hay` nearest to `hint` (0-based), at or after
/// `floor` (hunks apply in order and never overlap).
fn locate(hay: &[String], needle: &[String], hint: usize, floor: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(hint.clamp(floor, hay.len()));
    }
    if needle.len() > hay.len() {
        return None;
    }
    let last = hay.len() - needle.len();
    let at = |i: usize| i >= floor && i <= last && hay[i..i + needle.len()] == *needle;
    let hint = hint.min(last);
    for d in 0..=hay.len() {
        if hint >= d && at(hint - d) {
            return Some(hint - d);
        }
        if at(hint + d) {
            return Some(hint + d);
        }
        if hint < d && hint + d > last {
            break;
        }
    }
    None
}

/// Apply a unified diff to `before`. Every hunk must match exactly (context
/// and removed lines); a hunk that does not is an error, never a fuzzy guess —
/// judging a different edit from the one proposed would be a wrong verdict.
pub fn apply_patch(before: &str, patch: &str) -> Result<String, String> {
    let hunks = parse_hunks(patch)?;
    let had_eol = before.is_empty() || before.ends_with('\n');
    let mut lines: Vec<String> = before.lines().map(|l| l.to_string()).collect();
    let mut offset: isize = 0;
    let mut floor = 0usize;
    let mut final_no_eol: Option<bool> = None;
    for (i, h) in hunks.iter().enumerate() {
        let hint = (h.old_start.saturating_sub(1) as isize + offset).max(0) as usize;
        let at = locate(&lines, &h.old, hint, floor).ok_or_else(|| {
            format!("hunk {} (@@ -{}) does not apply: its context and removed lines are not in the file", i + 1, h.old_start)
        })?;
        let end = at + h.old.len();
        lines.splice(at..end, h.new.iter().cloned());
        floor = at + h.new.len();
        offset += h.new.len() as isize - h.old.len() as isize;
        if floor == lines.len() {
            final_no_eol = Some(h.new_no_eol);
        }
    }
    let mut out = lines.join("\n");
    let no_eol = final_no_eol.unwrap_or(!had_eol);
    if !lines.is_empty() && !no_eol {
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BEFORE: &str = "a\nb\nc\nd\ne\n";

    #[test]
    fn detection_tells_a_diff_from_a_file() {
        assert_eq!(detect("--- a/x.almd\n+++ b/x.almd\n@@ -1 +1 @@\n-a\n+b\n"), EditKind::Patch);
        assert_eq!(detect("diff --git a/x b/x\nindex 1..2\n--- a/x\n"), EditKind::Patch);
        assert_eq!(detect("@@ -1 +1 @@\n-a\n+b\n"), EditKind::Patch);
        assert_eq!(detect("fn main() -> Unit = ()\n"), EditKind::FullText);
        assert_eq!(detect("// --- not a diff\n"), EditKind::FullText);
    }

    #[test]
    fn a_hunk_applies_at_its_line_and_keeps_the_final_newline() {
        let p = "--- a/x\n+++ b/x\n@@ -2,2 +2,3 @@\n b\n-c\n+C\n+c2\n";
        assert_eq!(apply_patch(BEFORE, p).unwrap(), "a\nb\nC\nc2\nd\ne\n");
    }

    #[test]
    fn a_shifted_hunk_is_found_by_its_context() {
        // Claims line 1, but the context sits at line 4.
        let p = "@@ -1,2 +1,2 @@\n d\n-e\n+E\n";
        assert_eq!(apply_patch(BEFORE, p).unwrap(), "a\nb\nc\nd\nE\n");
    }

    #[test]
    fn a_hunk_whose_context_is_absent_is_refused() {
        let e = apply_patch(BEFORE, "@@ -1,1 +1,1 @@\n-zzz\n+y\n").unwrap_err();
        assert!(e.contains("does not apply"), "{}", e);
    }

    #[test]
    fn the_no_newline_marker_is_honoured() {
        let p = "@@ -5,1 +5,1 @@\n-e\n+E\n\\ No newline at end of file\n";
        assert_eq!(apply_patch(BEFORE, p).unwrap(), "a\nb\nc\nd\nE");
    }

    #[test]
    fn a_patch_naming_two_files_is_refused() {
        let p = "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n--- a/y\n+++ b/y\n@@ -1 +1 @@\n-a\n+b\n";
        assert!(apply_patch(BEFORE, p).unwrap_err().contains("more than one file"));
    }

    #[test]
    fn multiple_hunks_apply_in_order_with_offsets() {
        let p = "@@ -1,1 +1,2 @@\n a\n+a2\n@@ -4,1 +5,1 @@\n-d\n+D\n";
        assert_eq!(apply_patch(BEFORE, p).unwrap(), "a\na2\nb\nc\nD\ne\n");
    }
}
