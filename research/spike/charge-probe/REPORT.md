# Stage 1 charge-trace preservation — first empirical run

Status: **PASSED — 9/9 comparable runs three-point identical (2026-08-02)**
Spec: [docs/roadmap/active/logical-time-implementation.md](../../../docs/roadmap/active/logical-time-implementation.md) §4
再実行: `ALMIDE_FUEL_PROBE=1 almide run fixtures/<f>.almd [--target wasm]`（この branch のビルドで）

## 何を検証したか

logical-time-proofs.md が「証明しないもの」と明示した唯一の前提 —

> 両 renderer は共有 MIR の charge 事象を保存する（同じ事象、同じ順序、各 1 回）

— を、実装して実測した。`Op::Charge` を IR→MIR 境界（関数入口 + 全 LoopStart 直後、
W1 配置）に挿入し、native（生成 Rust + thread-local カウンタ）と wasm（exported
mutable globals + ゲスト内 epilogue 印字）の両レッグで
**(stdout, consumed, trace_hash)** の三点を比較した。trace は順序敏感
（`t*1000003+site` の wrapping、site = 関数名 FNV + 関数内連番 — 関数順序非依存）。

## 結果

| fixture | 対象経路 | 判定 |
|---|---|---|
| loop | while ループ | **MATCH** (1002) |
| recursion | 再帰 fib(20) | **MATCH** (21892) |
| branch | if/else + match | **MATCH** (602) |
| strings | 文字列連結ループ | **MATCH** (102) |
| mutual | 相互末尾再帰 5000 段 | **MATCH** (5002) |
| switch | 12-arm dense match（**br_table 経路**） | **MATCH** (1002) |
| fusion | 算術密集（**fuser 経路**） | **MATCH** (2002) |
| nested | 二重ループ | **MATCH** (2602) |
| recursion + `--wasm-opt` | **Binaryen -Oz 越し** | **MATCH** (21892) |
| list | lambda 入り filter/map | native wall（v0 fallback）— 比較不能 |
| bce | list.get ループ（BCE 経路） | native wall — 比較不能 |

**乖離ゼロ。** 最有力容疑だった renderer-local peephole（fuser・br_table 化・
bounds-check-elision・wasm-opt）はいずれも charge を落とさず、順序も乱さなかった。

## 消費値の意味論的検証（数字が正しいことの独立確認）

- fib(20): 呼び出し木サイズ 21891 + main 入口 1 = **21892** ✓
- loop 1000 回: 入口 1 + ループ頭通過 1001（脱出判定の回も通る）= **1002** ✓
- is_even(5000): 呼び出し 5001 + main 1 = **5002** ✓

## 既知の限界（Stage 1 スコープ外として明示）

1. **native wall = probe 消失が無音**: v1 native が wall して v0 fallback すると
   probe 行が出ない。fallback 検出を probe 失敗として扱う配線が本実装では必要。
2. **fan{} の並列 native**: thread-local カウンタは arm スレッドの消費を合算しない。
   arm 直列化 or atomic + 順序正規化が Stage 3（race）で必要 — deterministic-bounds
   の本丸がここに現れる。
3. **trap 経路**: proc_exit 直行で epilogue が走らず probe 行が出ない（両レッグ同様）。
4. 計測は entry + loop 頭のみ（粒度 v0）。可変コスト（Dyn charge）は未実装。

## 判定

charge-trace 保存は、この粒度・この経路集合では**成立している**。Stage 1 の
本実装（spec/wasm_cross fixture 化、charge certificate、property test）に進む
根拠が取れた。反証は出なかったが、探索空間は 9 プログラム — 網羅ではない。
