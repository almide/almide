// Almide AST → TypeScript (Deno) code generator
import type {
  Program, Decl, Stmt, Expr, Pattern, MatchArm,
  FieldInit, TypeExpr,
} from "./ast.ts";

const RUNTIME = `// ---- Almide Runtime ----
const __fs = {
  exists(p: string): boolean { try { Deno.statSync(p); return true; } catch { return false; } },
  read_text(p: string): string { return Deno.readTextFileSync(p); },
  read_bytes(p: string): Uint8Array { return Deno.readFileSync(p); },
  write(p: string, s: string): void { Deno.writeTextFileSync(p, s); },
  write_bytes(p: string, b: Uint8Array | number[]): void { Deno.writeFileSync(p, b instanceof Uint8Array ? b : new Uint8Array(b)); },
  append(p: string, s: string): void { Deno.writeTextFileSync(p, Deno.readTextFileSync(p) + s); },
  mkdir_p(p: string): void { Deno.mkdirSync(p, { recursive: true }); },
  exists_q(p: string): boolean { try { Deno.statSync(p); return true; } catch { return false; } },
};
const __string = {
  trim(s: string): string { return s.trim(); },
  split(s: string, sep: string): string[] { return s.split(sep); },
  join(arr: string[], sep: string): string { return arr.join(sep); },
  len(s: string): number { return s.length; },
  pad_left(s: string, n: number, ch: string): string { return s.padStart(n, ch); },
  starts_with(s: string, prefix: string): boolean { return s.startsWith(prefix); },
  slice(s: string, start: number, end?: number): string { return end !== undefined ? s.slice(start, end) : s.slice(start); },
  to_bytes(s: string): number[] { return Array.from(new TextEncoder().encode(s)); },
  contains(s: string, sub: string): boolean { return s.includes(sub); },
  starts_with_q(s: string, prefix: string): boolean { return s.startsWith(prefix); },
  ends_with_q(s: string, suffix: string): boolean { return s.endsWith(suffix); },
  to_upper(s: string): string { return s.toUpperCase(); },
  to_lower(s: string): string { return s.toLowerCase(); },
  to_int(s: string): number { const n = parseInt(s, 10); if (isNaN(n)) throw new Error("invalid integer: " + s); return n; },
  replace(s: string, from: string, to: string): string { return s.split(from).join(to); },
  char_at(s: string, i: number): string | null { return i < s.length ? s[i] : null; },
};
const __list = {
  len<T>(xs: T[]): number { return xs.length; },
  get<T>(xs: T[], i: number): T | null { return i < xs.length ? xs[i] : null; },
  sort<T>(xs: T[]): T[] { return [...xs].sort(); },
  contains<T>(xs: T[], x: T): boolean { return xs.includes(x); },
  each<T>(xs: T[], f: (x: T) => void): void { xs.forEach(f); },
  map<T, U>(xs: T[], f: (x: T) => U): U[] { return xs.map(f); },
  filter<T>(xs: T[], f: (x: T) => boolean): T[] { return xs.filter(f); },
  find<T>(xs: T[], f: (x: T) => boolean): T | null { return xs.find(f) ?? null; },
  fold<T, U>(xs: T[], init: U, f: (acc: U, x: T) => U): U { return xs.reduce(f, init); },
};
const __int = {
  to_hex(n: bigint): string { return (n >= 0n ? n : n + (1n << 64n)).toString(16); },
  to_string(n: number): string { return String(n); },
};
const __env = {
  unix_timestamp(): number { return Math.floor(Date.now() / 1000); },
  args(): string[] { return Deno.args; },
};
function __bigop(op: string, a: any, b: any): any {
  if (typeof a === "bigint" || typeof b === "bigint") {
    const ba = typeof a === "bigint" ? a : BigInt(a);
    const bb = typeof b === "bigint" ? b : BigInt(b);
    switch(op) {
      case "^": return ba ^ bb;
      case "*": return ba * bb;
      case "%": return ba % bb;
      case "+": return ba + bb;
      case "-": return ba - bb;
      default: return ba;
    }
  }
  switch(op) {
    case "^": return a ^ b; case "*": return a * b; case "%": return a % b;
    case "+": return a + b; case "-": return a - b; default: return a;
  }
}
function println(s: string): void { console.log(s); }
function eprintln(s: string): void { console.error(s); }
function __deep_eq(a: any, b: any): boolean {
  if (a === b) return true;
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) { if (!__deep_eq(a[i], b[i])) return false; }
    return true;
  }
  if (a && b && typeof a === "object" && typeof b === "object") {
    const ka = Object.keys(a), kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    for (const k of ka) { if (!__deep_eq(a[k], b[k])) return false; }
    return true;
  }
  return false;
}
function assert_eq<T>(a: T, b: T): void { if (!__deep_eq(a, b)) throw new Error(\`assert_eq: \${JSON.stringify(a)} !== \${JSON.stringify(b)}\`); }
function assert_ne<T>(a: T, b: T): void { if (a === b) throw new Error(\`assert_ne: \${a} === \${b}\`); }
function assert(c: boolean): void { if (!c) throw new Error("assertion failed"); }
function unwrap_or<T>(x: T | null, d: T): T { return x !== null ? x : d; }
function __concat(a: any, b: any): any { return typeof a === "string" ? a + b : [...a, ...b]; }
function __assert_throws(fn: () => any, expectedMsg: string): void {
  try { fn(); throw new Error("Expected error but succeeded with: " + fn); }
  catch (e) { if (e instanceof Error && e.message === expectedMsg) return; throw e; }
}
// ---- End Runtime ----
`;

