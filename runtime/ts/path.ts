const __almd_path = {
  join(base: string, child: string): string { return base.replace(/\/+$/, "") + "/" + child; },
  dirname(p: string): string { const i = p.lastIndexOf("/"); return i >= 0 ? p.substring(0, i) : "."; },
  basename(p: string): string { const i = p.lastIndexOf("/"); return i >= 0 ? p.substring(i + 1) : p; },
  extension(p: string): string | null { const b = __almd_path.basename(p); const i = b.lastIndexOf("."); return i > 0 ? b.substring(i + 1) : null; },
  is_absolute(p: string): boolean { return p.startsWith("/"); },
};
