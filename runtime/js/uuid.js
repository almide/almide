const __almd_uuid = {
  v4() {
    return require("crypto").randomUUID();
  },
  v5(namespace, name) {
    const data = namespace.replace(/-/g, "") + name;
    let hash = 0;
    for (let i = 0; i < data.length; i++) {
      hash = ((hash << 5) - hash + data.charCodeAt(i)) | 0;
    }
    const hex = Math.abs(hash).toString(16).padStart(32, "0").slice(0, 32);
    return `${hex.slice(0,8)}-${hex.slice(8,12)}-5${hex.slice(13,16)}-${(parseInt(hex.slice(16,18),16) & 0x3F | 0x80).toString(16)}${hex.slice(18,20)}-${hex.slice(20,32)}`;
  },
  parse(s) {
    if (!__almd_uuid.is_valid(s)) throw new Error("invalid UUID: " + s);
    return s.toLowerCase();
  },
  is_valid(s) {
    return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(s.trim());
  },
  nil() { return "00000000-0000-0000-0000-000000000000"; },
  version(s) {
    if (!__almd_uuid.is_valid(s)) throw new Error("invalid UUID: " + s);
    return parseInt(s.charAt(14), 16);
  },
};
