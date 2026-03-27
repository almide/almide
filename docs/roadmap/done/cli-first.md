<!-- description: Enable comfortable CLI tool authoring with run, build, and WASM targets -->
<!-- done: 2026-03-17 -->
# CLI-First

## Vision

Write practical CLI tools in Almide, with `almide run` for instant execution during development and `almide build` for producing a single native binary for distribution. Cross-target consistency between TS and Rust paths is guaranteed by @extern + glue.

```
Dev:     almide run app.almd           → TS → Deno instant execution (like go run)
Dist:    almide build app.almd -o app  → Rust → single native binary (like go build)
WASM:    almide build app.almd --target wasm → Rust → .wasm
```

---

## Current Capabilities (v0.5.13)

### CLI tools you can write

| Category | Available features | Examples |
|---|---|---|
| **Arg parsing** | `args.positional()`, `args.flag()`, `args.option()` | `mytool --verbose -o out.json input.csv` |
| **File processing** | `fs.read_text/write/read_lines/glob/walk` | File conversion, log aggregation, static site generation |
| **Structured data** | `json.parse/stringify`, `csv.parse_with_header`, `toml.parse` | JSON/CSV/TOML conversion tools |
| **Path operations** | `path.join/dirname/basename/extension/normalize` | Cross-platform path handling |
| **Regex** | `regex.is_match/find_all/replace/captures` | Text search/replace tools |
| **Process execution** | `process.exec/exec_status/exec_with_stdin` | Build scripts, task runners |
| **HTTP** | `http.get/post/get_json`, `http.serve` | API clients, webhook servers |
| **Environment** | `env.get/cwd/os`, `process.exit` | Environment-dependent branching, exit codes |
| **Error handling** | `Result[T, E]`, `effect fn`, `do` block | Proper reporting of missing files, parse failures |

**Demonstrated**: exercises/ contains working CLI-like programs including config-merger (317 lines), pipeline, isbn-verifier, etc.

### CLI tools you can't write

| Gap | Impact | Solution |
|---|---|---|
| **Terminal decoration** | No colored output, progress bars, spinners | Extend `term` module (ANSI escapes) |
| **Async / concurrency** | Can't process files in parallel, can't call multiple APIs simultaneously | Structured concurrency (existing roadmap) |
| **Interactive input** | Can't do Y/N confirmations, menu selection | Add `io.prompt()`, `io.confirm()` |
| **Package dependencies** | Can't use external libraries | Package registry (can defer) |
| **DB connections** | Can't directly use SQLite/PostgreSQL | Wrap Rust crates via @extern |
| **Signal handling** | Can't handle Ctrl+C | Add `process.on_signal()` |

---

## Goal: 5 CLI tools can be written

The goal is for the following CLI tools to be naturally writable in Almide.

### 1. File conversion tool (can write now)

```almide
// csv2json: CSV → JSON conversion
effect fn main() =
  let args = args.positional()
  let input = args.get(0).unwrap_or("input.csv")
  let content = fs.read_text(input)
  let rows = csv.parse_with_header(content)
  let json_out = json.stringify_pretty(json.from(rows))
  println(json_out)
```

**Status: Can write now ✅**

### 2. Project initialization tool (nearly writable)

```almide
// init: Create directory structure
effect fn main() =
  let name = args.option_or("name", "my-project")
  fs.mkdir_p("{name}/src")
  fs.mkdir_p("{name}/tests")
  fs.write("{name}/almide.toml", "[project]\nname = \"{name}\"\nversion = \"0.1.0\"")
  fs.write("{name}/src/main.almd", "fn main() =\n  println(\"Hello, {name}!\")")
  println("Created project: {name}")
```

**Status: Can write now ✅**

### 3. API client (needs async)

```almide
// ghstats: Fetch stats from GitHub API
effect fn main() = do {
  let token = env.get("GITHUB_TOKEN").unwrap_or("")
  let repos = args.positional()
  // Want to fetch all repo info in parallel
  async let results = repos.map((repo) =>
    http.get_json("https://api.github.com/repos/{repo}")
  )
  for result in await results {
    let name = json.get_string(result, "full_name")
    let stars = json.get_int(result, "stargazers_count")
    println("{name}: {stars} stars")
  }
}
```

**Status: Needs async 🔶** — sequential sync execution works today

### 4. File search tool (nearly writable)

```almide
// find: Search files by pattern and grep contents
effect fn main() = do {
  let pattern = args.positional().get(0).unwrap_or("*.almd")
  let query = args.option("grep")
  let files = fs.glob(pattern)
  for file in files {
    match query {
      some(q) => {
        let content = fs.read_text(file)
        let lines = string.lines(content)
        for (i, line) in lines.enumerate() {
          if string.contains(line, q) {
            println("{file}:{i + 1}: {line}")
          }
        }
      }
      none => println(file)
    }
  }
}
```

**Status: Nearly writable ✅** — fully functional with `enumerate` (or manual counter as workaround)

### 5. Build script / task runner (wants terminal decoration)

