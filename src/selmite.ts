// Selmite compiler CLI
// Usage: deno run --allow-read src/selmite.ts <file.selm>

import { Lexer } from "./lexer.ts";
import { Parser } from "./parser.ts";
import { generate } from "./codegen.ts";

const file = Deno.args[0];
if (!file) {
  console.error("Usage: selmite <file.selm>");
  Deno.exit(1);
}

const src = await Deno.readTextFile(file);
const lexer = new Lexer(src);
const tokens = lexer.tokenize();
const parser = new Parser(tokens);
const ast = parser.parse();
const ts = generate(ast);
console.log(ts);
