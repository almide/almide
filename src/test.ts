import { Lexer } from "./lexer.ts";
import { Parser, ParseError } from "./parser.ts";

let passed = 0;
let failed = 0;

function test(name: string, src: string, expectFail = false) {
  try {
    const lexer = new Lexer(src);
    const tokens = lexer.tokenize();
    const parser = new Parser(tokens);
    const ast = parser.parse();

    if (expectFail) {
      console.log(`  FAIL: ${name} (expected failure but succeeded)`);
      failed++;
    } else {
      console.log(`  OK: ${name}`);
      passed++;
    }
    return ast;
  } catch (e) {
    if (expectFail) {
      console.log(`  OK: ${name} (expected failure: ${(e as Error).message.slice(0, 60)})`);
      passed++;
    } else {
      console.log(`  FAIL: ${name}`);
      console.log(`    ${(e as Error).message}`);
      failed++;
    }
    return null;
  }
}

// ============================================================
console.log("\n=== 1. Module & Import ===");

test("module declaration", `
module repo.index
`);

test("simple imports", `
module repo.config
import fs
import json
`);

test("selective import", `
module app
import collections.{List, Map}
`);

// ============================================================
console.log("\n=== 2. Type Declarations ===");

test("record type", `
module t
type User = {
  id: Int,
  name: String,
}
`);

test("variant type with pipe", `
module t
type Token =
  | Word(String)
  | Number(Int)
  | Eof
`);

test("variant with record fields", `
module t
type Shape =
  | Circle(Float)
  | Rect{ width: Float, height: Float }
  | Point
`);

test("inline variant", `
module t
type ConfigError = Io(IoError) | Parse(ParseError)
`);

test("generic type", `
module t
type Pair[A, B] = {
  first: A,
  second: B,
}
`);

test("variant with deriving", `
module t
type ConfigError =
  | Io(IoError)
  | Parse(ParseError)
  | Decode(DecodeError)
  deriving From
`);

test("deriving multiple traits", `
module t
type Color =
  | Red
  | Green
  | Blue
  deriving Eq, Show
`);

// ============================================================
console.log("\n=== 3. Function Declarations ===");

test("simple function", `
module t
fn add(x: Int, y: Int) -> Int = x + y
`);

test("function with hole", `
module t
fn parse(text: String) -> Result[Ast, ParseError] = _
`);

test("predicate function (?)", `
module t
fn empty?(xs: List[Int]) -> Bool = xs.len == 0
`);

test("effect function", `
module t
effect fn read_text(path: Path) -> Result[String, IoError] = _
`);

test("effect function with block body", `
module t
effect fn add(index: Index, file: Path) -> Result[Index, IoError] =
  if tracked?(index, file) then ok(index)
  else do {
    let bytes = try read(file)
    let id = hash(bytes)
    ok(index.insert(file, id))
  }
`);

test("function with todo", `
module t
fn optimize(ast: Ast) -> Ast = todo("implement constant folding")
`);

// ============================================================
console.log("\n=== 4. Expressions ===");

test("if expression", `
module t
fn f(x: Int) -> String =
  if x > 0 then "positive" else "non-positive"
`);

test("else-if chaining", `
module t
fn classify(x: Int) -> String =
  if x > 0 then "positive"
  else if x == 0 then "zero"
  else "negative"
`);

test("match expression", `
module t
fn show(r: Result[Int, String]) -> String =
  match r {
    ok(value) => "ok",
    err(error) => "err",
  }
`);

test("match with guard", `
module t
fn classify(x: Int) -> String =
  match x {
    n if n > 0 => "positive",
    n if n == 0 => "zero",
    _ => "negative",
  }
`);

test("match with variant and guard", `
module t
fn process(r: Result[Int, String]) -> String =
  match r {
    ok(v) if v > 100 => "big",
    ok(v) => "small",
    err(e) => e,
  }
`);

test("match with variant pattern", `
module t
fn area(s: Shape) -> Float =
  match s {
    Circle(r) => 3.14 * r * r,
    Rect{ width, height } => width * height,
    Point => 0.0,
  }
`);

test("lambda expression", `
module t
fn f(items: List[Int]) -> List[Int] =
  items.map(fn(x) => x + 1)
`);