```almide
// tasks.almd: Define and run project tasks
effect fn main() = do {
  let task = args.positional().get(0).unwrap_or("help")
  match task {
    "build" => {
      println("[build] Compiling...")     // would like colored output
      let result = process.exec("cargo", ["build", "--release"])
      println("[build] Done: {result}")
    }
    "test" => {
      let result = process.exec_status("almide", ["test"])
      process.exit(result.code)
    }
    "clean" => {
      fs.remove_all("target")
      println("[clean] Removed target/")
    }
    _ => {
      println("Available tasks: build, test, clean")
    }
  }
}
```

**Status: Writable (no color) ✅** — would like colored output via term module extension

---

## Gap Analysis and Priorities

### Must Have (required to achieve CLI goal)

| Feature | Current state | Action | Existing roadmap |
|---|---|---|---|
| **@extern + glue** | Design complete | Implement Steps 1-3 | [Runtime Architecture Reform](stdlib-self-hosted-redesign.md) |
| **? suffix removal** | ✅ Complete | — | [API Surface Reform](stdlib-verb-system.md) Step 1 |
| **TS unified Result representation** | Design complete (glue) | Implement glue runtime | [Runtime Architecture Reform](stdlib-self-hosted-redesign.md) |

### Should Have (improves CLI experience)

| Feature | Current state | Action | Existing roadmap |
|---|---|---|---|
| **Terminal color/style** | `term` module minimal | Add ANSI escape wrappers | None (new) |
| **Async / parallel** | Design complete | Implement Phase 0-1 | [Platform Async](platform-async.md), [Structured Concurrency](structured-concurrency.md) |
| **Interactive input** | `io.read_line` only | Add `io.prompt()`, `io.confirm()` | None (new) |
| **Verb System** | Design complete | Steps 2-5 | [API Surface Reform](stdlib-verb-system.md) |

### Nice to Have (benefits use cases beyond CLI)

| Feature | Action | Existing roadmap |
|---|---|---|
| DB connections | Wrap SQLite/PostgreSQL via @extern | None |
| Signal handling | `process.on_signal("SIGINT", handler)` | None |
| Package registry | External dependency resolution | on-hold |

---

## Dev / Distribution Model

### Development: Fast iteration via TS path

```bash
almide run app.almd              # TS → Deno instant execution
almide run app.almd arg1 arg2    # Args can be passed too (existing feature)
```

- No Rust compilation needed, instant execution
- Async maps directly to JS async/await (easy to verify)
- Error messages give immediate feedback

### Distribution: Native binary

```bash
almide build app.almd -o mytool          # Single binary
almide build app.almd --target wasm      # WASM
```

- `./mytool` just works. No runtime dependency
- Same distribution experience as Go / Rust CLI tools
- Async is absorbed by tokio (transparent to the user)

### Async verification strategy

**Verify on TS path first, Rust path later**:

```
Almide async let → JS Promise.all    # Nearly 1:1 mapping. Lock down semantics here first
Almide async let → tokio::spawn      # Convert to Rust after semantics are settled
```

The TS path requires less compiler work (JS has async natively, so it's just syntax transformation). The Rust path has tokio's Send + 'static constraints and other complications, so it's safer to tackle it after semantics are settled.

---

## Implementation Steps

### Step 1: Showcase CLI tools (immediate)

Build 2-3 CLI tools using only existing features and place in exercises/ or examples/. Demonstrate that "you can write CLI tools in Almide."

Candidates:
- `csv2json` — CSV → JSON conversion (args + fs + csv + json)
- `project-init` — Project initialization (args + fs + path)
- `grep-lite` — Text search in files (args + fs + glob + string/regex)

### Step 2: Terminal decoration (extend term module)

Add ANSI escape code wrappers to `stdlib/term.almd`:

```almide
term.red("Error: file not found")
term.green("✓ Done")
term.bold("Building...")
term.dim("(3 files)")
```

Implementable in pure Almide (ANSI escapes are string operations). No @extern needed.

### Step 3: Interactive input

```almide
let name = io.prompt("Project name: ")
let confirm = io.confirm("Create {name}?")   // Y/n
```

Can be built on top of `io.read_line` in .almd.

### Step 4: Async (follow existing roadmap)

Implement [Platform Async](platform-async.md) / [Structured Concurrency](structured-concurrency.md) Phase 0-1 in CLI context.

Verification tool: Run `ghstats` (parallel multiple API calls) on TS path.

### Step 5: @extern + glue (follow existing roadmap)

Implement [Runtime Architecture Reform](stdlib-self-hosted-redesign.md) Steps 1-3. Guarantee CLI tools work on both TS/Rust paths.

---

## Success Criteria

- 3+ practical CLI tools working in exercises/
- `almide run tool.almd` provides instant execution via TS path
- `almide build tool.almd -o tool` produces a native binary
- Same .almd file produces identical results on both TS and Rust paths
- Terminal colored output works
- `io.prompt()` / `io.confirm()` provide interactive input

## Related

- [Runtime Architecture Reform](stdlib-self-hosted-redesign.md) — @extern + glue
- [API Surface Reform](stdlib-verb-system.md) — verb system
- [Platform Async](platform-async.md) — transparent async
- [Structured Concurrency](structured-concurrency.md) — async let
