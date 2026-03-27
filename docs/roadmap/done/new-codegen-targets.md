<!-- description: Candidate new codegen targets (Go, Python, C, Swift, Kotlin) -->
<!-- done: 2026-03-18 -->
# New Codegen Targets

IR redesign 完了により、新ターゲット追加のコストが大幅低下。`&IrProgram` を受け取って文字列を返すだけで新バックエンドが書ける。

## Why

Almide は「LLM が最も正確に書ける言語」。ターゲットが増えれば、LLM が Almide で書いたコードをより多くの環境で実行できる。IR が正規化されているため、各ターゲットは AST の複雑さを理解する必要がない。

## Candidate targets

### Priority 1: High impact

| Target | Output | Use case |
|--------|--------|----------|
| **Go** | `.go` | Cloud-native CLI、サーバーサイド。GC あり、goroutine |
| **Python** | `.py` | ML/データサイエンス、スクリプティング。最大のエコシステム |

### Priority 2: Strategic

| Target | Output | Use case |
|--------|--------|----------|
| **C** | `.c` | 組み込み、最大移植性、FFI ブリッジ |
| **Swift** | `.swift` | iOS/macOS ネイティブアプリ |
| **Kotlin** | `.kt` | Android、JVM サーバーサイド |

### Priority 3: Experimental

| Target | Output | Use case |
|--------|--------|----------|
| **Zig** | `.zig` | Rust 代替、WASM、C interop |
| **Lua** | `.lua` | ゲームエンジン組み込み (Roblox, Neovim) |

## Implementation pattern

各ターゲットは同一パターンで実装:

```rust
// src/emit_go/mod.rs
pub fn emit(ir: &IrProgram) -> String {
    let mut emitter = GoEmitter::new();
    emitter.emit_program(ir);
    emitter.out
}
```

IR の主要ノードに対する変換:

| IR Node | 各ターゲットの仕事 |
|---------|-------------------|
| `IrExprKind::BinOp { op: AddInt }` | `+` (全言語共通) |
| `IrExprKind::Call { target: Module }` | stdlib マッピング (言語固有) |
| `IrTypeDeclKind::Variant` | tagged union / sealed class / enum (言語固有) |
| `IrExprKind::Match` | switch / match / when (言語固有) |
| `IrFunction { is_effect: true }` | Result / Exception / error return (言語固有) |

推定規模: 1 ターゲットあたり 500-1000 行 (Rust emitter は ~1200 行、TS は ~800 行)。

## Unlocked by

IR Redesign Phase 5 完了。codegen の入力が `&IrProgram` に統一されたため、新ターゲットは AST を理解する必要がない。
