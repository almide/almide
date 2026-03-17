const __almd_io = {
  read_line() { const buf = Buffer.alloc(1024); let s = ""; while (true) { const n = require("fs").readSync(0, buf, 0, 1, null); if (n === 0) break; const ch = buf.toString("utf-8", 0, n); s += ch; if (ch === "\n") break; } return s.replace(/\r?\n$/, ""); },
  print(s) { __node_process.stdout.write(s); },
  read_all() { return require("fs").readFileSync(0, "utf-8"); },
};
