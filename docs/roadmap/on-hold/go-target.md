# Go Target

**優先度:** post-1.0
**前提:** Codegen v3 アーキテクチャ完成済み (is_rust()=0, TOML+pass方式)

## 作業内容

1. `codegen/templates/go.toml` — Go構文テンプレート
2. Go-specific passes:
   - `ResultToTuplePass` — Result → (T, error) 変換
   - `GoroutineLoweringPass` — fan → goroutine + channel
3. `runtime/go/` — Go runtime functions
4. CI: cross-target Go テスト

## 見積り

アーキテクチャは準備完了。TOML 1ファイル + pass 2-3個で対応可能。
