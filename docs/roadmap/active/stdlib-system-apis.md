<!-- description: Add HTTP client, process spawn, and signal stdlib modules -->

# Stdlib System APIs

porta の Rust ブリッジを Almide に移行するために必要な stdlib 追加。

## 1. HTTP Client

```almide
import http

let resp = http.get("https://api.example.com/data")!
// resp: {status: Int, body: String, headers: Map[String, String]}

let resp = http.post("https://api.example.com", 
  headers: [("Content-Type", "application/json")],
  body: "{\"key\": \"value\"}")!
```

Rust 実装: `reqwest::blocking` をラップ。`process.exec` と同じパターンで `@impure` な stdlib 関数として追加。

## 2. Process Spawn (non-blocking)

```almide
import process

// 既存: exec は完了を待つ
let output = process.exec("ls", ["-la"])!

// 新規: spawn は待たない、PID を返す
let pid = process.spawn("node", ["server.js"])!
// pid: Int

// 新規: ステータスチェック
let running = process.is_alive(pid)
// running: Bool
```

Rust 実装: `std::process::Command::spawn()` → child.id() を返す。

## 3. Signal

```almide
import process

process.kill(pid, 15)!   // SIGTERM
process.kill(pid, 9)!    // SIGKILL
```

Rust 実装: `libc::kill(pid, signal)`

## 4. Environment Variable Read

```almide
import process

let home = process.env("HOME") ?? "/tmp"
let api_key = process.env("ANTHROPIC_API_KEY") ?? ""
```

Rust 実装: `std::env::var(key).ok()`

現状 `process.args()` はあるが `process.env()` がない。

## Priority

1. `process.env` — 最も基本的。多くの場面で必要
2. `process.spawn` + `process.kill` — デーモン管理
3. `http` module — porta の HTTP bridge 移行
