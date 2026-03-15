/// I/O modules: fs, io, path, env, process.

// ──────────────────────────────── fs ────────────────────────────────

pub(super) const MOD_FS_TS: &str = r#"const __almd_fs = {
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
"#;

pub(super) const MOD_FS_JS: &str = r#"const __almd_fs = {
  exists(p) { const fs = require("fs"); try { fs.statSync(p); return true; } catch { return false; } },
  read_text(p) { return require("fs").readFileSync(p, "utf-8"); },
  read_bytes(p) { return Array.from(require("fs").readFileSync(p)); },
  write(p, s) { require("fs").writeFileSync(p, s); },
  write_bytes(p, b) { require("fs").writeFileSync(p, Buffer.from(b)); },
  append(p, s) { require("fs").appendFileSync(p, s); },
  mkdir_p(p) { require("fs").mkdirSync(p, { recursive: true }); },
  read_lines(p) { return require("fs").readFileSync(p, "utf-8").split("\n").filter(l => l.length > 0).map(l => l.replace(/\r$/, "")); },
  remove(p) { const fs = require("fs"); try { const s = fs.statSync(p); if (s.isDirectory()) fs.rmSync(p, { recursive: true }); else fs.unlinkSync(p); } catch(e) { throw e; } },
  list_dir(p) { return require("fs").readdirSync(p).sort(); },
  walk(dir) {
    const fs = require("fs");
    const results = [];
    function inner(d) {
      const entries = fs.readdirSync(d, { withFileTypes: true });
      for (const entry of entries) {
        const p = d + "/" + entry.name;
        results.push(p);
        if (entry.isDirectory()) inner(p);
      }
    }
    inner(dir);
    return results.sort();
  },
  stat(path) {
    const fs = require("fs");
    const s = fs.statSync(path);
    return { size: s.size, is_dir: s.isDirectory(), is_file: s.isFile(), modified: Math.floor(s.mtimeMs / 1000) };
  },
  is_dir(p) { try { return require("fs").statSync(p).isDirectory(); } catch { return false; } },
  is_file(p) { try { return require("fs").statSync(p).isFile(); } catch { return false; } },
  copy(src, dst) { require("fs").copyFileSync(src, dst); },
  rename(src, dst) { require("fs").renameSync(src, dst); },
  remove_all(p) { require("fs").rmSync(p, { recursive: true, force: true }); },
  file_size(p) { return require("fs").statSync(p).size; },
  temp_dir() { return require("os").tmpdir().replace(/\\/g, "/"); },
  glob(pattern) {
    const fs = require("fs"), path = require("path");
    const results = [];
    const parts = pattern.split("/");
    var base = "", gi = 0;
    for (var i = 0; i < parts.length; i++) {
      if (parts[i].includes("*") || parts[i].includes("?")) { gi = i; break; }
      base += (i > 0 ? "/" : "") + parts[i];
      gi = i + 1;
    }
    if (!base) base = ".";
    const globParts = parts.slice(gi);
    function match1(pat, name) {
      if (pat === "*") return true;
      var pi = 0, ni = 0, sp = -1, sn = 0;
      while (ni < name.length) {
        if (pi < pat.length && (pat[pi] === "?" || pat[pi] === name[ni])) { pi++; ni++; }
        else if (pi < pat.length && pat[pi] === "*") { sp = pi; sn = ni; pi++; }
        else if (sp >= 0) { pi = sp + 1; sn++; ni = sn; }
        else return false;
      }
      while (pi < pat.length && pat[pi] === "*") pi++;
      return pi === pat.length;
    }
    function inner(dir, gp) {
      if (gp.length === 0) return;
      var part = gp[0], rest = gp.slice(1);
      if (part === "**") {
        if (rest.length) inner(dir, rest);
        try { for (const e of fs.readdirSync(dir, { withFileTypes: true })) { if (e.isDirectory()) inner(dir + "/" + e.name, gp); } } catch {}
      } else {
        try {
          for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
            if (match1(part, e.name)) {
              var p = dir + "/" + e.name;
              if (rest.length === 0) results.push(p);
              else if (e.isDirectory()) inner(p, rest);
            }
          }
        } catch {}
      }
    }
    if (globParts.length === 0) { try { fs.statSync(base); results.push(base); } catch {} }
    else inner(base, globParts);
    return results.sort();
  },
  create_temp_file(prefix) {
    const os = require("os"), fs = require("fs"), path = require("path");
    const p = path.join(os.tmpdir(), prefix + Date.now() + Math.random().toString(36).slice(2));
    fs.writeFileSync(p, "");
    return p.replace(/\\/g, "/");
  },
  create_temp_dir(prefix) {
    const os = require("os"), fs = require("fs"), path = require("path");
    const p = path.join(os.tmpdir(), prefix + Date.now() + Math.random().toString(36).slice(2));
    fs.mkdirSync(p, { recursive: true });
    return p.replace(/\\/g, "/");
  },
  is_symlink(p) { try { return require("fs").lstatSync(p).isSymbolicLink(); } catch { return false; } },
  modified_at(p) { return Math.floor(require("fs").statSync(p).mtimeMs / 1000); },
};
"#;

