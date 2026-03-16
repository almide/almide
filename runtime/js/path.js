const __almd_path = {
  join(base, child) { return base.replace(/\/+$/, "") + "/" + child; },
  dirname(p) { const i = p.lastIndexOf("/"); return i >= 0 ? p.substring(0, i) : "."; },
  basename(p) { const i = p.lastIndexOf("/"); return i >= 0 ? p.substring(i + 1) : p; },
  extension(p) { const b = __almd_path.basename(p); const i = b.lastIndexOf("."); return i > 0 ? b.substring(i + 1) : null; },
  is_absolute(p) { return p.startsWith("/"); },
};
