# Almide Roadmap

## Multi-file / Module System

Production readiness のための最重要課題。

### 現状 (Done)

- [x] stdlib import (`import string`, `import list` 等 15モジュール)
- [x] ローカルユーザーモジュール解決 (`name.almd` / `name/mod.almd`)
- [x] git依存 (`almide.toml` で tag/branch 指定)
- [x] 循環import検出
- [x] バージョン付き依存解決 (ダイヤモンド依存対応)
- [x] Rust emitter での `mod name { ... }` 生成
- [x] checker でのモジュール関数登録・型検査

### TODO

#### Phase 1: ユーザーモジュール分割の e2e 確認
- [ ] ユーザー定義モジュール間の import が全ターゲット (Rust/TS/JS) で動くことを確認
- [ ] exercise として multi-file サンプルを追加
- [ ] `resolve.rs` の整理 (現在 dead code 警告あり)

#### Phase 2: TS/JS emitter のマルチファイル対応
- [ ] 現状は IIFE オブジェクトラップのみ — ESM 出力 or bundle 対応
- [ ] playground (単一ファイル出力) との整合
- [ ] import されたモジュールの関数を正しく呼び出せるようにする

#### Phase 3: selective import
- [ ] `import name.{ func1, func2 }` — パーサーは対応済み、checker/emitter が未実装
- [ ] 使われていない関数の tree-shaking (将来)

#### Phase 4: モジュール可視性
- [ ] `pub fn` / private fn の区別
- [ ] export されていない関数へのアクセスをエラーにする

#### Phase 5: ネストしたモジュールパス
- [ ] `import foo.bar.baz` — パーサーは対応済み、resolver が未対応
- [ ] ディレクトリ階層に基づくモジュール解決

## その他

- [ ] ユーザー定義ジェネリクス
- [ ] パッケージレジストリ (現在 git-only)
