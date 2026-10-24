use std::process::Command;
use crate::{err, err_no_nl};

const REPO: &str = "almide/almide";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(target_os = "macos")]
const OS: &str = "macos";
#[cfg(target_os = "linux")]
const OS: &str = "linux";
#[cfg(target_os = "windows")]
const OS: &str = "windows";

#[cfg(target_arch = "x86_64")]
const ARCH: &str = "x86_64";
#[cfg(target_arch = "aarch64")]
const ARCH: &str = "aarch64";

pub fn cmd_self_update(version: Option<&str>, skip_attestation: bool) {
    let target_version = match version {
        Some(v) => {
            let v = v.strip_prefix('v').unwrap_or(v);
            v.to_string()
        }
        None => match fetch_latest_version() {
            Ok(v) => v,
            Err(e) => {
                err(&format!("error: failed to check latest version: {}", e));
                std::process::exit(1);
            }
        },
    };

    if target_version == CURRENT_VERSION {
        err(&format!("almide {} is already up to date.", CURRENT_VERSION));
        return;
    }

    err(&format!(
        "Updating almide {} → {}",
        CURRENT_VERSION, target_version
    ));

    let tag = format!("v{}", target_version);
    let archive = archive_name();
    let base_url = format!(
        "https://github.com/{}/releases/download/{}",
        REPO, tag
    );

    let tmp = tempdir();

    // Download archive
    let archive_path = format!("{}/{}", tmp, archive);
    err(&format!("Downloading {}...", archive));
    curl_download(&format!("{}/{}", base_url, archive), &archive_path);

    // Download checksums
    let checksum_path = format!("{}/almide-checksums.sha256", tmp);
    curl_download(
        &format!("{}/almide-checksums.sha256", base_url),
        &checksum_path,
    );

    // Verify checksum
    err(&format!("Verifying checksum..."));
    verify_checksum(&archive_path, &checksum_path, &archive);

    // Verify origin: the checksum came from the same place as the archive,
    // so it says nothing about who built it (#4127).
    if skip_attestation {
        err("warning: --skip-attestation: installing without verifying the release attestation");
    } else {
        err("Verifying the release attestation...");
        if let Err(e) = verify_attestation("gh", &archive_path) {
            err(&format!("error: {e}"));
            err("       install the GitHub CLI (`gh`) to verify it, or pass --skip-attestation to install on the checksum alone");
            std::process::exit(1);
        }
    }

    // Extract
    let extract_dir = format!("{}/extracted", tmp);
    std::fs::create_dir_all(&extract_dir).unwrap_or_else(|e| {
        err(&format!("error: failed to create extraction dir: {}", e));
        std::process::exit(1);
    });

    #[cfg(not(target_os = "windows"))]
    {
        let status = Command::new("tar")
            .args(["xzf", &archive_path, "-C", &extract_dir])
            .status()
            .unwrap_or_else(|e| {
                err(&format!("error: failed to extract archive: {}", e));
                std::process::exit(1);
            });
        if !status.success() {
            err(&format!("error: tar extraction failed"));
            std::process::exit(1);
        }
    }

    #[cfg(target_os = "windows")]
    {
        let status = Command::new("powershell")
            .args([
                "-Command",
                &format!(
                    "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                    archive_path, extract_dir
                ),
            ])
            .status()
            .unwrap_or_else(|e| {
                err(&format!("error: failed to extract archive: {}", e));
                std::process::exit(1);
            });
        if !status.success() {
            err(&format!("error: extraction failed"));
            std::process::exit(1);
        }
    }

    // Find the extracted binary
    let binary_name = if cfg!(target_os = "windows") {
        "almide.exe"
    } else {
        "almide"
    };
    let stem = archive_stem();
    let extracted_binary = format!("{}/{}/{}", extract_dir, stem, binary_name);

    if !std::path::Path::new(&extracted_binary).exists() {
        err(&format!(
            "error: expected binary not found at {}",
            extracted_binary
        ));
        std::process::exit(1);
    }

    // Replace current binary
    let current_exe = std::env::current_exe().unwrap_or_else(|e| {
        err(&format!("error: cannot determine current executable path: {}", e));
        std::process::exit(1);
    });

    replace_binary(&extracted_binary, &current_exe);
    update_verifier(&format!("{}/{}", extract_dir, stem), &current_exe);

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp);

    err(&format!("Updated to almide {}.", target_version));

    // Verify
    let output = Command::new(current_exe).arg("--version").output();
    if let Ok(out) = output {
        err_no_nl(&format!("{}", String::from_utf8_lossy(&out.stdout)));
    }
}