test("chained method calls", `
module t
fn f(items: List[Int]) -> Int =
  items
    .filter(fn(x) => x > 0)
    .map(fn(x) => x * 2)
    .fold(0, fn(acc, x) => acc + x)
`);

test("pipe expression", `
module t
fn f(text: String) -> List[String] =
  text
    |> string.trim
    |> string.split(",")
`);

test("record literal", `
module t
fn f() -> User = { name: "alice", age: 30 }
`);

test("spread record", `
module t
fn f(user: User) -> User = { ...user, name: "bob" }
`);

test("list literal", `
module t
fn f() -> List[Int] = [1, 2, 3]
`);

test("string interpolation", `
module t
fn greet(name: String) -> String = "hello \${name}"
`);

test("nested expressions", `
module t
fn f(x: Int, y: Int) -> Bool = (x + y) * 2 > 10 and x != 0
`);

test("none and some", `
module t
fn f(x: Option[Int]) -> Int =
  match x {
    some(v) => v,
    none => 0,
  }
`);

// ============================================================
console.log("\n=== 5. Named Arguments ===");

test("call with named args", `
module t
fn f() -> User =
  create_user(name: "alice", age: 30, active: true)
`);

test("mixed positional and named args", `
module t
fn f() -> User =
  create_user("alice", age: 30, active: true)
`);

test("named args with expressions", `
module t
fn f(x: Int) -> Result[Int, Error] =
  compute(input: x + 1, retry: 3)
`);

test("type constructor with named args", `
module t
fn f() -> Config =
  Config(root: "/repo", bare: false)
`);

// ============================================================
console.log("\n=== 6. Statements ===");

test("let and var", `
module t
fn f() -> Int = {
  let x = 1
  var y = 2
  y = y + x
  y
}
`);

test("let with type annotation", `
module t
fn f() -> String = {
  let name: String = "hello"
  name
}
`);

// ============================================================
console.log("\n=== 7. Do block (Result monad) ===");

test("do block", `
module t
effect fn load(path: Path) -> Result[Config, ConfigError] =
  do {
    let text = fs.read_text(path)
    let raw = json.parse(text)
    decode(raw)
  }
`);

// ============================================================
console.log("\n=== 8. Try expression ===");

test("try expression", `
module t
effect fn f(path: Path) -> Result[String, IoError] = {
  let text = try fs.read_text(path)
  ok(text)
}
`);

// ============================================================
console.log("\n=== 9. Async / Await ===");

test("async function", `
module t
async fn fetch(url: String) -> Result[String, HttpError] = _
`);

test("await expression", `
module t
async fn load(url: String) -> Result[Config, AppError] = {
  let text = await fetch(url)
  ok(parse(text))
}
`);

test("async with do block", `
module t
async fn load(url: String) -> Result[Config, AppError] =
  do {
    let text = await fetch(url)
    let config = parse(text)
    config
  }
`);

test("async with parallel", `
module t
async fn load_all(urls: List[String]) -> Result[List[String], HttpError] =
  do {
    await parallel(urls.map(fn(url) => fetch(url)))
  }
`);

test("async effect fn (redundant but allowed)", `
module t
async effect fn fetch_and_save(url: String, path: Path) -> Result[Unit, AppError] =
  do {
    let text = await fetch(url)
    fs.write(path, text)
  }
`);

// ============================================================
console.log("\n=== 10. Trait & Impl ===");

test("trait declaration", `
module t
trait Iterable[T] {
  fn map[U](self, f: fn(T) -> U) -> List[U]
  fn filter(self, f: fn(T) -> Bool) -> List[T]
}
`);

test("impl declaration", `
module t
impl From[IoError] for ConfigError {
  fn from(e: IoError) -> ConfigError = Io(e)
}
`);

test("trait with effect method", `
module t
trait Storage[T] {
  effect fn save(self, item: T) -> Result[Unit, IoError]
  effect fn load(self, id: String) -> Result[T, IoError]
}
`);

test("trait with async method", `
module t
trait HttpClient {
  async fn get(self, url: String) -> Result[String, HttpError]
  async fn post(self, url: String, body: String) -> Result[String, HttpError]
}
`);

