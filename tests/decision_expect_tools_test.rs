//! #4858: a harness test whose external tool is missing fails by default. The
//! workspace's cargo config arms `ALMIDE_EXPECT_TOOLS` for every `cargo test`
//! process, so the skip branch of a tool-needing test — here, one asking for a
//! tool name no machine has — is a failure unless the run opted out. Row
//! `env.expect-tools`.
use std::process::Command;

#[path = "support/novel.rs"]
mod novel;

/// The readers' own idiom: a missing tool is a skip unless the switch is set.
fn tool_or_verdict(tool: &str) -> Result<(), String> {
    let found = Command::new(tool).arg("--version").output().is_ok_and(|o| o.status.success());
    if found {
        return Ok(());
    }
    if std::env::var_os("ALMIDE_EXPECT_TOOLS").is_some() {
        Err(format!("{tool} is required (ALMIDE_EXPECT_TOOLS is set) but was not found"))
    } else {
        Ok(()) // the silent skip #4858 is about
    }
}

#[test]
fn novel_a_missing_tool_fails_by_default() {
    let tool = novel::ident("no_such_tool");
    assert!(
        tool_or_verdict(&tool).is_err(),
        "a harness test whose tool `{tool}` is missing passed: ALMIDE_EXPECT_TOOLS is not armed by default"
    );
}
