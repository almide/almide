//! The ONE process the fetch path starts (ADR-0035 D2): `ssh` to an ssh
//! source's host, running `git-upload-pack` there.
//!
//! The argv is built from typed fields only: the port is a number, the user
//! and every path component are [`crate::git_source::Segment`]s (never
//! starting with `-`, holding no quote, space or shell character), the host
//! is a DNS name (never starting with `-`), and `--` ends ssh's options
//! before the host. Host-key checking is left to the user's ssh
//! configuration (on by default); nothing here turns it off.
//!
//! `tests/fetch_spawn_gate_test.rs` holds this file to that shape: it is the
//! only file of the fetch path that may start a process, [`launch`] is its
//! only function that does, and `launch` takes no string.

use std::process::{Child, Command, Stdio};

use crate::git_source::GitSource;

/// `ssh [-p PORT] [-l USER] -o SendEnv=GIT_PROTOCOL -- HOST git-upload-pack '/a/b[.git]'`,
/// with stdin and stdout piped for the git protocol.
pub fn launch(source: &GitSource, dot_git: bool) -> std::io::Result<Child> {
    let mut cmd = Command::new("ssh");
    cmd.arg("-o").arg("SendEnv=GIT_PROTOCOL");
    if let Some(port) = source.port {
        cmd.arg("-p").arg(port.to_string());
    }
    if let Some(user) = &source.user {
        cmd.arg("-l").arg(user.as_str());
    }
    cmd.arg("--")
        .arg(source.host.as_str())
        .arg(format!("git-upload-pack '{}'", source.remote_path(dot_git)))
        .env("GIT_PROTOCOL", "version=2")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
}