// ============================================================
console.log("\n=== 11. Test Declarations ===");

test("test declaration", `
module t
fn add(x: Int, y: Int) -> Int = x + y

test "addition works" {
  let result = add(1, 2)
  assert_eq(result, 3)
}
`);

test("test with match and guard", `
module t
fn classify(x: Int) -> String =
  match x {
    n if n > 0 => "positive",
    _ => "non-positive",
  }

test "classify positive" {
  assert_eq(classify(5), "positive")
}

test "classify zero" {
  assert_eq(classify(0), "non-positive")
}
`);

// ============================================================
console.log("\n=== 12. Complete Sample ===");

test("complete sample: repo.config", `
module repo.config

import fs
import json
import http

type Config = {
  root: Path,
  bare: Bool,
  description: String,
}

type ConfigError =
  | Io(IoError)
  | Parse(ParseError)
  | Decode(DecodeError)
  deriving From

fn exists?(path: Path) -> Bool =
  fs.exists?(path)

effect fn load(path: Path) -> Result[Config, ConfigError] =
  do {
    let text = fs.read_text(path)
    let raw = json.parse(text)
    decode(raw)
  }

async fn fetch_config(url: String) -> Result[Config, ConfigError] =
  do {
    let text = await http.get(url)
    let raw = json.parse(text)
    decode(raw)
  }

fn with_description(config: Config, desc: String) -> Config =
  { ...config, description: desc }

fn default_config(root: Path) -> Config =
  { root: root, bare: false, description: "" }

fn summary(config: Config) -> String =
  "root=\${config.root}, bare=\${config.bare}"

fn classify_config(config: Config) -> String =
  match config {
    c if c.bare => "bare repository",
    c if c.description == "" => "no description",
    _ => config.description,
  }

test "default config has empty description" {
  let cfg = default_config("/repo")
  assert_eq(cfg.description, "")
}

test "with_description updates description" {
  let cfg = default_config("/repo")
  let updated = with_description(cfg, "my repo")
  assert_eq(updated.description, "my repo")
}

test "classify bare repo" {
  let cfg = { root: "/repo", bare: true, description: "" }
  assert_eq(classify_config(cfg), "bare repository")
}
`);

// ============================================================
console.log("\n=== 13. Destructuring ===");

test("let destructure record", `
module t
fn f(user: User) -> String = {
  let { name, age } = user
  name
}
`);

test("let destructure in do block", `
module t
effect fn f(path: Path) -> Result[Config, Error] =
  do {
    let raw = load(path)
    let { host, port } = parse(raw)
    ok(connect(host, port))
  }
`);

// ============================================================
console.log("\n=== 14. Newtype ===");

test("newtype declaration", `
module t
type UserId = newtype Int
`);

test("newtype with generic", `
module t
type Email = newtype String
`);

// ============================================================
console.log("\n=== 15. Placeholder in pipe ===");

test("pipe with placeholder", `
module t
fn f(text: String) -> List[String] =
  text |> split(_, ",")
`);

test("pipe with placeholder in chain", `
module t
fn f(xs: List[Int]) -> List[Int] =
  xs
    |> filter(_, fn(x) => x > 0)
    |> map(_, fn(x) => x * 2)
`);

test("placeholder in regular call", `
module t
fn f(xs: List[Int]) -> List[Int] =
  map(_, fn(x) => x + 1)
`);

// ============================================================
console.log("\n=== 16. Guard ===");

test("guard in block", `
module t
fn f(x: Int) -> Int = {
  guard x > 0 else err("negative")
  x * 2
}
`);

test("guard with function call", `
module t
effect fn f(path: Path) -> Result[Config, Error] = {
  guard fs.exists?(path) else err(NotFound(path))
  let text = try fs.read_text(path)
  ok(parse(text))
}
`);

// ============================================================
console.log("\n=== 17. Should-fail cases ===");

test("wildcard import should fail", `
module t
import fs.*
`, true);

// ============================================================
console.log("\n=== Results ===");
console.log(`Passed: ${passed}, Failed: ${failed}`);
if (failed > 0) {
  Deno.exit(1);
}
