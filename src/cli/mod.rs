/// CLI command implementations.

mod run;
mod build;
mod emit;
mod check;
mod commands;

pub use run::{cmd_run, cmd_run_inner};
pub use build::cmd_build;
pub use emit::cmd_emit;
pub use check::{cmd_check, cmd_check_json};
pub use commands::{cmd_init, cmd_test, cmd_test_json, cmd_fmt, cmd_clean};

use std::hash::{Hash, Hasher};

/// Compute a 64-bit hash of a byte slice (using DefaultHasher).
fn hash64(data: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hasher);
    hasher.finish()
}

/// Cache directory for incremental compilation.
fn incremental_cache_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(".almide/cache")
}

/// Recursively collect .almd files that contain `test` blocks.
fn collect_test_files(dir: &std::path::Path) -> Vec<String> {
    let mut files = Vec::new();
    // Skip hidden directories, target/, node_modules/, etc.
    let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if dir_name.starts_with('.') || dir_name == "target" || dir_name == "node_modules" {
        return files;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                files.extend(collect_test_files(&path));
            } else if path.extension().map(|e| e == "almd").unwrap_or(false) {
                // Check if file contains a test block
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if content.contains("\ntest ") || content.starts_with("test ") {
                        files.push(path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
    files
}
