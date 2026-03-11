# HTTPS Client [IN PROGRESS]

## Approach
Do not implement TLS from scratch. Link directly to the OS standard TLS library using `rustc -l`.
No cargo dependency, no binary size increase (dynamic linking).

## Architecture

```
Almide code
  → http.get("https://...")
  → generated Rust: almide_https_get(url)
  → FFI: system TLS library
  → rustc -l framework=Security (macOS) / -l ssl -l crypto (Linux)
```

## Implementation Steps

### Step 1: HTTPS Support in URL Parser
- Recognize `https://` scheme
- Default port 443

### Step 2: macOS Implementation (Security.framework)
- `SSLCreateContext` → `SSLSetIOFuncs` → `SSLHandshake`
- CFNetwork's `CFReadStream` / `CFWriteStream` may be simpler
- Or call macOS standard `URLSession` via FFI
- Delegate certificate verification to the OS (no custom verification)

### Step 3: Linux Implementation (OpenSSL)
- `SSL_CTX_new` → `SSL_new` → `SSL_connect` → `SSL_read/write`
- Declare OpenSSL functions with `extern "C"`
- Delegate certificate verification to OpenSSL defaults

### Step 4: Integration
- Detect OS at compile time (`#[cfg(target_os)]`)
- Use different FFI depending on macOS / Linux
- almide CLI automatically sets `rustc` link flags

### Step 5: Testing
- GET test against `https://httpbin.org/get`
- Certificate error handling (self-signed certs, etc.)
- Confirm both HTTP and HTTPS work with the same `http.get` API

## Generated Code Example

```rust
// macOS
#[cfg(target_os = "macos")]
fn almide_https_get(url: &str) -> Result<String, String> {
    // Security.framework FFI
}

// Linux
#[cfg(target_os = "linux")]
fn almide_https_get(url: &str) -> Result<String, String> {
    // OpenSSL FFI
}
```

## rustc Link Flags

```bash
# macOS
rustc main.rs -l framework=Security -l framework=CoreFoundation

# Linux
rustc main.rs -l ssl -l crypto
```

## almide CLI Changes
- `almide build` / `almide run` automatically add link flags when HTTPS functions are used
- HTTPS not supported for WASM target (provided by the host)

## Risks
- Large FFI differences between OSes (separate implementations needed for macOS and Linux)
- Windows not supported (future support possible via Schannel)
- OpenSSL version differences (1.1 vs 3.x)

## Fallback Alternative
If system TLS is too complex, fall back to curl FFI:
```rust
fn almide_https_get(url: &str) -> Result<String, String> {
    let output = std::process::Command::new("curl")
        .arg("-s").arg(url)
        .output().map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
```
This works on all OSes. Performance is lower but security is maintained.

## Priority
Step 1 → Step 4 (curl fallback) → Step 2 (macOS) → Step 3 (Linux)
Implement curl fallback first to get something working, then replace incrementally with system TLS.
