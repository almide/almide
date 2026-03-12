# Rainbow FFI [ON HOLD]

## Thesis

Almide はコンパイルターゲットを複数持つ。この特性を逆手に取り、各ターゲットのエコシステムをそのまま FFI で吸収する。1つの言語から Python, Ruby, JavaScript, Rust, Swift, Kotlin, Erlang のライブラリを呼べる — これが Rainbow FFI。

エコシステムが無い弱点を「全言語のエコシステムが使える」という強みに反転させる。

## Architecture

```
                    ┌─ Rust FFI ──→ crates.io (300k+ crates)
                    ├─ C ABI ────→ SQLite, OpenSSL, libcurl, ...
almide source ──→   ├─ JS FFI ───→ npm (2M+ packages)
                    ├─ Python ───→ PyPI (500k+ packages) via PyO3/WASM
                    ├─ Ruby ─────→ RubyGems via native ext
                    ├─ Swift ────→ Apple frameworks via Rust bridging
                    ├─ Kotlin ───→ JVM via GraalVM/JNI
                    └─ Erlang ───→ BEAM via NIF/Port
```

## Syntax

統一された `extern` 宣言で全ターゲットの FFI を記述:

```almide
// Rust crate を直接呼ぶ (--target rust)
extern "rust" fn sha256(data: String) -> String
  from "sha2"

// npm パッケージを呼ぶ (--target ts/js)
extern "js" fn marked(markdown: String) -> String
  from "marked"

// Python ライブラリを呼ぶ (--target wasm+python)
extern "python" fn sentiment(text: String) -> Float
  from "textblob"

// C ABI (via Rust unsafe, 全ターゲット共通)
extern "c" fn sqlite3_open(filename: String) -> Int
  from "sqlite3"
```

### クロスターゲット FFI

同じ `.almd` ファイルに複数ターゲットの extern を書ける。コンパイル時にターゲットに応じて適切な実装が選択される:

```almide
// ターゲットに応じて実装が切り替わる
extern "rust" fn hash(data: String) -> String
  from "sha2"

extern "js" fn hash(data: String) -> String
  from "crypto-js"

// 利用側は同じ
fn main() = {
  let h = hash("hello")  // Rust → sha2, JS → crypto-js
}
```

## Implementation Phases

### Phase 1: Rust FFI (P0)
Almide の主要ターゲットが Rust なので最も自然。
- [ ] `extern "rust"` 宣言の構文追加
- [ ] `from "crate_name"` で Cargo.toml に依存追加
- [ ] 型マッピング: `String ↔ String`, `Int ↔ i64`, `List[T] ↔ Vec<T>`, `Option[T] ↔ Option<T>`
- [ ] `Result[T, E]` の自動変換
- [ ] `almide.toml` の `[dependencies]` セクション

### Phase 2: JavaScript FFI (P0)
TS/JS ターゲットでは npm エコシステムが巨大。
- [ ] `extern "js"` 宣言
- [ ] `from "package"` で import 文生成
- [ ] Promise → Result 自動変換
- [ ] TypeScript 型定義からの自動バインディング生成

### Phase 3: C ABI (P1)
Rust の unsafe FFI 経由で C ライブラリを呼ぶ。
- [ ] `extern "c"` 宣言
- [ ] ポインタ型は Almide 側に露出しない（内部で安全にラップ）
- [ ] `almide.toml` の `[native-deps]` で pkg-config / vcpkg 連携

### Phase 4: Python Interop (P2)
PyO3 経由 or WASM + Python runtime。
- [ ] `extern "python"` 宣言
- [ ] PyO3 バインディング自動生成（Rust ターゲット時）
- [ ] WASM + Pyodide 統合（Web ターゲット時）

### Phase 5: Exotic Targets (P3)
Swift, Kotlin, Ruby, Erlang — 各言語の FFI メカニズムを活用。
- [ ] Swift: Rust → C ABI → Swift bridging header
- [ ] Kotlin: GraalVM native-image or JNI via Rust
- [ ] Ruby: native extension via Rust (rb-sys)
- [ ] Erlang: NIF (Rustler) or Port protocol

## MoonBit との差別化

MoonBit は Python FFI を「エコシステム継承」として推進している。Almide は Rust + JS の二刀流をベースに、より多くの言語エコシステムを横断的に吸収する。

MoonBit: Python FFI → エコシステム継承
Almide: Rainbow FFI → 全言語エコシステム吸収

## 前提条件

- Phase 1-2 は現在のコンパイラアーキテクチャで実装可能
- Phase 3 は Rust ターゲットの unsafe ブロック生成が必要
- Phase 4-5 は各ランタイムとの統合が必要で、実装コストが高い
- `almide forge` との連携: FFI バインディングも自動生成可能にする

## Priority

Rust FFI ≧ JS FFI > C ABI > Python > exotic targets
