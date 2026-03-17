// crypto extern — Rust native implementations (no external crate)

pub fn almide_rt_crypto_random_bytes(n: i64) -> Result<Vec<i64>, String> {
    use std::fs::File;
    use std::io::Read;
    let mut buf = vec![0u8; n as usize];
    // Try /dev/urandom first, fall back to time-based
    if let Ok(mut f) = File::open("/dev/urandom") {
        f.read_exact(&mut buf).map_err(|e| e.to_string())?;
    } else {
        // Fallback: xorshift from time
        let mut state = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default().as_nanos() as u64;
        for b in &mut buf {
            state ^= state << 13; state ^= state >> 7; state ^= state << 17;
            *b = state as u8;
        }
    }
    Ok(buf.into_iter().map(|b| b as i64).collect())
}

pub fn almide_rt_crypto_random_hex(n: i64) -> Result<String, String> {
    let bytes = almide_rt_crypto_random_bytes(n)?;
    Ok(bytes.iter().map(|b| format!("{:02x}", *b as u8)).collect())
}

pub fn almide_rt_crypto_hmac_sha256(key: String, data: String) -> Result<String, String> {
    // Simple HMAC-SHA256 implementation
    let key_bytes = key.as_bytes();
    let data_bytes = data.as_bytes();
    let block_size = 64usize;

    let key_block = if key_bytes.len() > block_size {
        let h = sha256(key_bytes);
        let mut k = h.to_vec();
        k.resize(block_size, 0);
        k
    } else {
        let mut k = key_bytes.to_vec();
        k.resize(block_size, 0);
        k
    };

    let o_key_pad: Vec<u8> = key_block.iter().map(|b| b ^ 0x5c).collect();
    let i_key_pad: Vec<u8> = key_block.iter().map(|b| b ^ 0x36).collect();

    let mut inner = i_key_pad;
    inner.extend_from_slice(data_bytes);
    let inner_hash = sha256(&inner);

    let mut outer = o_key_pad;
    outer.extend_from_slice(&inner_hash);
    let result = sha256(&outer);

    Ok(result.iter().map(|b| format!("{:02x}", b)).collect())
}

pub fn almide_rt_crypto_hmac_verify(key: String, data: String, signature: String) -> Result<bool, String> {
    let computed = almide_rt_crypto_hmac_sha256(key, data)?;
    Ok(computed == signature)
}

// Minimal SHA-256 implementation
fn sha256(data: &[u8]) -> [u8; 32] {
    let k: [u32; 64] = [
        0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
        0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
        0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
        0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
        0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
        0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
        0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
        0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667,0xbb67ae85,0x3c6ef372,0xa54ff53a,0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19];

    // Padding
    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 { msg.push(0); }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i*4], chunk[i*4+1], chunk[i*4+2], chunk[i*4+3]]);
        }
        for i in 16..64 {
            let s0 = w[i-15].rotate_right(7) ^ w[i-15].rotate_right(18) ^ (w[i-15] >> 3);
            let s1 = w[i-2].rotate_right(17) ^ w[i-2].rotate_right(19) ^ (w[i-2] >> 10);
            w[i] = w[i-16].wrapping_add(s0).wrapping_add(w[i-7]).wrapping_add(s1);
        }
        let [mut a,mut b,mut c,mut d,mut e,mut f,mut g,mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(k[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh=g; g=f; f=e; e=d.wrapping_add(t1); d=c; c=b; b=a; a=t1.wrapping_add(t2);
        }
        for (i, v) in [a,b,c,d,e,f,g,hh].iter().enumerate() { h[i] = h[i].wrapping_add(*v); }
    }
    let mut out = [0u8; 32];
    for i in 0..8 { out[i*4..i*4+4].copy_from_slice(&h[i].to_be_bytes()); }
    out
}
