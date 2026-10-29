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
//! media = { "almide/svg" = ["image/svg+xml"] }
//! ```
//!
//! The package is named by its source (`owner/repo` on github.com,
//! `host/owner/repo` elsewhere, the `[dependencies]` name for a path
//! dependency), so a re-pointed dependency is not covered by an old grant.
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

/// The key `[trust] media` names a package by (ADR-0032 Q17).
pub fn grant_key(source: &str, name: &str) -> String {
    let rest = match source.split_once("://") {
        Some((scheme, rest)) if !scheme.eq_ignore_ascii_case("file") => rest,
        _ => return name.to_ascii_lowercase(),
    };
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
    let segments: Vec<&str> = path.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
    let [owner, repo, ..] = segments.as_slice() else { return name.to_ascii_lowercase() };
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    let tail = format!("{}/{}", owner.to_ascii_lowercase(), repo.to_ascii_lowercase());
    if host == "github.com" { tail } else { format!("{host}/{tail}") }
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
            let key = grant_key(&fd.source, &fd.pkg_id.name);
            DepMedia {
                name: fd.pkg_id.name.clone(),
                granted: proj.media_grants.iter().filter(|g| g.key == key).flat_map(|g| g.media.clone()).collect(),
                key,
                direct: proj.dependencies.iter().any(|d| d.name == fd.pkg_id.name),
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
    for g in proj.media_grants.iter().filter(|g| !deps.iter().any(|d| d.key == g.key)) {
        msg.push_str(&format!(
            "error[E118]: {}:{} grants media types to `{}`, which is not a dependency of this project\n  hint: a grant names a dependency's source (`owner/repo`); remove the entry or fix its source\n",
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
        assert_eq!(grant_key("https://github.com/almide/svg", "svg"), "almide/svg");
        assert_eq!(grant_key("https://github.com/Almide/SVG.git", "svg"), "almide/svg");
        assert_eq!(grant_key("https://gitlab.com/alice/svg", "svg"), "gitlab.com/alice/svg");
        assert_eq!(grant_key("", "local_svg"), "local_svg");
        assert_eq!(grant_key("file:///tmp/svg", "svg"), "svg");
    }
}
