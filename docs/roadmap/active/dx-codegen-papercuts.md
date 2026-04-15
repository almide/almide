<!-- description: Codegen bugs (effect fn unification) + DX papercuts (test stderr, explain, local imports) -->
# DX & Codegen Papercuts

Loop で踏んだ codegen バグ 2 件 + DX 課題 3 件の集約 roadmap。1 PR で全部畳むのが筋。

## 動機

このバッチで踏んだ症状を集めると **「user effect fn と built-in effect の codegen 経路が揃ってない」** + **「失敗時に開発者が次の一手を決めるための情報が出ない」** に二分される。個別 issue ではなく、**root cause を同じ pass で揃える** 方針で着地させたい。

## 課題一覧

### A. Codegen バグ (root: effect 経路の非一貫)

#### A-1. lambda 内 `println(...)` の statement 形が rustc fail

**最小再現**:
```almide
[1, 2] |> list.fold(0, (acc, x) => { println("x"); acc + x })
```

**生成 Rust の症状**: `println(format!(...))` (macro `!` が抜ける) → `error[E0061]: function 'println' expects ...`

**真因**: lambda body の statement-level builtin call で、`emit_stmt` 経路が `println!` macro 呼びを emit せず、関数呼びとして書き出している。普通の fn 直下では `BuiltinLowering` か template が `!` を補うが、lambda body はそのパスを通っていない。

**回避策**: lambda 外で呼ぶ、または `let _ = println("x")` で expression にして fold する。

#### A-2. `match` arm の user effect fn と built-in 混在で type mismatch

**最小再現**:
```almide
match cond {
  ok(_) => my_effect_fn(),
  err(_) => println("x"),
}
```

**生成 Rust の症状**: `Result<T, String>` (user effect fn の戻り) と `()` (built-in println の戻り) で arm 間の型が合わず rustc fail。

**真因**: user `effect fn` は `Result<T, String>` で codegen される (`auto-?` 伝播)、built-in は `()` のまま。`match` arm の型を unify する場で両方 `Result<T, String>` に揃える wrapper が要る。

**回避策**: 片方を `let _ = built_in_effect()` で `()` 側に揃えるか、user effect fn 側を `let _ = my_effect_fn()` 経由で expression 化。

#### A-3. 根治設計

両方 **「built-in effect と user effect fn の戻り値型が混在する」** が核。`pass_effect_inference` で built-in effect を `Result<T, String>` 系に lift するか、または逆に user effect fn を `()` で揃える (panic-on-err にする) のどちらか。

memory `reference_codegen_ideal_roadmap.md` の **#1 関数解決を独立パスに** と並走する文脈、`pass_resolve_calls` の前後で effect lift を入れるのが筋。

### B. DX papercuts

#### B-1. `almide test` がコンパイル失敗時に詳細握り潰し

**現状**: `1 previous error; N warnings emitted` だけ出して中身が見えない。`almide run` だと cargo の stderr が出る。

**対症**: `--verbose` flag を用意 (or `ALMIDE_TEST_VERBOSE=1`)、cargo stderr を pass-through。

**真因**: `cargo_build_test` (src/cli/mod.rs:312) が stderr を `Vec<u8>` に capture して、rustc error の場合だけ要約。要約ロジックが「previous error; warnings emitted」しか拾えてない。

**根治**: rustc error spans を JSON で読む (`--message-format=json`) か、capture せず passthrough する。

#### B-2. `almide explain E003` が欲しい

**現状**: diagnostic に `E003` 等の code が付くが、code → 詳細説明の lookup table がない。`grep` で code を探す必要。

**対症**: `almide explain <code>` サブコマンド + `docs/diagnostics/` に code 別 markdown を置く。

**dojo 側 利益**: 自動分類精度が上がる、LLM が hint を辿りやすくなる。

#### B-3. project-local import (`./classify.almd`)

**現状**: `import classify from "./classify.almd"` はおそらくサポートされていない (要確認)。結果として `src/main.almd` が 700 行に肥大化。

**対症**: `almide.toml` に `[modules]` セクション、または file-path import を直接サポート。

**根治**: package system の同 crate 内 module 解決を強化。`docs/specs/module-system.md` に追記。

## ロードマップ

### Phase 1: A 系 (codegen 直し、優先)
- [ ] 最小再現を spec/lang/ または research/ に追加
- [ ] `pass_effect_inference` を読んで built-in と user の lift 経路を統一
- [ ] A-1 / A-2 が repro → fix → green

### Phase 2: B-1 (test の stderr passthrough)
- [ ] `cargo_build_test` に `--verbose` か env var で passthrough mode
- [ ] CI で出力肥大化しないよう default は要約のまま

### Phase 3: B-2 (`almide explain`)
- [ ] `docs/diagnostics/E001.md` 等の skeleton を 5-10 code 分書く
- [ ] CLI subcommand 実装
- [ ] generate script で diagnostic 一覧 README

### Phase 4: B-3 (project-local import)
- [ ] 現状サポート確認 (resolve.rs / module-system.md)
- [ ] 未サポートなら syntax 設計 → 実装

## 出元

dojo loop で実 user 視点で踏んだもの:
- A-1, A-2: nn / dojo タスク両方で繰り返し
- B-1: nn の test 失敗で何度も「再現できないバグ」になりかけた
- B-2: dojo の auto classify でコード数値推測に依存している
- B-3: nn/src 7 ファイル化を諦めた根本理由
