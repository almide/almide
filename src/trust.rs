//! `[trust] owners` (#4093): which source owners a project fetches from.
//!
//! Models invent repository names, and an invented name repeats across runs,
//! so an attacker can register it before the model asks for it. The
//! toolchain therefore fetches a git dependency only from an owner the ROOT
//! manifest trusts:
//!
//! - `github.com/almide` is trusted by default;
//! - any other owner, on any host, must be listed as `host/owner` in the root
//!   `almide.toml`'s `[trust] owners`;
//! - `path =` dependencies, and git sources on the local filesystem
//!   (`file://` or a plain path), are exempt: nobody can register a name
//!   there;
//! - a dependency's own `[trust]` is never read, so a package cannot grant
//!   trust on the root's behalf.
//!
//! The check runs before any network access, on `almide add` and on every
//! fetch (`build` / `run` / `test` / `bench` / `update`), for direct,
//! transitive and already-locked dependencies alike. There is no flag or
//! environment variable that trusts everything: a grant always names its
//! owner, in a file that is reviewed with the rest of the change.

/// The owner every project trusts without a grant.
pub const DEFAULT_TRUSTED_OWNER: &str = "github.com/almide";

/// The `host/owner` a git source belongs to, lowercased.
///
/// `Ok(None)` for a source on the local filesystem (`file://…` or a path),
/// which needs no trust. `Err` for a URL whose owner cannot be read safely:
/// a `.` or `..` path segment or a percent escape could make the URL fetch
/// from another owner than the one its text names.
pub fn source_owner(url: &str) -> Result<Option<String>, String> {
    let (host, path) = if let Some((scheme, rest)) = url.split_once("://") {
        if scheme.eq_ignore_ascii_case("file") {
            return Ok(None);
        }
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
        (host.split(':').next().unwrap_or(host), path)
    } else if let Some((before, path)) = url.split_once(':').filter(|(b, _)| !b.contains('/') && !b.is_empty()) {
        // scp-like `git@host:owner/repo`.
        (before.rsplit_once('@').map_or(before, |(_, h)| h), path)
    } else {
        return Ok(None);
    };
    let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    if segments.iter().any(|s| *s == "." || *s == ".." || s.contains('%') || s.contains('\\')) || host.contains('%') {
        return Err(format!(
            "the source `{url}` has a `.`/`..` segment or an escape in its path, so the owner it fetches from cannot be read from its text\n  \
             hint: write the URL as https://host/owner/repo"
        ));
    }
    let owner = segments.first().copied().unwrap_or("");
    if host.is_empty() || owner.is_empty() {
        return Err(format!("the source `{url}` names no owner\n  hint: write the URL as https://host/owner/repo"));
    }
    Ok(Some(format!("{}/{}", host.to_ascii_lowercase(), owner.to_ascii_lowercase())))
}

/// `[trust] owners` entry spelling: `host/owner`, one `/`, both parts
/// non-empty. `Err` names the problem.
pub fn validate_owner_entry(entry: &str) -> Result<(), String> {
    match entry.split_once('/') {
        Some((host, owner)) if !host.is_empty() && !owner.is_empty() && !owner.contains('/') && host.contains('.') => Ok(()),
        _ => Err(format!(
            "[trust].owners entry `{entry}` is not `host/owner`\n  hint: write the host and the owner, like \"github.com/{}\"",
            entry.rsplit('/').next().unwrap_or(entry)
        )),
    }
}

/// An owner as written on the command line (`--trust`, `almide trust`):
/// `host/owner`, with an `https://` prefix or a trailing `/` tolerated, or a
/// bare `owner`, which means `github.com/owner`. Lowercased, like
/// [`source_owner`].
pub fn owner_arg(arg: &str) -> String {
    let t = arg.trim().trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/');
    let full = if t.contains('/') { t.to_string() } else { format!("github.com/{t}") };
    full.to_ascii_lowercase()
}

