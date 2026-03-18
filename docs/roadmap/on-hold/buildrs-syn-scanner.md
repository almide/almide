# build.rs Runtime Scanner 堅牢化

**優先度:** post-1.0
**見積:** ±200行, 中。ビルド時間とのトレードオフ。

## 現状

正規表現で `runtime/rs/src/*.rs` の関数シグネチャをパース。壊れやすいが動いてる。

## 理想

`syn` crate で正確にパース。

## タスク

- [ ] syn crate 導入 (build-dependencies)
- [ ] 関数シグネチャ抽出を AST ベースに
- [ ] ビルド時間への影響測定

## 判断

壊れてからでいい。syn は重い（ビルド時間 +5-10秒）。
