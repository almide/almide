// fs extern — Rust native implementations (platform layer: native only, no WASM)

use std::path::Path;

fn io_err(e: impl std::fmt::Display) -> String { format!("{}", e) }

// Read
pub fn almide_rt_fs_read_text(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(io_err)
}
pub fn almide_rt_fs_read_bytes(path: String) -> Result<Vec<i64>, String> {
    std::fs::read(&path).map(|b| b.into_iter().map(|x| x as i64).collect()).map_err(io_err)
}
pub fn almide_rt_fs_read_lines(path: String) -> Result<Vec<String>, String> {
    std::fs::read_to_string(&path).map(|s| s.lines().map(|l| l.to_string()).collect()).map_err(io_err)
}

// Write
pub fn almide_rt_fs_write(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, &content).map_err(io_err)
}
pub fn almide_rt_fs_write_bytes(path: String, bytes: Vec<i64>) -> Result<(), String> {
    let data: Vec<u8> = bytes.iter().map(|&b| b as u8).collect();
    std::fs::write(&path, &data).map_err(io_err)
}
pub fn almide_rt_fs_append(path: String, content: String) -> Result<(), String> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().append(true).create(true).open(&path).map_err(io_err)?;
    f.write_all(content.as_bytes()).map_err(io_err)
}

// Directory
pub fn almide_rt_fs_mkdir_p(path: String) -> Result<(), String> {
    std::fs::create_dir_all(&path).map_err(io_err)
}
pub fn almide_rt_fs_list_dir(path: String) -> Result<Vec<String>, String> {
    let entries = std::fs::read_dir(&path).map_err(io_err)?;
    let mut names = Vec::new();
    for entry in entries {
        let e = entry.map_err(io_err)?;
        names.push(e.file_name().to_string_lossy().to_string());
    }
    names.sort();
    Ok(names)
}

// Delete
pub fn almide_rt_fs_remove(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if p.is_dir() { std::fs::remove_dir(&path).map_err(io_err) }
    else { std::fs::remove_file(&path).map_err(io_err) }
}
pub fn almide_rt_fs_remove_all(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if p.is_dir() { std::fs::remove_dir_all(&path).map_err(io_err) }
    else { std::fs::remove_file(&path).map_err(io_err) }
}

// Copy / Rename
pub fn almide_rt_fs_copy(src: String, dst: String) -> Result<(), String> {
    std::fs::copy(&src, &dst).map(|_| ()).map_err(io_err)
}
pub fn almide_rt_fs_rename(src: String, dst: String) -> Result<(), String> {
    std::fs::rename(&src, &dst).map_err(io_err)
}

// Predicates
pub fn almide_rt_fs_exists(path: String) -> bool { Path::new(&path).exists() }
pub fn almide_rt_fs_is_dir(path: String) -> bool { Path::new(&path).is_dir() }
pub fn almide_rt_fs_is_file(path: String) -> bool { Path::new(&path).is_file() }
pub fn almide_rt_fs_is_symlink(path: String) -> bool { Path::new(&path).is_symlink() }

// Metadata
pub fn almide_rt_fs_file_size(path: String) -> Result<i64, String> {
    std::fs::metadata(&path).map(|m| m.len() as i64).map_err(io_err)
}
pub fn almide_rt_fs_modified_at(path: String) -> Result<i64, String> {
    let meta = std::fs::metadata(&path).map_err(io_err)?;
    let modified = meta.modified().map_err(io_err)?;
    Ok(modified.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64)
}
pub fn almide_rt_fs_stat(path: String) -> Result<(i64, bool, bool, i64), String> {
    let meta = std::fs::metadata(&path).map_err(io_err)?;
    let size = meta.len() as i64;
    let is_dir = meta.is_dir();
    let is_file = meta.is_file();
    let modified = meta.modified().ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64).unwrap_or(0);
    Ok((size, is_dir, is_file, modified))
}

// Temp
pub fn almide_rt_fs_temp_dir() -> String {
    std::env::temp_dir().to_string_lossy().replace('\\', "/")
}
pub fn almide_rt_fs_create_temp_file(prefix: String) -> Result<String, String> {
    let dir = std::env::temp_dir();
    let name = format!("{}{}", prefix, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
    let path = dir.join(&name);
    std::fs::write(&path, "").map_err(io_err)?;
    Ok(path.to_string_lossy().replace('\\', "/"))
}
pub fn almide_rt_fs_create_temp_dir(prefix: String) -> Result<String, String> {
    let dir = std::env::temp_dir();
    let name = format!("{}{}", prefix, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
    let path = dir.join(&name);
    std::fs::create_dir_all(&path).map_err(io_err)?;
    Ok(path.to_string_lossy().replace('\\', "/"))
}

// Walk (recursive)
pub fn almide_rt_fs_walk(dir: String) -> Result<Vec<String>, String> {
    let mut results = Vec::new();
    walk_recursive(Path::new(&dir), &mut results)?;
    results.sort();
    Ok(results)
}

fn walk_recursive(dir: &Path, results: &mut Vec<String>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(io_err)? {
        let entry = entry.map_err(io_err)?;
        let path = entry.path();
        results.push(path.to_string_lossy().replace('\\', "/"));
        if path.is_dir() { walk_recursive(&path, results)?; }
    }
    Ok(())
}

// Glob (simple pattern matching)
pub fn almide_rt_fs_glob(pattern: String) -> Result<Vec<String>, String> {
    // Simple glob: split by * and match files in current dir
    let dir = Path::new(".").canonicalize().map_err(io_err)?;
    let mut results = Vec::new();
    glob_recursive(&dir, &pattern, &mut results)?;
    results.sort();
    Ok(results)
}

fn glob_recursive(dir: &Path, pattern: &str, results: &mut Vec<String>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(io_err)? {
        let entry = entry.map_err(io_err)?;
        let path = entry.path();
        let name = path.to_string_lossy().replace('\\', "/");
        if glob_match(pattern, &name) {
            results.push(name.clone());
        }
        if path.is_dir() { glob_recursive(&path, pattern, results)?; }
    }
    Ok(())
}

fn glob_match(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 { return name == pattern; }
    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() { continue; }
        match name[pos..].find(part) {
            Some(idx) => {
                if i == 0 && idx != 0 { return false; }
                pos += idx + part.len();
            }
            None => return false,
        }
    }
    true
}

pub fn almide_rt_fs_read_bytes_raw(path: String) -> Result<Vec<u8>, String> {
    std::fs::read(&path).map_err(io_err)
}

pub fn almide_rt_fs_write_bytes_raw(path: &str, data: &Vec<u8>) -> Result<(), String> {
    std::fs::write(path, data).map_err(io_err)
}
