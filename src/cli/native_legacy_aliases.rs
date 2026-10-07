//! The pre-#3338 names of a package's own items, kept callable from its
//! `native/*.rs` for a deprecation window (#3425).
//!
//! #3338 made `module_ident` injective, which respelled every item a native
//! module calls back into: `almide_rt_tf_v0_entry` became
//! `almide_rt_tf_0v0_entry`, `almide_rt_tf_v0_calc_double` became
//! `almide_rt_tf_0v0_1calc_double`. Native code written against a released
//! compiler names the old spellings, so a crate that copies a `native/` tree
//! also gets one `use` alias per old spelling of the package's items — the
//! root module's and its sub-modules' fns, under the versioned `<pkg>_v<N>`
//! module path.
//!
//! An alias is emitted only when its old spelling is unambiguous: the old
//! spelling folded `.` to `_`, so `a.b` and `a_b` (and module `a` with fn
//! `b_c`) shared one name — the collision #3338 fixed. Such a name gets no
//! alias, and neither does one that some other item already defines.
//!
//! Only crates carrying a package `native/` tree get aliases; every other
//! build is byte-identical. A native file that names an old spelling gets a
//! warning, once per name per process, naming the new spelling and the dialect
//! epoch that removes the alias.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Mutex;

use almide_base::names::{legacy_module_ident, split_module_item};

/// The dialect epoch whose break list removes the aliases; recorded in the
/// `[[deprecation]]` entry `legacy-native-callback-names` of
/// `proofs/dialect-epochs.toml` (scripts/check-dialect-epochs.sh cross-checks
/// the two).
pub(super) const LEGACY_NATIVE_CALLBACK_REMOVAL_EPOCH: u32 = 13;

/// The old spellings resolved against one crate's code.
#[derive(Debug, Default, PartialEq)]
pub(super) struct LegacyAliases {
    /// old spelling → the item it now names.
    pub aliases: BTreeMap<String, String>,
    /// old spellings of a package item that no alias carries: the items
    /// sharing the name (or the one already defining it).
    pub ambiguous: BTreeMap<String, BTreeSet<String>>,
}

/// The name a top-level item line defines, with whether it is a `fn`.
fn defined_item(line: &str) -> Option<(&str, bool)> {
    if line.starts_with(char::is_whitespace) {
        return None;
    }
    let mut rest = line;
    for prefix in ["pub(crate) ", "pub ", "async ", "unsafe ", "extern \"C\" "] {
        rest = rest.strip_prefix(prefix).unwrap_or(rest);
    }
    if rest.starts_with("const fn ") {
        rest = &rest["const ".len()..];
    }
    let (is_fn, rest) = ["fn ", "struct ", "enum ", "static mut ", "static ", "const ", "type ", "trait ", "union "]
        .iter()
        .find_map(|kw| rest.strip_prefix(kw).map(|r| (*kw == "fn ", r)))?;
    let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(rest.len());
    (end > 0).then(|| (&rest[..end], is_fn))
}

