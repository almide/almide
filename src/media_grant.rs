//! `[trust] media` (ADR-0032 D7): which dependency packages may own a
//! script-capable media type.
//!
//! A package type may conform to `Body` and own media types with
//! `@media_owns(…)`. Owning a type a browser runs script in (`html`'s
//! `@script_capable` declaration: HTML, XHTML, SVG, XML) is the authority to
//! put script in front of a user, so a dependency that declares one is
//! refused unless the ROOT manifest grants that package that type:
//!
//! ```toml
//! [trust]
//! media = { "github.com/almide/svg" = ["image/svg+xml"] }
//! ```
//!
//! The package is named by its typed origin (ADR-0035; ADR-0032 amended by
//! #4868): `host[:port]/full/path` of its git source, every path segment,
//! the host always written (`github.com/almide/svg`); a path dependency by
//! its package name. A re-pointed dependency is not covered by an old grant,
//! and a grant names one repository, never a group of them. The key form
//! before #4868 (`owner/repo`, github.com implied) is refused with the new
//! form as its hint, so no grant silently widens or narrows.
//! A grant naming a type the package does not declare is refused too, and so
//! are two owners — the stdlib's, a dependency's, the application's own —
//! whose patterns can match one media type. The application's own types need
//! no grant (the developer wrote them). Every refusal is E118.

use std::path::{Path, PathBuf};

use crate::media_owners::{self as mo, Owned};
use crate::project::{FetchedDep, Project};

/// One package's media ownership.
#[derive(Debug, Clone)]
pub struct DepMedia {
    pub name: String,
    /// The source key a grant names it by.
    pub key: String,
    pub direct: bool,
    /// Each `@media_owns` pattern, with the file it is declared in.
    pub owns: Vec<(Owned, PathBuf)>,
    /// The media types the root manifest grants it.
    pub granted: Vec<String>,
}

/// The key `[trust] media` names a package by: the typed origin of its git
/// source — host, port and the full path (ADR-0035 identity; ADR-0032's Q17
/// amended by #4868) — or, without one, its package name. The path keeps its
/// case, as the origin does (#4892): on a host whose paths are case-sensitive,
/// `Alice/svg` and `alice/svg` are two repositories and get two keys.
pub fn grant_key(source: Option<&crate::git_source::GitSource>, name: &str) -> String {
    let Some(src) = source else { return name.to_ascii_lowercase() };
    let port = src.port.map(|p| format!(":{p}")).unwrap_or_default();
    let path: Vec<&str> = src.path.iter().map(|s| s.as_str()).collect();
    format!("{}{port}/{}", src.host.as_str().to_ascii_lowercase(), path.join("/"))
}

/// A `[trust] media` key as the manifest writes it, in [`grant_key`]'s form:
/// the host (before the first `/`) lowercased, the path as written; a key
/// with no `/` is a package name, lowercased.
pub fn manifest_grant_key(key: &str) -> String {
    match key.split_once('/') {
        Some((host, path)) => format!("{}/{path}", host.to_ascii_lowercase()),
        None => key.to_ascii_lowercase(),
    }
}

/// A `[trust] media` key in the form before #4868 — `owner/repo` with
/// github.com implied: two segments, the first no host (no `.` or `:`) — is
/// refused with the key it now is, since the same text no longer names the
/// same packages.
pub fn check_grant_key(key: &str) -> Result<(), String> {
    let segs: Vec<&str> = key.split('/').collect();
    if let [owner, repo] = segs.as_slice()
        && !owner.contains(['.', ':'])
        && !owner.is_empty()
        && !repo.is_empty()
    {
        return Err(format!(
            "[trust].media key \"{key}\" is the old `owner/repo` form; a grant now names the source's host and full path\n  hint: write \"github.com/{key}\" if the package comes from github.com"
        ));
    }
    Ok(())
}

/// The `[trust] media` key the application names itself by: its grant is the
/// media types `http.typed` may label bytes with beyond its table (E117). It
/// names no dependency, and no dependency may be named by it.
pub const SELF_KEY: &str = "self";