export function generate(program: Program): string {
  const lines: string[] = [RUNTIME];

  if (program.module && program.module.kind === "module") {
    lines.push(`// module: ${program.module.path.join(".")}`);
  }

  let hasMain = false;
  for (const decl of program.decls) {
    if (decl.kind === "fn" && decl.name === "main") hasMain = true;
    lines.push(genDecl(decl));
    lines.push("");
  }

  // Auto-generate CLI entry point if main() exists
  if (hasMain) {
    lines.push(`// ---- Entry Point ----`);
    lines.push(`try { main(["minigit", ...Deno.args]); } catch (e) { if (e instanceof Error) { eprintln(e.message); Deno.exit(1); } throw e; }`);
  }

  return lines.join("\n");
}

function genDecl(decl: Decl): string {
  switch (decl.kind) {
    case "module": return `// module: ${decl.path.join(".")}`;
    case "import": return `// import: ${decl.path.join(".")}`;
    case "type": return genTypeDecl(decl);
    case "fn": return genFnDecl(decl);
    case "trait": return `// trait ${decl.name}`;
    case "impl": return genImplDecl(decl);
    case "test": return genTestDecl(decl);
    case "strict": return `// strict ${decl.mode}`;
  }
}

function genTypeDecl(decl: Extract<Decl, { kind: "type" }>): string {
  const t = decl.type;
  if (t.kind === "record") {
    const fields = t.fields.map(f => `  ${f.name}: ${genTypeExpr(f.type)};`).join("\n");
    return `interface ${decl.name} {\n${fields}\n}`;
  }
  if (t.kind === "variant") {
    return `// variant type ${decl.name} (runtime uses tagged objects or strings)`;
  }
  if (t.kind === "newtype") {
    return `type ${decl.name} = ${genTypeExpr(t.inner)} & { readonly __brand: "${decl.name}" };`;
  }
  return `type ${decl.name} = ${genTypeExpr(t)};`;
}

function genTypeExpr(t: TypeExpr): string {
  switch (t.kind) {
    case "simple": return mapTypeName(t.name);
    case "generic": {
      if (t.name === "List") return `${genTypeExpr(t.args[0])}[]`;
      if (t.name === "Map") return `Map<${t.args.map(genTypeExpr).join(", ")}>`;
      if (t.name === "Set") return `Set<${genTypeExpr(t.args[0])}>`;
      // Result[T,E] and Option[T] are erased — just T (errors are exceptions)
      if (t.name === "Result") return genTypeExpr(t.args[0]);
      if (t.name === "Option") return `${genTypeExpr(t.args[0])} | null`;
      return `${t.name}<${t.args.map(genTypeExpr).join(", ")}>`;
    }
    case "record": {
      const fields = t.fields.map(f => `${f.name}: ${genTypeExpr(f.type)}`).join(", ");
      return `{ ${fields} }`;
    }
    case "fn": {
      const params = t.params.map((p, i) => `_${i}: ${genTypeExpr(p)}`).join(", ");
      return `(${params}) => ${genTypeExpr(t.ret)}`;
    }
    case "newtype": return genTypeExpr(t.inner);
    default: return "any";
  }
}

