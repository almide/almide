const __almd_fs = {
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
