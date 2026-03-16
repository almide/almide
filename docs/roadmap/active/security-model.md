# Security Model — Layer 2–5

Almide のセキュリティは 5 層で構成される。Layer 1 (Effect Isolation) は実装済み ([done/effect-isolation.md](../done/effect-isolation.md))。

## Layer 2: Capability Restriction — effect fn の権限を細分化

```almide
// 将来構想: effect fn にアクセス可能なリソースを型レベルで制限
effect[fs] fn read_config() -> String = fs.read_text("config.toml")
effect[http] fn fetch(url: String) -> String = http.get(url)
effect[fs, http] fn load_and_send() -> Unit = { ... }
```

- effect fn が「何に」アクセスできるかを明示
- `effect[fs]` は filesystem のみ、`effect[http]` は HTTP のみ
- パッケージの dependency が要求する capability を audit 可能
- **セキュリティ上の意味**: パッケージが `effect[http]` しか使わなければ、filesystem は安全

## Layer 3: Package Boundary — import 時の capability 制限

```almide
// 将来構想: import 時に許可する capability を宣言
import parser  // pure only — no capabilities needed
import fetcher with [http]  // http のみ許可
import toolkit with [fs, http]  // fs + http 許可
```

- パッケージが要求する capability と、使用側が許可する capability のマッチング
- 許可されていない capability を使うパッケージは import エラー
- **セキュリティ上の意味**: supply chain attack で悪意のあるコードが混入しても、capability が制限されていれば被害を限定

## Layer 4: Runtime Sandbox — 実行時の隔離

- WASM ターゲットでの capability-based security
- ファイルシステムアクセスの仮想化
- ネットワークアクセスの allowlist
- **セキュリティ上の意味**: コンパイル時チェックを突破されても、runtime で防御

## Layer 5: Supply Chain Integrity — パッケージの信頼性検証

- パッケージの capability 宣言とコードの整合性を検証
- pure fn only のパッケージは自動的に「安全」マーク
- effect fn を含むパッケージは capability audit が必要
- **セキュリティ上の意味**: 信頼チェーン全体の整合性を保証

## 実装優先度

| Layer | 内容 | 難易度 | 依存 |
|-------|------|--------|------|
| 2 | Capability types | 中 | 型システム拡張 |
| 3 | Import capability | 中 | Layer 2 + パッケージシステム |
| 4 | Runtime sandbox | 高 | WASM target |
| 5 | Supply chain | 高 | Layer 2-3 + パッケージレジストリ |