function mapTypeName(name: string): string {
  switch (name) {
    case "Int": return "number";
    case "Float": return "number";
    case "String": return "string";
    case "Bool": return "boolean";
    case "Unit": return "void";
    case "Path": return "string";
    default: return name;
  }
}

function genFnDecl(decl: Extract<Decl, { kind: "fn" }>): string {
  const async_ = decl.async ? "async " : "";
  const name = sanitizeName(decl.name);
  const params = decl.params
    .filter(p => p.name !== "self")
    .map(p => `${sanitizeName(p.name)}: ${genTypeExpr(p.type)}`)
    .join(", ");
  const retType = decl.returnType ? `: ${genTypeExpr(decl.returnType)}` : "";
  const body = genExpr(decl.body);

  if (decl.body.kind === "block") {
    return `${async_}function ${name}(${params})${retType} ${body}`;
  }
  if (decl.body.kind === "do_block") {
    return `${async_}function ${name}(${params})${retType} {\n${body}\n}`;
  }
  return `${async_}function ${name}(${params})${retType} {\n  return ${body};\n}`;
}

function genImplDecl(decl: Extract<Decl, { kind: "impl" }>): string {
  const lines = [`// impl ${decl.trait_} for ${decl.for_}`];
  for (const m of decl.methods) lines.push(genDecl(m));
  return lines.join("\n");
}

function genTestDecl(decl: Extract<Decl, { kind: "test" }>): string {
  const body = genExpr(decl.body);
  return `Deno.test(${JSON.stringify(decl.name)}, () => ${body});`;
}

function needsIIFE(expr: Expr): boolean {
  return expr.kind === "block" || expr.kind === "do_block";
}

