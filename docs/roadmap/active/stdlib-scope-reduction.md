# Stdlib Scope Reduction

**優先度:** 1.0前 — 凍結前に外に出すものを決める
**リサーチ:** [stdlib-module-matrix.md](../../research/stdlib-module-matrix.md)

## 削除候補 (stdlib → first-party package)

他言語の1.0 stdlibとの比較に基づく判断。

| Module | 現在 | 根拠 | 対応 |
|--------|------|------|------|
| **uuid** | TOML 6関数 | Gleam/Elm/Rust/Kotlin/MoonBit/Elixir **全てstdlib外** | 削除。`crypto.random_hex` で代替可 |
| **crypto** | TOML 4関数 | Rust/Kotlin/MoonBit/Elixir全てstdlib外。Go のみ含む | 削除。薄すぎて凍結リスク |
| **toml** | .almd 14関数 | **全言語がstdlib外** | first-party packageに |
| **compress** | .almd 4関数 | Go以外全てstdlib外。4関数では中途半端 | first-party packageに |
| **term** | .almd 21関数 | **全言語がstdlib外**。TS targetで動作不可 | first-party packageに |

## 判断基準

1. **他言語の1.0 stdlibに含まれているか** — 半数以上が含めていないならstdlib外
2. **multi-target で動作するか** — Rust + TS 両方で意味があるか
3. **凍結リスク** — API が成熟してないまま凍結すると Go の log 問題になる
4. **代替手段** — stdlib 内の他モジュールで代替可能か

## TODO

- [ ] uuid 削除 → `crypto.random_hex(16)` で十分
- [ ] crypto 削除 → security-sensitive なAPIは成熟してから再導入
- [ ] toml, compress, term を stdlib/ から packages/ に移動
- [ ] STDLIB_MODULES, PRELUDE_MODULES から除外
- [ ] FROZEN_API.md を更新