/// Is `owner` (from [`source_owner`]) trusted by `proj`'s `[trust] owners`
/// or by default?
pub fn is_trusted(proj_owners: &[String], owner: &str) -> bool {
    owner == DEFAULT_TRUSTED_OWNER || proj_owners.iter().any(|o| o.eq_ignore_ascii_case(owner))
}

/// The trust refusal for a dependency fetched while building (`chain` is the
/// path of dependency names from the root, the refused one last).
pub fn fetch_refusal(chain: &[String], url: &str, owner: &str) -> String {
    let name = chain.last().map(String::as_str).unwrap_or("?");
    let via = if chain.len() > 1 { format!(" (required by {})", chain.join(" → ")) } else { String::new() };
    let mut msg = format!(
        "error: dependency `{name}`{via} is fetched from an owner this project does not trust\n  \
         source: {url}\n  \
         owner: {owner} (not in [trust] owners)\n  \
         note: models often invent repository names, and attackers register the names models invent\n  \
         hint: if this dependency is expected, run `almide trust {owner}` (it adds \"{owner}\" to [trust] owners in almide.toml)\n  \
         hint: if you chose this name yourself, do not trust it; use a package from github.com/almide or ask the user"
    );
    if let Some(s) = suggestion(url) {
        msg.push_str(&format!("\n  {s}"));
    }
    msg
}

/// The trust refusal for `almide add`. `retry` is the add command line to
/// rerun with `--trust`.
pub fn add_refusal(name: &str, url: &str, owner: &str, retry: &str) -> String {
    let mut msg = format!(
        "error: refusing to add `{name}`: its owner is not trusted by this project\n  \
         source: {url}\n  \
         owner: {owner} (not in [trust] owners)\n  \
         note: models often invent repository names, and attackers register the names models invent\n  \
         hint: if the user asked for this exact repository, run:\n          {retry} --trust {owner}\n  \
         hint: if you chose this name yourself, do not trust it; use a package from github.com/almide or ask the user"
    );
    if let Some(s) = suggestion(url) {
        msg.push_str(&format!("\n  {s}"));
    }
    msg
}

// ── Near names and the github.com/almide package snapshot ─────────────

/// `<package-name> <repository>` for every package under github.com/almide,
/// generated by `scripts/gen-almide-packages.sh`.
const ALMIDE_PACKAGES: &str = include_str!("almide_packages.txt");

/// `(package name, repository)` pairs from the embedded snapshot.
pub fn almide_packages() -> Vec<(&'static str, &'static str)> {
    ALMIDE_PACKAGES
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once(' '))
        .collect()
}

/// The repository under github.com/almide that the one-part name `name`
/// means: the snapshot's repository for that package name or repository
/// name, else `name` itself.
pub fn almide_repo_for(name: &str) -> (&str, String) {
    match almide_packages().into_iter().find(|(pkg, repo)| *pkg == name || *repo == name) {
        Some((pkg, repo)) => (pkg, repo.to_string()),
        None => (name, name.to_string()),
    }
}

/// Lowercase, an `almide-` / `almide_` / `almide.` prefix stripped, `-`
/// `_` `.` folded away — the spelling near-name detection compares.
fn fold(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let lower = lower.strip_suffix(".git").unwrap_or(&lower);
    let stripped = ["almide-", "almide_", "almide."].iter().find_map(|p| lower.strip_prefix(p)).unwrap_or(lower);
    stripped.chars().filter(|c| !matches!(c, '-' | '_' | '.')).collect()
}

fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            cur.push((prev[j] + usize::from(ca != cb)).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// A "did you mean" line for a refused or not-found source, or `None`.
///
/// Three shapes, after [`fold`]: the same name as an almide package under
/// another owner; an almide package within edit distance 2 (and a third of
/// the name's length); an owner that is
/// a near miss of `almide` (`almide-lang`, `almlde`). It only suggests; it
/// never substitutes.
pub fn suggestion(url: &str) -> Option<String> {
    let owner = source_owner(url).ok().flatten()?;
    let (host, owner_name) = owner.split_once('/')?;
    let repo = url.trim_end_matches('/').rsplit('/').next()?;
    let wanted = fold(repo);
    let packages = almide_packages();
    let line = |pkg: &str, repo: &str| format!("hint: did you mean `almide add {pkg}`?  (github.com/almide/{repo})");
    if owner != DEFAULT_TRUSTED_OWNER {
        if let Some((pkg, r)) = packages.iter().find(|(p, r)| fold(p) == wanted || fold(r) == wanted) {
            return Some(line(pkg, r));
        }
    }
    let near = packages
        .iter()
        .filter(|(_, r)| *r != repo || owner != DEFAULT_TRUSTED_OWNER)
        .map(|(p, r)| (edit_distance(&fold(p), &wanted).min(edit_distance(&fold(r), &wanted)), p, r))
        // Within 2 edits, and at most a third of the name: `mylib` is two
        // edits from `nnlib` and means nothing by it.
        .filter(|(d, _, _)| *d <= 2 && *d * 3 <= wanted.len())
        .min_by_key(|(d, p, _)| (*d, p.len()));
    if let Some((_, pkg, r)) = near {
        return Some(line(pkg, r));
    }
    let folded_owner: String = owner_name.chars().filter(|c| !matches!(c, '-' | '_' | '.')).collect();
    if host == "github.com" && owner_name != "almide" && (folded_owner.contains("almide") || {
        let d = edit_distance(&folded_owner, "almide");
        d <= 2 && d * 3 <= folded_owner.len()
    }) {
        return Some(format!("hint: `{owner}` is not the Almide organization; did you mean github.com/almide/{repo}?"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owners_are_host_and_first_segment() {
        assert_eq!(source_owner("https://github.com/Almide/svg").unwrap().as_deref(), Some("github.com/almide"));
        assert_eq!(source_owner("https://x@GitLab.com:443/alice/lib.git").unwrap().as_deref(), Some("gitlab.com/alice"));
        assert_eq!(source_owner("git@github.com:bob/util.git").unwrap().as_deref(), Some("github.com/bob"));
        assert_eq!(source_owner("ssh://git@github.com/bob/util").unwrap().as_deref(), Some("github.com/bob"));
        assert_eq!(source_owner("file:///srv/git/x").unwrap(), None);
        assert_eq!(source_owner("/srv/git/x").unwrap(), None);
        assert!(source_owner("https://github.com/almide/../evil/x").is_err());
        assert!(source_owner("https://github.com/almide/%2e%2e/evil/x").is_err());
    }

    #[test]
    fn near_names_suggest_and_never_substitute() {
        assert!(suggestion("https://github.com/almide-lang/csv").unwrap().contains("almide add csv"));
        assert!(suggestion("https://github.com/almide/csvv").unwrap().contains("almide add csv"));
        assert!(suggestion("https://github.com/alice/almide-bindgen").unwrap().contains("almide add bindgen"));
        assert!(suggestion("https://github.com/almlde/whatever").unwrap().contains("github.com/almide/whatever"));
        assert_eq!(suggestion("https://github.com/alice/totally-unrelated-thing"), None);
        assert_eq!(suggestion("https://github.com/alice/mylib"), None, "two edits on a five-letter name is noise");
    }

    #[test]
    fn one_part_names_resolve_through_the_snapshot() {
        assert_eq!(almide_repo_for("bindgen").1, "almide-bindgen");
        assert_eq!(almide_repo_for("almide-bindgen"), ("bindgen", "almide-bindgen".to_string()));
        assert_eq!(almide_repo_for("csv").1, "csv");
        assert_eq!(almide_repo_for("not_listed").1, "not_listed");
    }
}
