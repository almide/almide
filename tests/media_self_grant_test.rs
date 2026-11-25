//! E117's closed table (ADR-0032 D7, owner ruling 2026-11-24): `http.typed`
//! serves the standard non-script media types its stdlib declaration lists;
//! any other type is refused unless the application grants it to itself in
//! `almide.toml` (`[trust] media = { "self" = [...] }`). A `"self"` entry
//! naming a type only its owner may serve is E118. No dependencies, no
//! network: the grant is the root manifest's own.

#[path = "support/novel.rs"]
mod novel;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn project(tag: &str, manifest: &str, media: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-media-self-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).expect("mkdir");
    std::fs::write(dir.join("almide.toml"), format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n{manifest}")).expect("write");
    std::fs::write(
        dir.join("src/main.almd"),
        format!(
            "import http\n\nfn main() -> Unit = {{\n  let r = http.response(200, http.typed(\"{media}\", bytes.from_string(\"x\")))\n  println(http.header(r, \"Content-Type\") ?? \"none\")\n}}\n"
        ),
    )
    .expect("write");
    dir
}

fn almide(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_almide")).current_dir(dir).args(args).output().expect("spawn almide")
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

#[test]
fn a_listed_type_needs_no_grant() {
    let dir = project("listed", "", "image/png");
    let out = almide(&dir, &["run", "src/main.almd"]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "image/png");
}

#[test]
fn novel_an_unlisted_type_is_refused_until_the_application_grants_it() {
    // A FRESH media type no list in the compiler can have named: neither in
    // the table nor script-capable by html's declaration. Before the closed
    // table it was served by default.
    let fresh = format!("application/vnd.{}+json", novel::ident("novel"));
    let other = format!("application/{}", novel::ident("other"));
    let dir = project("refused", "", &fresh);
    let out = almide(&dir, &["check", "src/main.almd"]);
    assert!(!out.status.success(), "an unlisted type was admitted:\n{}", text(&out));
    assert!(text(&out).contains("E117"), "{}", text(&out));
    assert!(text(&out).contains("\"self\""), "the refusal names the grant: {}", text(&out));

    let grant = format!("\n[trust]\nmedia = {{ \"self\" = [\"{fresh}\"] }}\n");
    let dir = project("granted", &grant, &fresh);
    for route in [&["check", "src/main.almd"][..], &["run", "src/main.almd"][..]] {
        let out = almide(&dir, route);
        assert!(out.status.success(), "{route:?} refused a granted type:\n{}", text(&out));
    }
    let out = almide(&dir, &["run", "src/main.almd"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), fresh);

    // A grant names exactly its types: another fresh one stays refused.
    let dir = project("other", &grant, &other);
    let out = almide(&dir, &["check", "src/main.almd"]);
    assert!(!out.status.success() && text(&out).contains("E117"), "{}", text(&out));
}

#[test]
fn a_self_grant_cannot_name_a_type_only_its_owner_serves() {
    for t in ["text/html", "application/xhtml+xml", "not-a-type"] {
        let dir = project("owner", &format!("\n[trust]\nmedia = {{ \"self\" = [\"{t}\"] }}\n"), "text/plain");
        let out = almide(&dir, &["check", "src/main.almd"]);
        assert!(!out.status.success(), "a self grant of {t} was accepted:\n{}", text(&out));
        assert!(text(&out).contains("E118") && text(&out).contains(t), "{t}: {}", text(&out));
    }
}

#[test]
fn a_script_type_is_admitted_only_by_its_exact_self_grant() {
    // Owner ruling 2026-11-25: a script-capable type is served when, and only
    // when, the application names that exact type; without the grant, or
    // with another script type granted, it stays E117.
    for t in ["text/javascript", "image/svg+xml", "application/rss+xml"] {
        let dir = project("script-none", "", t);
        let out = almide(&dir, &["check", "src/main.almd"]);
        assert!(!out.status.success() && text(&out).contains("E117"), "{t} admitted without a grant:\n{}", text(&out));
        assert!(text(&out).contains(&format!("\"self\" = [\"{t}\"]")), "the refusal names the exact grant: {}", text(&out));

        let dir = project("script-other", "\n[trust]\nmedia = { \"self\" = [\"text/ecmascript\"] }\n", t);
        let out = almide(&dir, &["check", "src/main.almd"]);
        assert!(!out.status.success() && text(&out).contains("E117"), "{t} admitted by another type's grant:\n{}", text(&out));

        let dir = project("script-exact", &format!("\n[trust]\nmedia = {{ \"self\" = [\"{t}\"] }}\n"), t);
        let out = almide(&dir, &["run", "src/main.almd"]);
        assert!(out.status.success(), "{t} refused under its exact grant:\n{}", text(&out));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), t);
    }
}

#[test]
fn a_self_grant_pattern_over_a_script_type_is_refused() {
    for t in ["*/*+xml", "text/*", "application/*"] {
        let dir = project("script-pattern", &format!("\n[trust]\nmedia = {{ \"self\" = [\"{t}\"] }}\n"), "text/plain");
        let out = almide(&dir, &["check", "src/main.almd"]);
        assert!(!out.status.success(), "a self grant of the pattern {t} was accepted:\n{}", text(&out));
        assert!(text(&out).contains("E118") && text(&out).contains(t), "{t}: {}", text(&out));
    }
}
