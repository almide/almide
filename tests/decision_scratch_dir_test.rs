//! #4853: the files a tool hands another process (the certificate gate's
//! artifact and bundle) live in a directory CREATED for them, mode 0700, never
//! at a name that already exists. A fresh stem and seed name a candidate the
//! test pre-plants as a symlink to a victim directory: the helper must skip it,
//! not enter or follow it. Row `verify.gate-build-scratch-files`.
#![cfg(unix)]

#[path = "support/novel.rs"]
mod novel;

#[test]
fn novel_a_planted_scratch_name_is_skipped_never_followed() {
    use std::os::unix::fs::PermissionsExt;
    let base = tempfile::TempDir::new().unwrap();
    let victim = base.path().join(novel::path_segment());
    std::fs::create_dir(&victim).unwrap();
    let stem = novel::path_segment();
    let seed = u128::from(novel::fs_op() as u32);
    let planted = base.path().join(format!("{stem}-{}-{seed}-0", std::process::id()));
    std::os::unix::fs::symlink(&victim, &planted).unwrap();

    let dir = almide_base::private_dir::private_dir_in(base.path(), &stem, seed).expect("a private dir");
    assert_ne!(dir, planted, "the planted name was reused");
    assert!(!std::fs::symlink_metadata(&dir).unwrap().file_type().is_symlink(), "the scratch dir is a link");
    assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700, "the scratch dir is not private");
    std::fs::write(dir.join("artifact.wasm"), b"x").unwrap();
    assert_eq!(std::fs::read_dir(&victim).unwrap().count(), 0, "a write went through the planted link");
}

/// Task-6 (2): the route itself. `almide build --target wasm` with a stub
/// `almide-verify` beside a copy of the compiler (the first place
/// `locate_verifier` looks): the stub records where it was run and what it
/// was handed. The hand-off directory is a fresh, non-link, 0700 directory
/// under TMPDIR holding the artifact and the bundle; a planted symlink in
/// TMPDIR (to a victim directory) is never written through; nothing is left
/// behind. The control half: the stub's own verdict is the build's — a stub
/// exiting 1 fails the build, so the stub was run.
#[test]
fn novel_the_build_route_hands_the_verifier_a_fresh_private_directory() {
    use std::os::unix::fs::PermissionsExt;
    let base = tempfile::TempDir::new().unwrap();
    let (bin, tmp, victim, proj) =
        (base.path().join("bin"), base.path().join("tmp"), base.path().join("victim"), base.path().join("proj"));
    for d in [&bin, &tmp, &victim, &proj] {
        std::fs::create_dir_all(d).unwrap();
    }
    let planted = tmp.join(format!("almide-build-{}", novel::path_segment()));
    std::os::unix::fs::symlink(&victim, &planted).unwrap();
    let almide = bin.join("almide");
    std::fs::copy(env!("CARGO_BIN_EXE_almide"), &almide).unwrap();
    let log = base.path().join("stub.log");
    let stub = bin.join("almide-verify");
    let write_stub = |code: i32| {
        std::fs::write(
            &stub,
            format!(
                "#!/bin/sh\n{{ pwd -P; [ -L \"$(pwd)\" ] && echo LINK; ls -A; stat -c %a . 2>/dev/null || stat -f %Lp .; echo \"$1 $2\"; }} > '{}'\nexit {code}\n",
                log.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    };
    let main = format!("fn {}() -> Int = 1\n\neffect fn main() -> Unit = println(\"hi\")\n", novel::ident("f"));
    std::fs::write(proj.join("main.almd"), main).unwrap();
    let build = || {
        std::process::Command::new(&almide)
            .current_dir(&proj)
            .env("TMPDIR", &tmp)
            .args(["build", "main.almd", "--target", "wasm", "-o", "out.wasm"])
            .output()
            .unwrap()
    };
    write_stub(0);
    let o = build();
    let text = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    assert!(o.status.success(), "{text}");
    let seen = std::fs::read_to_string(&log).unwrap_or_else(|_| panic!("the stub verifier was not run:\n{text}"));
    let lines: Vec<&str> = seen.lines().collect();
    let tmp_real = tmp.canonicalize().unwrap();
    let dir = std::path::Path::new(lines[0]);
    assert_eq!(dir.parent(), Some(tmp_real.as_path()), "the hand-off dir is not directly under TMPDIR:\n{seen}");
    assert!(dir.file_name().unwrap().to_string_lossy().starts_with("almide-build-"), "{seen}");
    assert_ne!(dir, planted.as_path(), "the planted name was used:\n{seen}");
    assert!(!seen.contains("LINK"), "the hand-off dir is a link:\n{seen}");
    assert!(lines.contains(&"artifact.wasm") && lines.contains(&"certificate.bundle"), "{seen}");
    assert!(lines.contains(&"700"), "the hand-off dir is not mode 0700:\n{seen}");
    assert!(lines.contains(&"bundle certificate.bundle"), "{seen}");
    assert_eq!(std::fs::read_dir(&victim).unwrap().count(), 0, "a write went through the planted link");
    let left: Vec<_> = std::fs::read_dir(&tmp).unwrap().flatten().map(|e| e.path()).filter(|p| p != &planted).collect();
    assert!(left.is_empty(), "the hand-off dir was left behind: {left:?}");
    // Control: the stub's verdict decides the build.
    write_stub(1);
    let o = build();
    assert!(!o.status.success(), "a rejecting verifier did not fail the build");
}
