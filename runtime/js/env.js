const __almd_env = {
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