/// The media types the root manifest of `entry`'s project grants the
/// application itself (`"self"`); none without a project.
pub fn self_grants_for(entry: &str) -> Vec<String> {
    crate::project::permissions_project_for(entry)
        .map(|p| p.media_grants.into_iter().filter(|g| g.key == SELF_KEY).flat_map(|g| g.media).collect())
        .unwrap_or_default()
}

/// Every `@media_owns` pattern declared under `dir`, with its file.
pub fn owns_under(dir: &Path) -> Vec<(Owned, PathBuf)> {
    let mut files = Vec::new();
    crate::native_grant::almd_files(dir, &mut files);
    files.sort();
    let mut out = Vec::new();
    for file in files {
        let Ok(source) = std::fs::read_to_string(&file) else { continue };
        let tokens = crate::lexer::Lexer::tokenize(&source);
        let Ok(program) = crate::parser::Parser::new(tokens).parse() else { continue };
        out.extend(mo::owns_in(&program).into_iter().map(|o| (o, file.clone())));
    }
    out
}

/// Each fetched dependency's media ownership and grant.
pub fn survey(proj: &Project, fetched: &[FetchedDep]) -> Vec<DepMedia> {
    fetched
        .iter()
        .map(|fd| {
            let key = grant_key(fd.source.as_ref(), &fd.pkg_id.name);
            DepMedia {
                name: fd.pkg_id.name.clone(),
                granted: proj.media_grants.iter().filter(|g| g.key == key && g.key != SELF_KEY).flat_map(|g| g.media.clone()).collect(),
                key,
                direct: proj.dependencies.iter().any(|d| d.name == fd.pkg_id.name && crate::project::PackageOrigin::of(d) == fd.origin),
                owns: owns_under(&fd.source_dir),
            }
        })
        .collect()
}

/// The application's own owners (they need no grant, Q18).
fn root_owns(proj: &Project) -> Vec<(Owned, PathBuf)> {
    let src = proj.root.join("src");
    owns_under(if src.is_dir() { &src } else { &proj.root })
}

fn at(file: &Path, o: &Owned) -> String {
    format!("{}:{} `@media_owns(\"{}\")` on `{}`", file.display(), o.line, o.pattern, o.func)
}

/// The `media = { … }` line granting `extra` on top of the current grants.
fn grant_line(proj: &Project, extra: &[(String, String)]) -> String {
    let mut table: Vec<(String, Vec<String>)> = proj.media_grants.iter().map(|g| (g.key.clone(), g.media.clone())).collect();
    for (key, media) in extra {
        match table.iter_mut().find(|(k, _)| k == key) {
            Some((_, m)) if !m.contains(media) => m.push(media.clone()),
            Some(_) => {}
            None => table.push((key.clone(), vec![media.clone()])),
        }
    }
    let entries: Vec<String> = table
        .iter()
        .map(|(k, m)| format!("\"{k}\" = [{}]", m.iter().map(|t| format!("\"{t}\"")).collect::<Vec<_>>().join(", ")))
        .collect();
    format!("media = {{ {} }}", entries.join(", "))
}

