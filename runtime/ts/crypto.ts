const __almd_crypto = {
  random_bytes(n: number): number[] {
    const buf = new Uint8Array(n);
    crypto.getRandomValues(buf);
    return Array.from(buf);
  },
  random_hex(n: number): string {
    const buf = new Uint8Array(n);
    crypto.getRandomValues(buf);
    return Array.from(buf).map(b => b.toString(16).padStart(2, "0")).join("");
  },
  async hmac_sha256(key: string, data: string): Promise<string> {
    const enc = new TextEncoder();
    const k = await crypto.subtle.importKey("raw", enc.encode(key), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
    const sig = await crypto.subtle.sign("HMAC", k, enc.encode(data));
    return Array.from(new Uint8Array(sig)).map(b => b.toString(16).padStart(2, "0")).join("");
  },
  async hmac_verify(key: string, data: string, signature: string): Promise<boolean> {
    const computed = await __almd_crypto.hmac_sha256(key, data);
    if (computed.length !== signature.length) return false;
    let diff = 0;
    for (let i = 0; i < computed.length; i++) diff |= computed.charCodeAt(i) ^ signature.charCodeAt(i);
    return diff === 0;
  },
};
