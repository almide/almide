use std::process::Command;

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

pub fn cmd_self_update(version: Option<&str>) {
    let target_version = match version {
        Some(v) => {
            let v = v.strip_prefix('v').unwrap_or(v);
            v.to_string()
        }
        None => match fetch_latest_version() {
            Ok(v) => v,
            Err(e) => {
                eprintln!("error: failed to check latest version: {}", e);
                std::process::exit(1);
            }
        },
    };

    if target_version == CURRENT_VERSION {
        eprintln!("almide {} is already up to date.", CURRENT_VERSION);
        return;
    }

    eprintln!(
        "Updating almide {} → {}",
        CURRENT_VERSION, target_version
    );

    let tag = format!("v{}", target_version);
    let archive = archive_name();
    let base_url = format!(
        "https://github.com/{}/releases/download/{}",
        REPO, tag
    );

    let tmp = tempdir();

    // Download archive
    let archive_path = format!("{}/{}", tmp, archive);
    eprintln!("Downloading {}...", archive);
    curl_download(&format!("{}/{}", base_url, archive), &archive_path);

    // Download checksums
    let checksum_path = format!("{}/almide-checksums.sha256", tmp);
    curl_download(
        &format!("{}/almide-checksums.sha256", base_url),
        &checksum_path,
    );

    // Verify checksum
    eprintln!("Verifying checksum...");
    verify_checksum(&archive_path, &checksum_path, &archive);

    // Extract
    let extract_dir = format!("{}/extracted", tmp);
    std::fs::create_dir_all(&extract_dir).unwrap();

    #[cfg(not(target_os = "windows"))]
    {
        let status = Command::new("tar")
            .args(["xzf", &archive_path, "-C", &extract_dir])
            .status()
            .unwrap_or_else(|e| {
                eprintln!("error: failed to extract archive: {}", e);
                std::process::exit(1);
            });
        if !status.success() {
            eprintln!("error: tar extraction failed");
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
                eprintln!("error: failed to extract archive: {}", e);
                std::process::exit(1);
            });
        if !status.success() {
            eprintln!("error: extraction failed");
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
        eprintln!(
            "error: expected binary not found at {}",
            extracted_binary
        );
        std::process::exit(1);
    }

    // Replace current binary
    let current_exe = std::env::current_exe().unwrap_or_else(|e| {
        eprintln!("error: cannot determine current executable path: {}", e);
        std::process::exit(1);
    });

    replace_binary(&extracted_binary, &current_exe);

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp);

    eprintln!("Updated to almide {}.", target_version);

    // Verify
    let output = Command::new(current_exe).arg("--version").output();
    if let Ok(out) = output {
        eprint!("{}", String::from_utf8_lossy(&out.stdout));
    }
}

fn fetch_latest_version() -> Result<String, String> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        REPO
    );
    let output = Command::new("curl")
        .args(["-fsSL", "-H", "Accept: application/vnd.github+json", &url])
        .output()
        .map_err(|e| format!("curl not found: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "GitHub API request failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
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

fn tempdir() -> String {
    let dir = std::env::temp_dir().join(format!("almide-update-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.to_string_lossy().to_string()
}

fn curl_download(url: &str, dest: &str) {
    let status = Command::new("curl")
        .args(["-fsSL", "-o", dest, url])
        .status()
        .unwrap_or_else(|e| {
            eprintln!("error: curl not found: {}", e);
            eprintln!("       curl is required for self-update");
            std::process::exit(1);
        });
    if !status.success() {
        eprintln!("error: download failed: {}", url);
        eprintln!("       check that the version exists: https://github.com/{}/releases", REPO);
        std::process::exit(1);
    }
}

fn verify_checksum(archive_path: &str, checksum_path: &str, archive_name: &str) {
    let checksums = std::fs::read_to_string(checksum_path).unwrap_or_else(|e| {
        eprintln!("error: failed to read checksums: {}", e);
        std::process::exit(1);
    });

    let expected = checksums
        .lines()
        .find(|line| line.contains(archive_name))
        .and_then(|line| line.split_whitespace().next())
        .unwrap_or_else(|| {
            eprintln!("error: checksum not found for {}", archive_name);
            std::process::exit(1);
        });

    // Try sha256sum first (Linux), then shasum (macOS)
    let actual = if let Ok(output) = Command::new("sha256sum").arg(archive_path).output() {
        if output.status.success() {
            String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_string()
        } else {
            shasum_fallback(archive_path)
        }
    } else {
        shasum_fallback(archive_path)
    };

    if actual.is_empty() {
        eprintln!("warning: could not verify checksum (sha256sum/shasum not found)");
        return;
    }

    if expected != actual {
        eprintln!("error: checksum mismatch");
        eprintln!("       expected: {}", expected);
        eprintln!("       got:      {}", actual);
        std::process::exit(1);
    }
}

fn shasum_fallback(path: &str) -> String {
    Command::new("shasum")
        .args(["-a", "256", path])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_string()
        })
        .unwrap_or_default()
}

fn replace_binary(new_path: &str, current_exe: &std::path::Path) {
    #[cfg(not(target_os = "windows"))]
    {
        // Unix: copy new binary over the current one (inodes allow this)
        // Use a temp file + rename for atomicity
        let dir = current_exe.parent().unwrap();
        let tmp_path = dir.join(".almide-update.tmp");

        std::fs::copy(new_path, &tmp_path).unwrap_or_else(|e| {
            eprintln!("error: failed to copy new binary: {}", e);
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
                eprintln!("error: failed to replace binary: {}", e);
                eprintln!("       try: sudo cp {} {}", new_path, current_exe.display());
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
            eprintln!("error: failed to rename current binary: {}", e);
            std::process::exit(1);
        });
        std::fs::copy(new_path, current_exe).unwrap_or_else(|e| {
            // Rollback
            let _ = std::fs::rename(&old_path, current_exe);
            eprintln!("error: failed to copy new binary: {}", e);
            std::process::exit(1);
        });
        let _ = std::fs::remove_file(&old_path);
    }
}