/// The E118 refusal for `deps`, or `None` when every ownership is granted
/// and unique.
pub fn refusal(proj: &Project, deps: &[DepMedia]) -> Option<String> {
    let manifest = proj.root.join("almide.toml");
    let mut msg = String::new();
    let mut missing: Vec<(String, String)> = Vec::new();
    for d in deps {
        let how = if d.direct { "" } else { " (required by another dependency)" };
        for (o, file) in d.owns.iter().filter(|(o, _)| mo::is_script_capable(&o.pattern) && !d.granted.contains(&o.pattern)) {
            msg.push_str(&format!(
                "error[E118]: dependency `{}`{how} ({}) owns `{}`, which runs script in a browser, and {} does not grant it\n  declared at {}\n",
                d.name, d.key, o.pattern, manifest.display(), at(file, o)
            ));
            missing.push((d.key.clone(), o.pattern.clone()));
        }
        for t in d.granted.iter().filter(|t| !d.owns.iter().any(|(o, _)| &o.pattern == *t)) {
            msg.push_str(&format!(
                "error[E118]: {} grants `{}` the media type `{t}`, which it does not declare\n  hint: remove `\"{t}\"` from its [trust] media entry, or grant a pattern the package's `@media_owns` writes\n",
                manifest.display(), d.key
            ));
        }
    }
    for d in deps.iter().filter(|d| d.key == SELF_KEY) {
        msg.push_str(&format!(
            "error[E118]: dependency `{}` is named `{SELF_KEY}`, the key {} reserves for the application's own grant\n  hint: rename the dependency\n",
            d.name, manifest.display()
        ));
    }
    // The application's own grant: each type well formed, and none a type
    // only its owner may serve (script-capable, or a stdlib owner's), which
    // `http.typed` refuses whatever the grant says.
    for g in proj.media_grants.iter().filter(|g| g.key == SELF_KEY) {
        for t in &g.media {
            let why = if !mo::well_formed_pattern(t) {
                Some("is not a media type (`type/subtype`)".to_string())
            } else if mo::is_script_capable(t) {
                Some("runs script in a browser, so only a type that owns it may serve it".to_string())
            } else {
                mo::stdlib_owners().iter().find(|(p, _)| mo::overlap(t, p)).map(|(_, f)| format!("is owned by `{f}`"))
            };
            if let Some(why) = why {
                msg.push_str(&format!(
                    "error[E118]: {}:{} grants the application `{t}`, which {why}\n  hint: remove `\"{t}\"` from the `\"{SELF_KEY}\"` entry\n",
                    manifest.display(), g.line
                ));
            }
        }
    }
    for g in proj.media_grants.iter().filter(|g| g.key != SELF_KEY && !deps.iter().any(|d| d.key == g.key)) {
        msg.push_str(&format!(
            "error[E118]: {}:{} grants media types to `{}`, which is not a dependency of this project\n  hint: a grant names a dependency's source by host and full path (`github.com/owner/repo`); remove the entry or fix its source\n",
            manifest.display(), g.line, g.key
        ));
    }
    // One owner per media type (Q22): the stdlib's, each dependency's and the
    // application's own patterns may not overlap across owners.
    let mut owners: Vec<(String, String, String)> =
        mo::stdlib_owners().iter().map(|(p, f)| (format!("stdlib {}", f.split('.').next().unwrap_or(f)), p.clone(), format!("`{f}`"))).collect();
    for d in deps {
        owners.extend(d.owns.iter().map(|(o, file)| (format!("dependency `{}` ({})", d.name, d.key), o.pattern.clone(), at(file, o))));
    }
    owners.extend(root_owns(proj).iter().map(|(o, file)| ("this package".to_string(), o.pattern.clone(), at(file, o))));
    for (i, (a_owner, a, a_at)) in owners.iter().enumerate() {
        for (b_owner, b, b_at) in &owners[..i] {
            if a_owner != b_owner && mo::overlap(a, b) {
                msg.push_str(&format!(
                    "error[E118]: {a_owner} owns `{a}` and {b_owner} owns `{b}`, which can name one media type\n  declared at {a_at}\n  and at {b_at}\n  note: a media type has one owner, so `http.typed` refuses it in one name\n"
                ));
            }
        }
    }
    if msg.is_empty() {
        return None;
    }
    if !missing.is_empty() {
        msg.push_str("  note: owning a script-capable media type lets a package put script in front of a browser (ADR-0032 D7)\n");
        msg.push_str(&format!(
            "  hint: if you trust {} to build those types safely, grant it in {}:\n    [trust]\n    {}\n",
            missing.iter().map(|(k, _)| format!("`{k}`")).collect::<std::collections::BTreeSet<_>>().into_iter().collect::<Vec<_>>().join(", "),
            manifest.display(),
            grant_line(proj, &missing)
        ));
    }
    Some(msg.trim_end().to_string())
}