function genExpr(expr: Expr, indent = 0): string {
  const ind = "  ".repeat(indent);
  switch (expr.kind) {
    case "int": {
      // Use BigInt for large integers (> MAX_SAFE_INTEGER)
      const raw = expr.raw ?? String(expr.value);
      try {
        const n = BigInt(raw);
        if (n > 9007199254740991n || n < -9007199254740991n) return `${raw}n`;
      } catch { /* not a valid bigint, fall through */ }
      return raw;
    }
    case "float": return String(expr.value);
    case "string": return JSON.stringify(expr.value);
    case "interpolated_string": {
      // Transform ${...} interpolation contents: erase try/await, resolve module names
      const processed = expr.value.replace(/\$\{([^}]*)\}/g, (_m: string, inner: string) => {
        let cleaned = inner.replace(/\btry\s+/g, "").replace(/\bawait\s+/g, "");
        // Resolve module prefixes: fs. -> __fs., string. -> __string., etc.
        cleaned = cleaned.replace(/\b(fs|string|list|int|env)\./g, (_: string, mod: string) => `__${mod}.`);
        // Resolve predicate suffixes: exists? -> exists_q, starts_with? -> starts_with_q, etc.
        cleaned = cleaned.replace(/(\w)\?(\s*\()/g, "$1_q$2");
        return "${" + cleaned + "}";
      });
      return "`" + processed.replace(/\$\{/g, "${") + "`";
    }
    case "bool": return String(expr.value);
    case "ident": return resolveIdent(expr.name);
    case "type_name": return expr.name;
    case "list": return `[${expr.elements.map(e => genExpr(e)).join(", ")}]`;
    case "record": {
      const fields = expr.fields.map(f => `${f.name}: ${genExpr(f.value)}`).join(", ");
      return `{ ${fields} }`;
    }
    case "spread_record": {
      const fields = expr.fields.map(f => `${f.name}: ${genExpr(f.value)}`).join(", ");
      return `{ ...${genExpr(expr.base)}, ${fields} }`;
    }
    case "call": return genCall(expr);
    case "member": {
      const obj = genExpr(expr.object);
      const field = sanitizeName(expr.field);
      return `${mapModule(obj)}.${field}`;
    }
    case "pipe": return genPipe(expr);
    case "if": {
      const thenStr = needsIIFE(expr.then) ? `(() => ${genExpr(expr.then)})()` : genExpr(expr.then);
      const elseStr = needsIIFE(expr.else_) ? `(() => ${genExpr(expr.else_)})()` : genExpr(expr.else_);
      return `(${genExpr(expr.cond)} ? ${thenStr} : ${elseStr})`;
    }
    case "match": return genMatch(expr);
    case "block": return genBlock(expr.stmts, expr.expr, indent);
    case "do_block": return genDoBlock(expr.stmts, expr.expr, indent);
    case "lambda": {
      const params = expr.params.map(p => p.name).join(", ");
      const body = genExpr(expr.body);
      return `((${params}) => ${body})`;
    }
    case "hole": return `null as any /* hole */`;
    case "todo": return `(() => { throw new Error(${JSON.stringify(expr.message)}); })()`;
    case "try": return genExpr(expr.expr);
    case "await": return `await ${genExpr(expr.expr)}`;
    case "binary": return genBinary(expr);
    case "unary":
      if (expr.op === "not") return `!(${genExpr(expr.operand)})`;
      return `${expr.op}${genExpr(expr.operand)}`;
    case "paren": return `(${genExpr(expr.expr)})`;
    case "placeholder": return `__placeholder__`;
    case "unit": return "undefined";
    case "none": return "null";
    case "some": return genExpr(expr.expr);
    case "ok": return genExpr(expr.expr);
    case "err": return genErr(expr.expr);
  }
}

function pascalToMessage(name: string): string {
  return name.replace(/([a-z])([A-Z])/g, "$1 $2").replace(/^./, c => c.toUpperCase()).replace(/ ./g, c => c.toLowerCase());
}

function genErr(expr: Expr): string {
  // If it's a constructor call like FileNotFound("msg"), create an Error
  if (expr.kind === "call") {
    const callee = expr.callee.kind === "type_name" ? pascalToMessage(expr.callee.name) : genExpr(expr.callee);
    const args = expr.args.map(genExpr);
    return `(() => { throw new Error(${JSON.stringify(callee)} + ": " + ${args[0] || '""'}); })()`;
  }
  if (expr.kind === "type_name") {
    const msg = pascalToMessage(expr.name);
    return `(() => { throw new Error(${JSON.stringify(msg)}); })()`;
  }
  if (expr.kind === "string") {
    return `(() => { throw new Error(${JSON.stringify(expr.value)}); })()`;
  }
  return `(() => { throw new Error(String(${genExpr(expr)})); })()`;
}

function genErrMessage(expr: Expr): string {
  if (expr.kind === "string") return JSON.stringify(expr.value);
  if (expr.kind === "call" && expr.callee.kind === "type_name") {
    return `${JSON.stringify(pascalToMessage(expr.callee.name))} + ": " + ${genExpr(expr.args[0])}`;
  }
  if (expr.kind === "type_name") return JSON.stringify(pascalToMessage(expr.name));
  return `String(${genExpr(expr)})`;
}

function genCall(expr: Extract<Expr, { kind: "call" }>): string {
  const callee = genExpr(expr.callee);
  // Special case: assert_eq(x, err(e)) or assert_eq(err(e), x)
  if (callee === "assert_eq" && expr.args.length === 2) {
    const [a, b] = expr.args;
    if (b.kind === "err") return `__assert_throws(() => ${genExpr(a)}, ${genErrMessage(b.expr)})`;
    if (a.kind === "err") return `__assert_throws(() => ${genExpr(b)}, ${genErrMessage(a.expr)})`;
  }
  const args = expr.args.map(genExpr);
  if (expr.namedArgs && expr.namedArgs.length > 0) {
    const named = expr.namedArgs.map(a => `${a.name}: ${genExpr(a.value)}`);
    if (args.length > 0) return `${callee}(${args.join(", ")}, { ${named.join(", ")} })`;
    return `${callee}({ ${named.join(", ")} })`;
  }
  return `${callee}(${args.join(", ")})`;
}

// Map almide module.function calls to runtime
function resolveIdent(name: string): string {
  const sanitized = sanitizeName(name);
  return sanitized;
}

function sanitizeName(name: string): string {
  return name.replace(/\?/g, "_q");
}

const MODULE_MAP: Record<string, string> = {
  fs: "__fs",
  string: "__string",
  list: "__list",
  int: "__int",
  env: "__env",
};

function mapModule(name: string): string {
  return MODULE_MAP[name] ?? name;
}

function genBinary(expr: Extract<Expr, { kind: "binary" }>): string {
  const left = genExpr(expr.left);
  const right = genExpr(expr.right);
  switch (expr.op) {
    case "and": return `(${left} && ${right})`;
    case "or": return `(${left} || ${right})`;
    case "==": return `__deep_eq(${left}, ${right})`;
    case "!=": return `!__deep_eq(${left}, ${right})`;
    case "++": return `__concat(${left}, ${right})`;
    case "^": return `__bigop("^", ${left}, ${right})`;
    case "*": return `__bigop("*", ${left}, ${right})`;
    case "%": return `__bigop("%", ${left}, ${right})`;
    case "/": return `Math.trunc(${left} / ${right})`;
    default: return `(${left} ${expr.op} ${right})`;
  }
}

function genPipe(expr: Extract<Expr, { kind: "pipe" }>): string {
  const left = genExpr(expr.left);
  const right = expr.right;

  if (right.kind === "call") {
    const hasPlaceholder = right.args.some(a => a.kind === "placeholder");
    if (hasPlaceholder) {
      const args = right.args.map(a => a.kind === "placeholder" ? left : genExpr(a));
      const callee = genExpr(right.callee);
      return `${callee}(${args.join(", ")})`;
    }
    const callee = genExpr(right.callee);
    const args = right.args.map(genExpr);
    if (args.length > 0) return `${callee}(${left}, ${args.join(", ")})`;
    return `${callee}(${left})`;
  }

  return `${genExpr(right)}(${left})`;
}

function genBlock(stmts: Stmt[], finalExpr: Expr | undefined, indent = 0): string {
  const ind = "  ".repeat(indent + 1);
  const lines: string[] = [];

  // Detect pattern: let x = expr; match x { ..., err(e) => ... }
  // Inline expr into match subject so try-catch in genMatch catches the throw
  if (finalExpr && finalExpr.kind === "match" && stmts.length > 0) {
    const lastStmt = stmts[stmts.length - 1];
    if (lastStmt.kind === "let" &&
        finalExpr.subject.kind === "ident" &&
        finalExpr.subject.name === lastStmt.name &&
        finalExpr.arms.some(a => a.pattern.kind === "err")) {
      for (let i = 0; i < stmts.length - 1; i++) {
        lines.push(ind + genStmt(stmts[i], indent + 1));
      }
      const inlinedExpr: Expr = { ...finalExpr, subject: lastStmt.value };
      lines.push(ind + "return " + genExpr(inlinedExpr) + ";");
      return `{\n${lines.join("\n")}\n${"  ".repeat(indent)}}`;
    }
  }

  for (const stmt of stmts) {
    lines.push(ind + genStmt(stmt, indent + 1));
  }
  if (finalExpr) {
    if (finalExpr.kind === "do_block") {
      // do_block generates a while loop (statement), don't use return
      lines.push(ind + genDoBlock(finalExpr.stmts, finalExpr.expr, indent + 1));
    } else {
      lines.push(ind + "return " + genExpr(finalExpr) + ";");
    }
  }
  return `{\n${lines.join("\n")}\n${"  ".repeat(indent)}}`;
}

function isUnitExpr(e: Expr): boolean {
  if (e.kind === "unit") return true;
  // ok(()) => some(unit) => genExpr returns "undefined"
  if (e.kind === "ok" && e.expr.kind === "unit") return true;
  if (e.kind === "some" && e.expr.kind === "unit") return true;
  return false;
}

function genDoBlock(stmts: Stmt[], finalExpr: Expr | undefined, indent = 0): string {
  // Check if this is a loop (has any guard) or auto-propagation block
  const hasGuard = stmts.some(s => s.kind === "guard");
  const ind = "  ".repeat(indent + 1);
  const lines: string[] = [];
  for (const stmt of stmts) {
    if (hasGuard && stmt.kind === "guard") {
      const cond = genExpr(stmt.cond);
      if (isUnitExpr(stmt.else_)) {
        lines.push(ind + `if (!(${cond})) { break; }`);
      } else {
        lines.push(ind + `if (!(${cond})) { return ${genExpr(stmt.else_)}; }`);
      }
    } else {
      lines.push(ind + genStmt(stmt, indent + 1));
    }
  }
  if (hasGuard) {
    if (finalExpr) {
      lines.push(ind + genExpr(finalExpr) + ";");
    }
    return `while (true) {\n${lines.join("\n")}\n${"  ".repeat(indent)}}`;
  }
  // Auto-propagation block: just a block, return final expr
  if (finalExpr) {
    lines.push(ind + `return ${genExpr(finalExpr)};`);
  }
  return `{\n${lines.join("\n")}\n${"  ".repeat(indent)}}`;
}

function genStmt(stmt: Stmt, indent = 0): string {
  switch (stmt.kind) {
    case "let":
      return `const ${sanitizeName(stmt.name)} = ${genExpr(stmt.value)};`;
    case "let_destructure":
      return `const { ${stmt.fields.join(", ")} } = ${genExpr(stmt.value)};`;
    case "var":
      return `let ${sanitizeName(stmt.name)} = ${genExpr(stmt.value)};`;
    case "assign":
      return `${sanitizeName(stmt.name)} = ${genExpr(stmt.value)};`;
    case "guard": {
      const cond = genExpr(stmt.cond);
      const else_ = stmt.else_;
      // If else is a block, inline the statements
      if (else_.kind === "block" || else_.kind === "do_block") {
        const stmts = (else_.kind === "block" || else_.kind === "do_block")
          ? else_.stmts.map(s => "  " + genStmt(s)).join("\n")
          : "";
        const finalExpr = else_.expr ? `  return ${genExpr(else_.expr)};` : "";
        const body = [stmts, finalExpr].filter(Boolean).join("\n");
        return `if (!(${cond})) {\n${body}\n}`;
      }
      return `if (!(${cond})) { return ${genExpr(else_)}; }`;
    }
    case "expr":
      return genExpr(stmt.expr) + ";";
  }
}

function genMatch(expr: Extract<Expr, { kind: "match" }>): string {
  const subject = genExpr(expr.subject);
  const tmp = `__m`;
  // Check if any arm has an err pattern — if so, wrap subject in try-catch
  const errArm = expr.arms.find(a => a.pattern.kind === "err");
  if (errArm) {
    // Result erasure: subject may throw (err). Wrap only subject eval in try-catch,
    // then process ok arms outside try to avoid catching their throws.
    const okArms = expr.arms.filter(a => a.pattern.kind !== "err");
    const errInner = errArm.pattern.kind === "err" ? errArm.pattern.inner : null;
    const errBodyStr = (errArm.body.kind === "block" || errArm.body.kind === "do_block")
      ? `(() => ${genExpr(errArm.body)})()`
      : genExpr(errArm.body);
    const errBinding = (errInner && errInner.kind === "ident") ? errInner.name : null;
    const catchReturn = errBinding
      ? `const ${errBinding} = __e instanceof Error ? __e.message : String(__e); return ${errBodyStr};`
      : `return ${errBodyStr};`;
    const lines = [`(() => { let ${tmp}; try { ${tmp} = ${subject}; } catch (__e) { ${catchReturn} }`];
    for (const arm of okArms) {
      const { cond, bindings } = genPatternCond(tmp, arm.pattern);
      const bindStr = bindings.map(b => `    const ${b.name} = ${b.expr};`).join("\n");
      const bodyStr = (arm.body.kind === "block" || arm.body.kind === "do_block")
        ? `(() => ${genExpr(arm.body)})()`
        : genExpr(arm.body);
      if (arm.guard) {
        const guardStr = genExpr(arm.guard);
        if (bindStr) {
          lines.push(`  { ${bindStr}\n    if (${cond} && ${guardStr}) return ${bodyStr}; }`);
        } else {
          lines.push(`  if (${cond} && ${guardStr}) return ${bodyStr};`);
        }
      } else {
        if (bindStr) {
          lines.push(`  if (${cond}) { ${bindStr}\n    return ${bodyStr}; }`);
        } else {
          lines.push(`  if (${cond}) return ${bodyStr};`);
        }
      }
    }
    lines.push(`  throw new Error("match exhausted");`);
    lines.push(`})()`);
    return lines.join("\n");
  }
  const lines = [`((${tmp}) => {`];
  for (const arm of expr.arms) {
    const { cond, bindings } = genPatternCond(tmp, arm.pattern);
    const bindStr = bindings.map(b => `    const ${b.name} = ${b.expr};`).join("\n");
    const bodyStr = (arm.body.kind === "block" || arm.body.kind === "do_block")
      ? `(() => ${genExpr(arm.body)})()`
      : genExpr(arm.body);
    if (arm.guard) {
      const guardStr = genExpr(arm.guard);
      if (bindStr) {
        lines.push(`  { ${bindStr}\n    if (${cond} && ${guardStr}) return ${bodyStr}; }`);
      } else {
        lines.push(`  if (${cond} && ${guardStr}) return ${bodyStr};`);
      }
    } else {
      if (bindStr) {
        lines.push(`  if (${cond}) { ${bindStr}\n    return ${bodyStr}; }`);
      } else {
        lines.push(`  if (${cond}) return ${bodyStr};`);
      }
    }
  }
  lines.push(`  throw new Error("match exhausted");`);
  lines.push(`})(${subject})`);
  return lines.join("\n");
}

interface Binding { name: string; expr: string; }

function genPatternCond(expr: string, pattern: Pattern): { cond: string; bindings: Binding[] } {
  switch (pattern.kind) {
    case "wildcard": return { cond: "true", bindings: [] };
    case "ident": return { cond: "true", bindings: [{ name: pattern.name, expr }] };
    case "literal": return { cond: `${expr} === ${genExpr(pattern.value)}`, bindings: [] };
    case "none": return { cond: `${expr} === null`, bindings: [] };
    case "some": {
      const inner = genPatternCond(expr, pattern.inner);
      return { cond: `${expr} !== null${inner.cond !== "true" ? ` && ${inner.cond}` : ""}`, bindings: inner.bindings };
    }
    case "ok": {
      const inner = genPatternCond(expr, pattern.inner);
      return { cond: inner.cond, bindings: inner.bindings };
    }
    case "err": return { cond: `false`, bindings: [] };
    case "constructor": {
      if (pattern.args.length === 0) return { cond: `${expr}?.tag === "${pattern.name}"`, bindings: [] };
      const bindings: Binding[] = [];
      const conds = [`${expr}?.tag === "${pattern.name}"`];
      pattern.args.forEach((arg, i) => {
        const sub = genPatternCond(`${expr}._${i}`, arg);
        if (sub.cond !== "true") conds.push(sub.cond);
        bindings.push(...sub.bindings);
      });
      return { cond: conds.join(" && "), bindings };
    }
    case "record_pattern": {
      const bindings: Binding[] = [];
      const conds = [`${expr}?.tag === "${pattern.name}"`];
      for (const f of pattern.fields) {
        if (f.pattern) {
          const sub = genPatternCond(`${expr}.${f.name}`, f.pattern);
          if (sub.cond !== "true") conds.push(sub.cond);
          bindings.push(...sub.bindings);
        } else {
          bindings.push({ name: f.name, expr: `${expr}.${f.name}` });
        }
      }
      return { cond: conds.join(" && "), bindings };
    }
  }
}
