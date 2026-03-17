const __almd_crypto = {
  random_bytes(n) {
    return Array.from(require("crypto").randomBytes(n));
  },
  random_hex(n) {
    return require("crypto").randomBytes(n).toString("hex");
  },
  hmac_sha256(key, data) {
    const h = require("crypto").createHmac("sha256", key);
    h.update(data);
    return h.digest("hex");
  },
  hmac_verify(key, data, signature) {
    const computed = __almd_crypto.hmac_sha256(key, data);
    if (computed.length !== signature.length) return false;
    return require("crypto").timingSafeEqual(Buffer.from(computed), Buffer.from(signature));
  },
};