fn fetch_latest_version() -> Result<String, String> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        REPO
    );
    let body = https_get(&url, &[("Accept", "application/vnd.github+json")])
        .map_err(|e| format!("GitHub API request failed: {e}"))?;

    let json: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| format!("failed to parse GitHub API response: {}", e))?;

    let tag = json
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or("no tag_name in release")?;

    Ok(tag.strip_prefix('v').unwrap_or(tag).to_string())
}

fn archive_name() -> String {
    if cfg!(target_os = "windows") {
        format!("almide-{}-{}.zip", OS, ARCH)
    } else {
        format!("almide-{}-{}.tar.gz", OS, ARCH)
    }
}

fn archive_stem() -> String {
    format!("almide-{}-{}", OS, ARCH)
}

/// A fresh private staging directory (#4127). The name is not predictable:
/// it is hashed with a per-process random key (std's `RandomState`). The
/// directory is created exclusively, with mode 0700 on unix, so a directory
/// another user created first is never reused. The old
/// `temp_dir()/almide-update-<pid>` with `create_dir_all` accepted any
/// existing directory, including one someone else owned.
fn tempdir() -> String {
    use std::hash::{BuildHasher, Hasher};
    let base = std::env::temp_dir();
    for _ in 0..16 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        );
        h.write_u32(std::process::id());
        let dir = base.join(format!("almide-update-{:016x}", h.finish()));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(&dir) {
            Ok(()) => return dir.to_string_lossy().to_string(),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                err(&format!("Failed to create {}: {}", dir.display(), e));
                std::process::exit(1);
            }
        }
    }
    err("error: could not create a private staging directory for the update");
    std::process::exit(1);
}

/// The hash the checksum file lists for exactly `archive_name`. It matches
/// the file-name field (`<hash>  <name>` or `<hash> *<name>`), never a
/// substring: an asset whose name only CONTAINS the archive name, such as
/// a `.sig` or `.sbom` for it, cannot supply the hash (#4127).
fn expected_checksum(checksums: &str, archive_name: &str) -> Option<String> {
    checksums.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let hash = fields.next()?;
        let name = fields.next()?;
        let name = name.strip_prefix('*').unwrap_or(name);
        (name == archive_name && fields.next().is_none()).then(|| hash.to_ascii_lowercase())
    })
}

fn curl_download(url: &str, dest: &str) {
    // In-process HTTPS (#4181): no `curl` from PATH, no `.curlrc`, and a
    // redirect may only lead to another https URL.
    let body = https_get(url, &[]).unwrap_or_else(|e| {
        err(&format!("error: download failed: {}: {}", url, e));
        err(&format!("       check that the version exists: https://github.com/{}/releases", REPO));
        std::process::exit(1);
    });
    if let Err(e) = std::fs::write(dest, &body) {
        err(&format!("error: cannot write {}: {}", dest, e));
        std::process::exit(1);
    }
}

/// The most redirects a download follows (GitHub's release assets take one).
const MAX_REDIRECTS: usize = 5;

/// GET `url` over the in-tree HTTPS client (rustls, the one the stdlib
/// `http` module uses), following redirects under [`follow_https`]'s rules.
fn https_get(url: &str, headers: &[(&str, &str)]) -> Result<Vec<u8>, String> {
    let mut hs: Vec<(String, String)> = vec![("User-Agent".into(), format!("almide-self-update/{CURRENT_VERSION}"))];
    hs.extend(headers.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    follow_https(url, &mut |u: &str| almide_rt_core::http_client_core::request_response_bytes("GET", u, "", &hs))
}

/// One fetch: `(status, headers, body)`.
type Fetched = (i64, Vec<(String, String)>, Vec<u8>);

/// The redirect policy of a self-update download (#4181): every URL —
/// the first and each `Location` — must be `https://`; at most
/// [`MAX_REDIRECTS`] hops; a 2xx is the answer and anything else an error.
fn follow_https(url: &str, fetch: &mut dyn FnMut(&str) -> Result<Fetched, String>) -> Result<Vec<u8>, String> {
    let mut url = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        if !url.starts_with("https://") {
            return Err(format!("refusing a non-https URL: {url}"));
        }
        let (status, headers, body) = fetch(&url)?;
        match status {
            200..=299 => return Ok(body),
            301 | 302 | 303 | 307 | 308 => {
                let loc = headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case("location"))
                    .map(|(_, v)| v.trim().to_string())
                    .ok_or_else(|| format!("HTTP {status} without a Location from {url}"))?;
                url = if loc.starts_with('/') {
                    // An absolute path on the same origin.
                    let origin_end = url["https://".len()..].find('/').map_or(url.len(), |i| i + "https://".len());
                    format!("{}{}", &url[..origin_end], loc)
                } else {
                    loc
                };
            }
            other => return Err(format!("HTTP {other} from {url}")),
        }
    }
    Err(format!("more than {MAX_REDIRECTS} redirects"))
}