// ──────────────────────────────── io ────────────────────────────────

pub(super) const MOD_IO_TS: &str = r#"const __almd_io = {
  read_line(): string { return prompt("") ?? ""; },
  print(s: string): void { const buf = new TextEncoder().encode(s); Deno.stdout.writeSync(buf); },
  read_all(): string { const d = new TextDecoder(); let r = ""; const buf = new Uint8Array(4096); let n: number | null; while ((n = Deno.stdin.readSync(buf)) !== null && n > 0) { r += d.decode(buf.subarray(0, n)); } return r; },
};
"#;

pub(super) const MOD_IO_JS: &str = r#"const __almd_io = {
  read_line() { const buf = Buffer.alloc(1024); let s = ""; while (true) { const n = require("fs").readSync(0, buf, 0, 1, null); if (n === 0) break; const ch = buf.toString("utf-8", 0, n); s += ch; if (ch === "\n") break; } return s.replace(/\r?\n$/, ""); },
  print(s) { __node_process.stdout.write(s); },
  read_all() { return require("fs").readFileSync(0, "utf-8"); },
};
"#;

// ──────────────────────────────── path ────────────────────────────────

pub(super) const MOD_PATH_TS: &str = r#"const __almd_path = {
  join(base: string, child: string): string { return base.replace(/\/+$/, "") + "/" + child; },
  dirname(p: string): string { const i = p.lastIndexOf("/"); return i >= 0 ? p.substring(0, i) : "."; },
  basename(p: string): string { const i = p.lastIndexOf("/"); return i >= 0 ? p.substring(i + 1) : p; },
  extension(p: string): string | null { const b = __almd_path.basename(p); const i = b.lastIndexOf("."); return i > 0 ? b.substring(i + 1) : null; },
  is_absolute(p: string): boolean { return p.startsWith("/"); },
};
"#;

pub(super) const MOD_PATH_JS: &str = r#"const __almd_path = {
  join(base, child) { return base.replace(/\/+$/, "") + "/" + child; },
  dirname(p) { const i = p.lastIndexOf("/"); return i >= 0 ? p.substring(0, i) : "."; },
  basename(p) { const i = p.lastIndexOf("/"); return i >= 0 ? p.substring(i + 1) : p; },
  extension(p) { const b = __almd_path.basename(p); const i = b.lastIndexOf("."); return i > 0 ? b.substring(i + 1) : null; },
  is_absolute(p) { return p.startsWith("/"); },
};
"#;

// ──────────────────────────────── env ────────────────────────────────

pub(super) const MOD_ENV_TS: &str = r#"const __almd_env = {
  unix_timestamp(): number { return Math.floor(Date.now() / 1000); },
  args(): string[] { return Deno.args; },
  get(name: string): string | null { const v = Deno.env.get(name); return v !== undefined ? v : null; },
  set(name: string, value: string): void { Deno.env.set(name, value); },
  cwd(): string { return Deno.cwd(); },
  millis(): number { return Date.now(); },
  sleep_ms(ms: number): void { const end = Date.now() + ms; while (Date.now() < end) {} },
  temp_dir(): string { return Deno.env.get("TMPDIR") || Deno.env.get("TEMP") || Deno.env.get("TMP") || "/tmp"; },
  os(): string { return Deno.build.os === "darwin" ? "macos" : Deno.build.os; },
};
"#;

