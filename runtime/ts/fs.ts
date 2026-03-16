const __almd_fs = {
  exists(p: string): boolean { try { Deno.statSync(p); return true; } catch { return false; } },
  read_text(p: string): string { return Deno.readTextFileSync(p); },
  read_bytes(p: string): Uint8Array { return Deno.readFileSync(p); },
  write(p: string, s: string): void { Deno.writeTextFileSync(p, s); },
  write_bytes(p: string, b: Uint8Array | number[]): void { Deno.writeFileSync(p, b instanceof Uint8Array ? b : new Uint8Array(b)); },
  append(p: string, s: string): void { Deno.writeTextFileSync(p, Deno.readTextFileSync(p) + s); },
  mkdir_p(p: string): void { Deno.mkdirSync(p, { recursive: true }); },
  read_lines(p: string): string[] { return Deno.readTextFileSync(p).split("\n").filter(l => l.length > 0).map(l => l.replace(/\r$/, "")); },
  remove(p: string): void { Deno.removeSync(p, { recursive: true }); },
  list_dir(p: string): string[] { return [...Deno.readDirSync(p)].map(e => e.name).sort(); },
  walk(dir: string): string[] {
    const results: string[] = [];
    function inner(d: string) {
      for (const entry of Deno.readDirSync(d)) {
        const p = d + "/" + entry.name;
        results.push(p);
        if (entry.isDirectory) inner(p);
      }
    }
    inner(dir);
    return results.sort();
  },
  stat(path: string): {size: number, is_dir: boolean, is_file: boolean, modified: number} {
    const s = Deno.statSync(path);
    return { size: s.size, is_dir: s.isDirectory, is_file: s.isFile, modified: Math.floor((s.mtime?.getTime() ?? 0) / 1000) };
  },
  is_dir(p: string): boolean { try { return Deno.statSync(p).isDirectory; } catch { return false; } },
  is_file(p: string): boolean { try { return Deno.statSync(p).isFile; } catch { return false; } },
  copy(src: string, dst: string): void { Deno.copyFileSync(src, dst); },
  rename(src: string, dst: string): void { Deno.renameSync(src, dst); },
  remove_all(p: string): void { try { Deno.removeSync(p, { recursive: true }); } catch {} },
  file_size(p: string): number { return Deno.statSync(p).size; },
  temp_dir(): string { return Deno.env.get("TMPDIR") || "/tmp"; },
  glob(pattern: string): string[] {
    const results: string[] = [];
    const parts = pattern.split("/");
    let base = "";
    let gi = 0;
    for (let i = 0; i < parts.length; i++) {
      if (parts[i].includes("*") || parts[i].includes("?")) { gi = i; break; }
      base += (i > 0 ? "/" : "") + parts[i];
      gi = i + 1;
    }
    if (!base) base = ".";
    const globParts = parts.slice(gi);
    function match1(pat: string, name: string): boolean {
      if (pat === "*") return true;
      let pi = 0, ni = 0, sp = -1, sn = 0;
      while (ni < name.length) {
        if (pi < pat.length && (pat[pi] === "?" || pat[pi] === name[ni])) { pi++; ni++; }
        else if (pi < pat.length && pat[pi] === "*") { sp = pi; sn = ni; pi++; }
        else if (sp >= 0) { pi = sp + 1; sn++; ni = sn; }
        else return false;
      }
      while (pi < pat.length && pat[pi] === "*") pi++;
      return pi === pat.length;
    }
    function inner(dir: string, gp: string[]) {
      if (gp.length === 0) return;
      const part = gp[0], rest = gp.slice(1);
      if (part === "**") {
        if (rest.length) inner(dir, rest);
        try { for (const e of Deno.readDirSync(dir)) { if (e.isDirectory) inner(dir + "/" + e.name, gp); } } catch {}
      } else {
        try {
          for (const e of Deno.readDirSync(dir)) {
            if (match1(part, e.name)) {
              const p = dir + "/" + e.name;
              if (rest.length === 0) results.push(p);
              else if (e.isDirectory) inner(p, rest);
            }
          }
        } catch {}
      }
    }
    if (globParts.length === 0) { try { Deno.statSync(base); results.push(base); } catch {} }
    else inner(base, globParts);
    return results.sort();
  },
  create_temp_file(prefix: string): string {
    const p = Deno.makeTempFileSync({ prefix });
    return p.replace(/\\/g, "/");
  },
  create_temp_dir(prefix: string): string {
    const p = Deno.makeTempDirSync({ prefix });
    return p.replace(/\\/g, "/");
  },
  is_symlink(p: string): boolean { try { return Deno.lstatSync(p).isSymlink; } catch { return false; } },
  modified_at(p: string): number { const s = Deno.statSync(p); return Math.floor((s.mtime?.getTime() ?? 0) / 1000); },
};