fn verify_checksum(archive_path: &str, checksum_path: &str, archive_name: &str) {
    let checksums = std::fs::read_to_string(checksum_path).unwrap_or_else(|e| {
        err(&format!("error: failed to read checksums: {}", e));
        std::process::exit(1);
    });

    let expected = expected_checksum(&checksums, archive_name).unwrap_or_else(|| {
        err(&format!("error: checksum not found for {}", archive_name));
        std::process::exit(1);
    });

    // Hashed in-process (#4127): a `sha256sum` / `shasum` found on PATH is
    // whatever the environment put there, and a missing one used to mean an
    // install with no check at all.
    let bytes = std::fs::read(archive_path).unwrap_or_else(|e| {
        err(&format!("error: failed to read the downloaded archive: {}", e));
        std::process::exit(1);
    });
    let actual = sha256_hex(&bytes);

    if expected != actual.to_ascii_lowercase() {
        err(&format!("error: checksum mismatch"));
        err(&format!("       expected: {}", expected));
        err(&format!("       got:      {}", actual));
        std::process::exit(1);
    }
}

/// The lowercase hex SHA-256 of `bytes`.
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Verify the archive's Sigstore attestation (#1534 attests every release
/// asset) with `gh attestation verify <archive> -R almide/almide` (#4127).
/// A missing `gh` is an error, not a pass.
fn verify_attestation(gh: &str, archive_path: &str) -> Result<(), String> {
    let out = Command::new(gh)
        .args(["attestation", "verify", archive_path, "-R", REPO])
        .output()
        .map_err(|e| format!("cannot verify the release attestation: `{gh}` could not be run ({e})"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "the release attestation did not verify:\n{}",
            String::from_utf8_lossy(&out.stderr).trim_end()
        ))
    }
}

/// #2152: the archive's independent verifier goes next to the updated almide —
/// `almide verify` execs it from there and has no linked fallback. Archives
/// older than the verifier do not carry it; that is a note, not a failure.
fn update_verifier(extracted_dir: &str, current_exe: &std::path::Path) {
    let name = format!("almide-verify{}", std::env::consts::EXE_SUFFIX);
    let new = std::path::Path::new(extracted_dir).join(&name);
    let Some(dir) = current_exe.parent() else { return };
    if !new.exists() {
        err("note: this release predates almide-verify; `almide verify` will report it missing");
        return;
    }
    let dest = dir.join(&name);
    if dest.exists() {
        replace_binary(&new.to_string_lossy(), &dest);
    } else if let Err(e) = std::fs::copy(&new, &dest) {
        err(&format!("warning: could not install {}: {}", dest.display(), e));
    }
}