/// The versioned package module path a module path belongs to: its first
/// segment is `<pkg>_v<N>`.
fn is_versioned_package_path(path: &str, pkg: &str) -> bool {
    let first = path.split('.').next().unwrap_or("");
    first
        .strip_prefix(pkg)
        .and_then(|r| r.strip_prefix("_v"))
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// The aliases `code` gets for the items of the packages named `pkgs` (the
/// packages whose `native/` tree the crate carries).
pub(super) fn legacy_aliases(code: &str, pkgs: &[String]) -> LegacyAliases {
    let mut defined: HashSet<&str> = HashSet::new();
    // Every old spelling → the generated items that would share it.
    let mut by_legacy: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut candidates: Vec<(String, String)> = Vec::new();
    for (name, is_fn) in code.lines().filter_map(defined_item) {
        defined.insert(name);
        let Some(ident) = name.strip_prefix("almide_rt_") else { continue };
        if !is_fn {
            continue;
        }
        let Some((path, item)) = split_module_item(ident) else { continue };
        let legacy = format!("almide_rt_{}_{}", legacy_module_ident(&path), item);
        if legacy == name {
            continue;
        }
        by_legacy.entry(legacy.clone()).or_default().insert(name.to_string());
        if pkgs.iter().any(|p| is_versioned_package_path(&path, p)) {
            candidates.push((legacy, name.to_string()));
        }
    }
    let mut out = LegacyAliases::default();
    for (legacy, new) in candidates {
        let sharing = &by_legacy[&legacy];
        if sharing.len() == 1 && !defined.contains(legacy.as_str()) {
            out.aliases.insert(legacy, new);
        } else {
            let mut names = sharing.clone();
            if defined.contains(legacy.as_str()) {
                names.insert(legacy.clone());
            }
            out.ambiguous.insert(legacy, names);
        }
    }
    out
}

/// The alias items appended to the crate root. Empty when there are none.
pub(super) fn render(aliases: &LegacyAliases) -> String {
    if aliases.aliases.is_empty() {
        return String::new();
    }
    let mut out = format!(
        "\n// Pre-#3338 spellings a package's native/ code may still call (#3425); removed at dialect epoch {}.\n",
        LEGACY_NATIVE_CALLBACK_REMOVAL_EPOCH
    );
    for (legacy, new) in &aliases.aliases {
        out.push_str(&format!("#[doc(hidden)]\n#[allow(unused_imports)]\nuse crate::{new} as {legacy};\n"));
    }
    out
}

/// Every `almide_rt_…` identifier `text` names.
fn named_items(text: &str) -> BTreeSet<&str> {
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    text.match_indices("almide_rt_")
        .filter(|(at, _)| !text[..*at].ends_with(is_ident))
        .map(|(at, _)| {
            let rest = &text[at..];
            &rest[..rest.find(|c: char| !is_ident(c)).unwrap_or(rest.len())]
        })
        .collect()
}

/// The warnings for the old spellings `file`'s text names, each one at most
/// once per process.
pub(super) fn warnings(file: &str, text: &str, aliases: &LegacyAliases) -> Vec<String> {
    static WARNED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
    let mut out = Vec::new();
    for name in named_items(text) {
        let message = if let Some(new) = aliases.aliases.get(name) {
            format!(
                "warning: {file} calls `{name}`, the spelling before #3338 of `{new}`. \
                 The old name is a deprecated alias, removed at dialect epoch {epoch} — write `crate::{new}`.",
                epoch = LEGACY_NATIVE_CALLBACK_REMOVAL_EPOCH
            )
        } else if let Some(sharing) = aliases.ambiguous.get(name) {
            let names: Vec<String> = sharing.iter().map(|n| format!("`{n}`")).collect();
            format!(
                "warning: {file} calls `{name}`, the spelling before #3338 that several items share ({}), \
                 so it has no alias — write the item's own name.",
                names.join(", ")
            )
        } else {
            continue;
        };
        if WARNED.lock().map(|mut w| w.insert(name.to_string())).unwrap_or(true) {
            out.push(message);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const CODE: &str = "\
pub fn almide_rt_tf_0v0_entry(x: i64) -> i64 {
    almide_rt_tf_0v0_1calc_double(x)
}
pub fn almide_rt_tf_0v0_1calc_double(x: i64) -> i64 { x * 2 }
fn almide_rt_list_len<T>(xs: &[T]) -> i64 { 0 }
fn main() {}
";

    #[test]
    fn root_and_sub_module_fns_get_their_old_spelling() {
        let a = legacy_aliases(CODE, &["tf".to_string()]);
        assert_eq!(a.aliases.get("almide_rt_tf_v0_entry").map(String::as_str), Some("almide_rt_tf_0v0_entry"));
        assert_eq!(a.aliases.get("almide_rt_tf_v0_calc_double").map(String::as_str), Some("almide_rt_tf_0v0_1calc_double"));
        assert_eq!(a.aliases.len(), 2);
        assert!(render(&a).contains("use crate::almide_rt_tf_0v0_entry as almide_rt_tf_v0_entry;"));
    }

    #[test]
    fn no_package_no_alias() {
        let a = legacy_aliases(CODE, &[]);
        assert!(a.aliases.is_empty());
        assert_eq!(render(&a), "");
        assert!(legacy_aliases(CODE, &["other".to_string()]).aliases.is_empty());
    }

    #[test]
    fn a_shared_old_spelling_gets_no_alias() {
        let code = "\
pub fn almide_rt_tf_0v0_1a_1b_c() -> i64 { 1 }
pub fn almide_rt_tf_0v0_1a_0b_c() -> i64 { 2 }
pub fn almide_rt_tf_0v0_1x_y() -> i64 { 3 }
pub fn almide_rt_tf_0v0_x_y() -> i64 { 4 }
pub fn almide_rt_tf_0v0_1solo_f() -> i64 { 5 }
";
        let a = legacy_aliases(code, &["tf".to_string()]);
        assert!(!a.aliases.contains_key("almide_rt_tf_v0_a_b_c"));
        assert_eq!(a.ambiguous["almide_rt_tf_v0_a_b_c"].len(), 2);
        assert!(!a.aliases.contains_key("almide_rt_tf_v0_x_y"));
        assert_eq!(a.aliases.get("almide_rt_tf_v0_solo_f").map(String::as_str), Some("almide_rt_tf_0v0_1solo_f"));
    }

    #[test]
    fn an_old_spelling_some_item_defines_gets_no_alias() {
        let code = "pub fn almide_rt_tf_0v0_entry() {}\nstatic almide_rt_tf_v0_entry: i64 = 0;\n";
        let a = legacy_aliases(code, &["tf".to_string()]);
        assert!(a.aliases.is_empty());
    }

    #[test]
    fn native_text_warns_once_per_old_name() {
        let a = legacy_aliases(CODE, &["tf".to_string()]);
        let text = "crate::almide_rt_tf_v0_entry(1) + crate::almide_rt_tf_v0_entry(2) + crate::almide_rt_tf_0v0_entry(3)";
        let w = warnings("native/host.rs", text, &a);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("`almide_rt_tf_0v0_entry`") && w[0].contains("dialect epoch 13"), "{}", w[0]);
        assert!(warnings("native/host.rs", text, &a).is_empty());
    }
}