/// `almide deps`'s media-owners section (Q23): every owner, with what grants it.
pub fn owner_lines(proj: &Project, deps: &[DepMedia]) -> Vec<String> {
    let mut out = vec!["media owners:".to_string()];
    let mut stdlib: Vec<(String, Vec<String>)> = Vec::new();
    for (p, f) in mo::stdlib_owners() {
        let module = f.split('.').next().unwrap_or(f).to_string();
        match stdlib.iter_mut().find(|(m, _)| *m == module) {
            Some((_, ps)) => ps.push(p.clone()),
            None => stdlib.push((module, vec![p.clone()])),
        }
    }
    for (m, ps) in stdlib {
        out.push(format!("  {m} (stdlib, granted implicitly): {}", ps.join(", ")));
    }
    let manifest = proj.root.join("almide.toml");
    for d in deps.iter().filter(|d| !d.owns.is_empty()) {
        let line = proj.media_grants.iter().find(|g| g.key == d.key).map(|g| format!("{}:{}", manifest.display(), g.line));
        let types: Vec<String> = d
            .owns
            .iter()
            .map(|(o, _)| match (mo::is_script_capable(&o.pattern), d.granted.contains(&o.pattern)) {
                (false, _) => format!("{} (needs no grant)", o.pattern),
                (true, true) => format!("{} (granted at {})", o.pattern, line.clone().unwrap_or_default()),
                (true, false) => format!("{} (NOT granted)", o.pattern),
            })
            .collect();
        out.push(format!("  {} ({}): {}", d.name, d.key, types.join(", ")));
    }
    let own = root_owns(proj);
    if !own.is_empty() {
        out.push(format!("  this package (needs no grant): {}", own.iter().map(|(o, _)| o.pattern.clone()).collect::<Vec<_>>().join(", ")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::grant_key;

    #[test]
    fn a_grant_names_the_source() {
        let src = |s: &str| crate::git_source::GitSource::parse(s).unwrap();
        assert_eq!(grant_key(Some(&src("https://github.com/almide/svg")), "svg"), "github.com/almide/svg");
        assert_eq!(grant_key(Some(&src("https://GitHub.com/almide/svg.git")), "svg"), "github.com/almide/svg");
        assert_eq!(grant_key(Some(&src("https://gitlab.com/alice/svg")), "svg"), "gitlab.com/alice/svg");
        assert_eq!(grant_key(None, "local_svg"), "local_svg");
    }

    /// #4892: the path keeps its case on both sides — a grant written for
    /// `alice/svg` is not the key of a source at `ALICE/svg`, and the manifest
    /// side lowercases the host only.
    #[test]
    fn a_grant_key_keeps_the_paths_case() {
        let src = |s: &str| crate::git_source::GitSource::parse(s).unwrap();
        let lower = grant_key(Some(&src("https://gitlab.example/alice/svg")), "svg");
        let upper = grant_key(Some(&src("https://gitlab.example/ALICE/svg")), "svg");
        assert_ne!(lower, upper);
        assert_eq!(upper, "gitlab.example/ALICE/svg");
        assert_eq!(super::manifest_grant_key("GitLab.Example/alice/svg"), lower);
        assert_ne!(super::manifest_grant_key("gitlab.example/alice/svg"), upper);
        assert_eq!(super::manifest_grant_key("Local_SVG"), grant_key(None, "local_svg"));
    }

    /// #4868: two repositories under one group get two keys, the port is
    /// part of the key, and the group's key is no repository's.
    #[test]
    fn a_grant_names_one_repository_by_host_port_and_full_path() {
        let src = |s: &str| crate::git_source::GitSource::parse(s).unwrap();
        let a = grant_key(Some(&src("https://gitlab.example/g/s/a")), "a");
        let b = grant_key(Some(&src("https://gitlab.example/g/s/b")), "b");
        assert_eq!(a, "gitlab.example/g/s/a");
        assert_ne!(a, b);
        assert_ne!(a, "gitlab.example/g/s");
        assert_eq!(grant_key(Some(&src("https://gitlab.example:8443/g/s/a")), "a"), "gitlab.example:8443/g/s/a");
    }

    /// The old `owner/repo` key is refused with its new form; host-led and
    /// package-name keys stand.
    #[test]
    fn the_old_owner_repo_key_is_refused_with_its_new_form() {
        let e = super::check_grant_key("almide/svg").unwrap_err();
        assert!(e.contains("github.com/almide/svg"), "{e}");
        assert!(super::check_grant_key("github.com/almide/svg").is_ok());
        assert!(super::check_grant_key("gitlab.example/g/s/a").is_ok());
        assert!(super::check_grant_key("local_svg").is_ok());
    }
}
