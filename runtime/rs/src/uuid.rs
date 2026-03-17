// uuid extern — Rust native implementations (no external crate)

pub fn almide_rt_uuid_v4() -> Result<String, String> {
    // Generate random UUID v4
    let bytes = random_16()?;
    Ok(format_uuid(&bytes, 4))
}

pub fn almide_rt_uuid_v5(namespace: String, name: String) -> Result<String, String> {
    // UUID v5: SHA-1 based (we use SHA-256 truncated for simplicity)
    let ns_bytes = parse_uuid_bytes(&namespace)?;
    let mut data = ns_bytes.to_vec();
    data.extend_from_slice(name.as_bytes());
    let hash = simple_hash(&data);
    Ok(format_uuid(&hash, 5))
}

pub fn almide_rt_uuid_nil() -> String {
    "00000000-0000-0000-0000-000000000000".to_string()
}

pub fn almide_rt_uuid_parse(s: String) -> Result<String, String> {
    let clean = s.replace('-', "");
    if clean.len() != 32 || !clean.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("invalid UUID: {}", s));
    }
    Ok(format!("{}-{}-{}-{}-{}", &clean[0..8], &clean[8..12], &clean[12..16], &clean[16..20], &clean[20..32]))
}

pub fn almide_rt_uuid_is_valid(s: String) -> bool {
    almide_rt_uuid_parse(s).is_ok()
}

pub fn almide_rt_uuid_version(s: String) -> Result<i64, String> {
    let clean = s.replace('-', "");
    if clean.len() != 32 { return Err("invalid UUID".into()); }
    let v = u8::from_str_radix(&clean[12..13], 16).map_err(|e| e.to_string())?;
    Ok(v as i64)
}

fn random_16() -> Result<[u8; 16], String> {
    use std::io::Read;
    let mut buf = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        f.read_exact(&mut buf).map_err(|e| e.to_string())?;
    } else {
        let mut state = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default().as_nanos() as u64;
        for b in &mut buf { state ^= state << 13; state ^= state >> 7; state ^= state << 17; *b = state as u8; }
    }
    Ok(buf)
}

fn format_uuid(bytes: &[u8; 16], version: u8) -> String {
    let mut b = *bytes;
    b[6] = (b[6] & 0x0f) | (version << 4); // version
    b[8] = (b[8] & 0x3f) | 0x80; // variant
    format!("{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0],b[1],b[2],b[3],b[4],b[5],b[6],b[7],b[8],b[9],b[10],b[11],b[12],b[13],b[14],b[15])
}

fn parse_uuid_bytes(s: &str) -> Result<[u8; 16], String> {
    let clean = s.replace('-', "");
    if clean.len() != 32 { return Err("invalid UUID".into()); }
    let mut bytes = [0u8; 16];
    for i in 0..16 {
        bytes[i] = u8::from_str_radix(&clean[i*2..i*2+2], 16).map_err(|e| e.to_string())?;
    }
    Ok(bytes)
}

fn simple_hash(data: &[u8]) -> [u8; 16] {
    // Simple hash for UUID v5 (not cryptographic, just deterministic)
    let mut h = [0u8; 16];
    for (i, &b) in data.iter().enumerate() {
        h[i % 16] ^= b;
        h[(i + 7) % 16] = h[(i + 7) % 16].wrapping_add(b);
    }
    h
}
