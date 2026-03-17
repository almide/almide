const __almd_process = {
  exec(cmd, args) { const { execFileSync } = require("child_process"); try { return execFileSync(cmd, args, { encoding: "utf-8" }); } catch (e) { const msg = e.stderr ? String(e.stderr) : e.message; throw new Error(msg || "command failed"); } },
  exec_status(cmd, args) { const { spawnSync } = require("child_process"); const r = spawnSync(cmd, args, { encoding: "utf-8" }); if (r.error) throw r.error; return { code: r.status ?? 1, stdout: r.stdout || "", stderr: r.stderr || "" }; },
  exit(code) { __node_process.exit(code); },
  stdin_lines() { return require("fs").readFileSync(0, "utf-8").split("\n").filter(l => l.length > 0); },
  exec_in(dir, cmd, args) { const { execFileSync } = require("child_process"); try { return execFileSync(cmd, args, { encoding: "utf-8", cwd: dir }); } catch (e) { const msg = e.stderr ? String(e.stderr) : e.message; throw new Error(msg || "command failed"); } },
  exec_with_stdin(cmd, args, input) { const { execFileSync } = require("child_process"); try { return execFileSync(cmd, args, { encoding: "utf-8", input }); } catch (e) { const msg = e.stderr ? String(e.stderr) : e.message; throw new Error(msg || "command failed"); } },
};
