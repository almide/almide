const __almd_io = {
  read_line(): string { return prompt("") ?? ""; },
  print(s: string): void { const buf = new TextEncoder().encode(s); Deno.stdout.writeSync(buf); },
  read_all(): string { const d = new TextDecoder(); let r = ""; const buf = new Uint8Array(4096); let n: number | null; while ((n = Deno.stdin.readSync(buf)) !== null && n > 0) { r += d.decode(buf.subarray(0, n)); } return r; },
};