pub(super) const MOD_ENV_JS: &str = r#"const __almd_env = {
  unix_timestamp() { return Math.floor(Date.now() / 1000); },
  args() { return __node_process.argv.slice(2); },
  get(name) { const v = __node_process.env[name]; return v !== undefined ? v : null; },
  set(name, value) { __node_process.env[name] = value; },
  cwd() { return __node_process.cwd(); },
  millis() { return Date.now(); },
  sleep_ms(ms) { const end = Date.now() + ms; while (Date.now() < end) {} },
  temp_dir() { return require("os").tmpdir(); },
  os() { const p = require("os").platform(); return p === "darwin" ? "macos" : p === "win32" ? "windows" : p; },
};
"#;

// ──────────────────────────────── process ────────────────────────────────

pub(super) const MOD_PROCESS_TS: &str = r#"const __almd_process = {
  exec(cmd: string, args: string[]): string { try { const p = new Deno.Command(cmd, { args, stdout: "piped", stderr: "piped" }); const out = p.outputSync(); if (out.success) { return new TextDecoder().decode(out.stdout); } else { const msg = new TextDecoder().decode(out.stderr); throw new Error(msg || "command failed"); } } catch (e) { if (e instanceof Error) throw e; throw new Error(String(e)); } },
  exec_status(cmd: string, args: string[]): {code: number, stdout: string, stderr: string} { try { const p = new Deno.Command(cmd, { args, stdout: "piped", stderr: "piped" }); const out = p.outputSync(); return { code: out.code, stdout: new TextDecoder().decode(out.stdout), stderr: new TextDecoder().decode(out.stderr) }; } catch (e) { throw e instanceof Error ? e : new Error(String(e)); } },
  exit(code: number): void { Deno.exit(code); },
  stdin_lines(): string[] { const buf = new Uint8Array(1024 * 1024); const n = Deno.stdin.readSync(buf); return n ? new TextDecoder().decode(buf.subarray(0, n)).split("\n").filter(l => l.length > 0) : []; },
  exec_in(dir: string, cmd: string, args: string[]): string { try { const p = new Deno.Command(cmd, { args, cwd: dir, stdout: "piped", stderr: "piped" }); const out = p.outputSync(); if (out.success) { return new TextDecoder().decode(out.stdout); } else { const msg = new TextDecoder().decode(out.stderr); throw new Error(msg || "command failed"); } } catch (e) { if (e instanceof Error) throw e; throw new Error(String(e)); } },
  exec_with_stdin(cmd: string, args: string[], input: string): string { try { const p = new Deno.Command(cmd, { args, stdin: "piped", stdout: "piped", stderr: "piped" }); const child = p.spawn(); const writer = child.stdin.getWriter(); writer.write(new TextEncoder().encode(input)); writer.close(); const out = child.outputSync(); if (out.success) { return new TextDecoder().decode(out.stdout); } else { throw new Error(new TextDecoder().decode(out.stderr) || "command failed"); } } catch (e) { if (e instanceof Error) throw e; throw new Error(String(e)); } },
};
"#;

pub(super) const MOD_PROCESS_JS: &str = r#"const __almd_process = {
  exec(cmd, args) { const { execFileSync } = require("child_process"); try { return execFileSync(cmd, args, { encoding: "utf-8" }); } catch (e) { const msg = e.stderr ? String(e.stderr) : e.message; throw new Error(msg || "command failed"); } },
  exec_status(cmd, args) { const { spawnSync } = require("child_process"); const r = spawnSync(cmd, args, { encoding: "utf-8" }); if (r.error) throw r.error; return { code: r.status ?? 1, stdout: r.stdout || "", stderr: r.stderr || "" }; },
  exit(code) { __node_process.exit(code); },
  stdin_lines() { return require("fs").readFileSync(0, "utf-8").split("\n").filter(l => l.length > 0); },
  exec_in(dir, cmd, args) { const { execFileSync } = require("child_process"); try { return execFileSync(cmd, args, { encoding: "utf-8", cwd: dir }); } catch (e) { const msg = e.stderr ? String(e.stderr) : e.message; throw new Error(msg || "command failed"); } },
  exec_with_stdin(cmd, args, input) { const { execFileSync } = require("child_process"); try { return execFileSync(cmd, args, { encoding: "utf-8", input }); } catch (e) { const msg = e.stderr ? String(e.stderr) : e.message; throw new Error(msg || "command failed"); } },
};
"#;