fn replace_binary(new_path: &str, current_exe: &std::path::Path) {
    #[cfg(not(target_os = "windows"))]
    {
        // Unix: copy new binary over the current one (inodes allow this)
        // Use a temp file + rename for atomicity
        let dir = current_exe.parent().unwrap_or_else(|| {
            err("error: could not determine directory of the running executable");
            std::process::exit(1);
        });
        let tmp_path = dir.join(".almide-update.tmp");

        std::fs::copy(new_path, &tmp_path).unwrap_or_else(|e| {
            err(&format!("error: failed to copy new binary: {}", e));
            std::process::exit(1);
        });

        // Preserve permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755));
        }

        std::fs::rename(&tmp_path, current_exe).unwrap_or_else(|e| {
            // rename fails across filesystems; fall back to copy
            if std::fs::copy(&tmp_path, current_exe).is_err() {
                err(&format!("error: failed to replace binary: {}", e));
                err(&format!("       try: sudo cp {} {}", new_path, current_exe.display()));
                let _ = std::fs::remove_file(&tmp_path);
                std::process::exit(1);
            }
            let _ = std::fs::remove_file(&tmp_path);
        });
    }

    #[cfg(target_os = "windows")]
    {
        // Windows: rename current exe to .old, copy new one in place
        let old_path = current_exe.with_extension("old.exe");
        let _ = std::fs::remove_file(&old_path);
        std::fs::rename(current_exe, &old_path).unwrap_or_else(|e| {
            err(&format!("error: failed to rename current binary: {}", e));
            std::process::exit(1);
        });
        std::fs::copy(new_path, current_exe).unwrap_or_else(|e| {
            // Rollback
            let _ = std::fs::rename(&old_path, current_exe);
            err(&format!("error: failed to copy new binary: {}", e));
            std::process::exit(1);
        });
        let _ = std::fs::remove_file(&old_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #4127: the checksum line is matched on the exact file-name field.
    /// A substring match let a `.sig` / `.sbom` line for the same archive
    /// supply the hash.
    #[test]
    fn expected_checksum_matches_the_exact_name_only() {
        let sums = "\
aaaa  almide-macos-aarch64.tar.gz.sig
BBBB *almide-macos-aarch64.tar.gz
cccc  almide-linux-x86_64.tar.gz
dddd  almide-macos-aarch64.tar.gz extra-field
";
        assert_eq!(expected_checksum(sums, "almide-macos-aarch64.tar.gz").as_deref(), Some("bbbb"));
        assert_eq!(expected_checksum(sums, "almide-linux-x86_64.tar.gz").as_deref(), Some("cccc"));
        assert_eq!(expected_checksum(sums, "almide-macos.tar.gz"), None);
        assert_eq!(expected_checksum("aaaa  almide-x.tar.gz.sig\n", "almide-x.tar.gz"), None);
    }

    /// #4127: the staging directory is fresh, private and unpredictable.
    #[test]
    fn tempdir_is_fresh_and_private() {
        let a = tempdir();
        let b = tempdir();
        assert_ne!(a, b, "two staging dirs must not share a name");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&a).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "staging dir mode {mode:o}");
        }
        assert!(!a.ends_with(&format!("almide-update-{}", std::process::id())), "name must not be the pid");
        let _ = std::fs::remove_dir(&a);
        let _ = std::fs::remove_dir(&b);
    }

    /// #4127: the archive is hashed in-process, not by a tool found on PATH.
    #[test]
    fn sha256_is_computed_in_process() {
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    /// #4127: a missing or failing verifier refuses the install; only a
    /// verifier that succeeds lets it through.
    #[cfg(unix)]
    #[test]
    fn the_attestation_check_fails_closed() {
        let missing = verify_attestation("/nonexistent/almide-test-gh", "x.tar.gz").unwrap_err();
        assert!(missing.contains("could not be run"), "{missing}");
        let failing = verify_attestation("false", "x.tar.gz").unwrap_err();
        assert!(failing.contains("did not verify"), "{failing}");
        assert!(verify_attestation("true", "x.tar.gz").is_ok());
    }

    fn redirect(to: &str) -> Result<Fetched, String> {
        Ok((302, vec![("Location".into(), to.into())], Vec::new()))
    }

    /// #4181: the download follows redirects only to https, and a bounded number.
    #[test]
    fn downloads_follow_only_https_redirects() {
        // https → https → 200: followed.
        let mut seen = Vec::new();
        let got = follow_https("https://a.example/x", &mut |u: &str| {
            seen.push(u.to_string());
            match u {
                "https://a.example/x" => redirect("https://b.example/y"),
                "https://b.example/y" => redirect("/z"),
                _ => Ok((200, vec![], b"ok".to_vec())),
            }
        });
        assert_eq!(got.unwrap(), b"ok");
        assert_eq!(seen, ["https://a.example/x", "https://b.example/y", "https://b.example/z"]);
        // A redirect to plain http is refused before it is fetched.
        let mut fetched_http = false;
        let e = follow_https("https://a.example/x", &mut |u: &str| {
            if u.starts_with("http://") {
                fetched_http = true;
            }
            redirect("http://evil.example/x")
        })
        .unwrap_err();
        assert!(e.contains("non-https"), "{e}");
        assert!(!fetched_http);
        // The first URL is held to the same rule.
        assert!(follow_https("http://a.example/x", &mut |_: &str| Ok((200, vec![], vec![]))).is_err());
        // A redirect loop ends.
        let e = follow_https("https://a.example/x", &mut |_: &str| redirect("https://a.example/x")).unwrap_err();
        assert!(e.contains("redirects"), "{e}");
        // A non-2xx is an error, not a file.
        assert!(follow_https("https://a.example/x", &mut |_: &str| Ok((404, vec![], b"nope".to_vec()))).is_err());
    }
}
