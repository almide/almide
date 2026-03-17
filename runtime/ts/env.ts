const __almd_env = {
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
