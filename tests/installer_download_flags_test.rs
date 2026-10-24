//! #4181: every download in the installers is held to https and TLS 1.2+,
//! ignores the user's `~/.curlrc`, and follows a redirect only to https. A
//! curl call added without the shared flags fails here.

#[test]
fn every_install_sh_download_uses_the_hardened_curl() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tools/install.sh")).unwrap();
    let def = src
        .lines()
        .find(|l| l.starts_with("CURL="))
        .expect("install.sh defines the shared CURL command");
    for flag in ["curl -q ", "--proto =https", "--proto-redir =https", "--tlsv1.2"] {
        assert!(def.contains(flag), "CURL= must carry `{flag}`: {def}");
    }
    let bare: Vec<&str> = src
        .lines()
        .map(str::trim_start)
        .filter(|l| !l.starts_with('#') && !l.starts_with("CURL="))
        .filter(|l| l.split(|c: char| !c.is_ascii_alphanumeric() && c != '_').any(|w| w == "curl"))
        .collect();
    assert!(bare.is_empty(), "a download must go through $CURL, not a bare curl: {bare:#?}");
}

#[test]
fn install_ps1_requires_tls12() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tools/install.ps1")).unwrap();
    assert!(src.contains("SecurityProtocolType]::Tls12"), "install.ps1 must enable TLS 1.2");
    assert!(!src.contains("http://"), "install.ps1 must not name a plain-http URL");
}
