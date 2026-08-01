<!-- description: Implementation blueprint: Op::Charge, fuel ABI, metered clones, race lowering, gates -->
# Logical-Time Async — the implementation blueprint

> 憲章: [async-inception.md](./async-inception.md)。意味論は
> [logical-time-async.md](./logical-time-async.md)、証明は
> [logical-time-proofs.md](./logical-time-proofs.md)。本文書は「コンパイラのどこに、
> どのデータ構造で、どう通すか」— 実在のコードベースに接地した工学設計である。

## 0. 接地 — コードベースが既に持っているもの

設計文書の語彙を、実装の語彙に翻訳するとこうなる。

| 設計文書の語 | 実装の実体 |
|---|---|
| 共有 MIR | `MirFunction { ops: Vec<Op> }` — **flat な op ストリーム**。basic block は存在せず、`IfThen` / `LoopStart` / `LoopBreakUnless` / `LoopEnd` の構造化マーカーで制御フローを表す（WAT 形状） |
| 両 renderer | `render_native.rs`（v1 native → Rust ソース）と `render_wasm/`（→ WAT）。native には v0 codegen fallback が残る |
| 証明形の validator | `certificate.rs` 族 — MIR ops を事象列に射影し、**Coq 証明済み checker**（`proofs/`、`check_all` / `check_names`）が再検証する既存の枠組み |
| exhaustion の伝播路 | lifted effect fn ABI（can-err = 常時 wrap、統一済み）。`Err(String)` が唯一のエラーチャネル |
| 特殊化の機構 | almide-optimize の mono（既存の複製次元に fuel を足す） |
| 三点比較の土台 | `spec/wasm_cross` fixture 群 + emit 時 Σ-probe という既存 evidence class |

この表の右列がすべて実在することが、以降の設計が絵に描いた餅でない根拠である。

## 1. `Op::Charge` — 時計の最小単位

MIR に 1 つだけ op を足す。

```rust
/// 論理時計の 1 事象。site は CM-1 の charge site 台帳上の安定 id。
/// cost は静的（Const）か、実行時値に線形（Dyn: base + per_unit × len(v)）。
Charge { site: ChargeSiteId, cost: ChargeCost /* Const(u32) | Dyn { base, per_unit, len_of: ValueId } */ },
```

**挿入点**（W1/W2 をこの配置で満たす）:

- 関数入口に 1 つ（cost = 入口からの直列 op 数）
- `LoopStart` 直後に 1 つ（cost = ループ 1 周の直列 op 数）— 全サイクルが必ず通る（W1）
- 可変コスト intrinsic（list 連結、string 構築等）の直前に `Dyn` charge
- `IfThen` の分岐でコストが大きく非対称な場合は分岐側に追加（初版は入口とループのみで開始し、粒度はCM の定数確定と同じ PR で決める）

挿入は **IR→MIR lowering（MIR 誕生時）**。以後の全 MIR パス（`pipeline*.rs`、
`render_wasm_fuse.rs` 等の peephole を含む）は Charge を**跨いで融合しない**義務を負う。

**精密化（憲章 §4 柱 1 の実装上の正確な形）**: 「最適化前に付与」の付与点は IR→MIR
境界である。上流の IR パス（optimize → mono → ir_link）は CM-1 の**定義の内側**にあり、
そこの変更で charge 数が変われば固定 fixture の `consumed_fuel` が赤くなる — つまり
コンパイラ改版による意味変更は ratchet が捕まえ、CM 版数の判断を強制する。ユーザーが
触れる最適化ノブは 2 つだけで、どちらも既に覆われている: native `--release` は rustc
最適化であり Charge はカウンタ演算として描画されるので消えない。`--wasm-opt` は既存の
差分 parity gate の観測対象にカウンタが入るので同様。

## 2. Fuel の実行機構 — 新しい ABI を作らない

- **wasm**: モジュール global `$__fuel: i64`。Charge の描画は
  `global.get → i64.const cost → i64.sub → global.set` + 枯渇分岐。`Dyn` は len を
  読んで積和。
- **native**: ランタイムの thread-local `FUEL: Cell<i64>`。描画は同型の減算 + 分岐。
- **枯渇の伝播は既存の effect ABI に乗せる**。metered クローン（§3）は can-err 形で
  描画され、枯渇は台帳定数メッセージの `Err` として通常の `?` 連鎖で
  `fan.bounded` / `fan.race` の境界まで運ばれる。新しい unwinding・新しい戻り値規約・
  新しい wrapper 網は**作らない** — #840/#841 で統一済みの ABI がそのまま使える。
- charge trace の観測（fixture 用）は `$__fuel` と並ぶ trace アキュムレータ
  （site id 列のハッシュ）を **probe ビルドでのみ**併設する。通常ビルドの race /
  bounded はハッシュを持たない（観測は勝敗と Err だけで足りる）。

## 3. 計測の特殊化 — mono の追加次元

計量は bounded region 内のみ、が設計要求。実装は mono の複製次元として実現する。

1. checker が `fan.bounded` / `fan.race` の body から到達可能な関数集合を閉包で取る
   （閉包リテラルは既存の closure table 経路で追える）。
2. その集合を `metered` フラグ付きで複製（mono がジェネリクスでやっていることと同じ。
   名前は `__fuel$` プレフィクス等の安定規約）。
3. metered クローンだけが IR→MIR で Charge 挿入を受け、can-err ABI で描画される。
   非 metered 側は今日と 1 バイトも変わらない — **グローバルなオーバーヘッドはゼロ**
   という主張はこの構造から従う。
4. **v1-only 制約**: native の v0 codegen fallback には Charge の概念がない。よって
   bounded region 内で v1 native render が wall に当たる構文は**コンパイルエラー**
   （wasm の hard wall と同じ流儀の診断）。fuel は trust-spine の上にだけ建てる。

race の枝はさらに `speculative` フラグを重ねる: trap しうる op（div/rem、配列境界）を
checked 形に描画し、trap を「枝の終端記録」に変換する（意味論の遅延 trap 判定）。
bounded は metered のみ（trap は通常どおり即死 — 投機ではないから）。

## 4. Stage 1 — `--fuel-probe`: 表面ゼロで反証器を先に立てる

特殊化（§3）は Stage 2 の重機であり、Stage 1 では作らない。隠しフラグ
`--fuel-probe` が**全関数**を metered 相当で描画し、プログラム終端で
`(consumed_fuel, trace_hash)` を stderr の規約行に印字する。

- fixture: `spec/wasm_cross/fuel_probe_*.almd` を両ターゲットで実行し、
  **result / consumed_fuel / trace_hash の三点**を比較する。1 単位・1 位置の乖離が
  そのまま「charge-trace 保存が破れた」の反証になる。最有力容疑の
  `render_wasm_fuse.rs` 系 peephole には、Charge を跨ぐ融合を拒否するガードを同 PR で
  入れる。
- **charge certificate**: `certificate.rs` 族に相似形を 1 本足す —
  `charge_certificate(f)` が MIR の Charge 列（site id 順序列）を射影し、render 後の
  再射影と一致することを検査する。ownership certificate → Coq checker と同じ形なので、
  将来 `proofs/` に `ChargeTotality.v`（列の等長・同順）を足す拡張点も同じ場所に開く。
- 生成 MIR 上の property test（ランダム op 列 → 両 render → trace 一致）を
  `tests/` に追加。

## 5. Stage 2 — `fan.bounded`: 表面と CM-1 定数の確定

- parser: fan v2 の head 文法（`fan.bounded(fuel: expr) { body }`）。`fuel:` は
  fan 構文の要素としてパース（汎用ラベル引数は作らない — `parse_fan_primary` の
  member-access 分岐を head-args + block の分岐に拡張する）。
- checker: `static_dispatch.rs` の fan アーム表に `bounded` を追加。body は pure 制約
  （既存 purity 機構）。型は `Result[T, String]`、auto-unwrap は race/any/settle の
  既存契約に従う。
- lowering: body 閉包を metered 集合に登録 → §3 の複製 → region 入口で
  `FUEL = min(n, 外側remaining)`（EIP-150 式 min-cap は「入るときに代入、出るとき
  に消費分を外側から減算」の 2 op で実装できる — region タグ不要の根拠）。
- **CM-1 の定数はこの PR で確定**し、契約台帳に versioned オブジェクトとして載せる。
  fixture は代表プログラムの consumed_fuel を絶対値で pin する（ratchet）。

## 6. Stage 3 — `fan.race`: まず逐次で出荷する

意味論の合流定理は「逐次 scan も並列も同じ観測」を保証している。これを実装順に翻訳
すると、**初版の race は両ターゲットとも逐次 scan でよい**:

1. 枝をリスト順に、cap = `min(n, 最良決定事象.time − 1)` で実行（cap は Charge の
   枯渇分岐がそのまま使える — cap 用の別機構は不要、`FUEL` に cap を入れるだけ）。
2. 枝の終端（Complete / Err / trap 記録 / 枯渇）を記録し、scan 後に merge 順最小の
   決定的事象で裁定。trap が可視窓内なら、記録した trap をそこで**再送出**する。
3. native のスレッド並列は**後段の性能レーン**として分離する。意味論は既に並列を
   許しており（証明済み）、初版に並列を入れない判断は「観測に影響しない最適化を
   後回しにした」以上の何も失わない。wasm はもとより逐次である。

この分割で Stage 3 の実装リスクは「checked-trap 描画」と「scan の driver」だけに
縮む。E027 の改訂（署名移行ヒント化）も同 PR。

## 7. fan v2 Wave 1 — 既存 desugar の簡略化として実装する

block 形の any/settle は、実は今日の `desugar_fan.rs` より**単純**になる。現行は
thunk リストのリテラル/let 束縛を追跡して本体を inline 復元している（#599 経路）。
block 形は arm の式が**最初から手元にある** — 追跡機構ごと不要になり、match チェーン
への脱糖はそのまま流用できる。mapper 形 `fan.any(xs, f)` は `fan.map` と同じ
「データ + 閉包 1 個」なので `List[funcref]` wall に当たらない（fan-v2.md の主張の
実装的裏付け）。

タッチ点: `parser/primary.rs`（head 分岐）、`static_dispatch.rs`（アーム表 + thunk
リスト形の E0xx tombstone）、`desugar_fan.rs`（簡略化）、`fmt`、interp ブリッジ、
spec 8 ファイル移行、`ExprKind::Await` / `r#async` の死骸撤去。

## 8. interp（第三の審級）の扱い

almide-interp は IR を歩く（MIR より上流）。fuel は MIR 定義なので、interp は
Stage 1–3 の間 **fuel 構文について abstain** と明示する（race/bounded を含む
プログラムで 3-way oracle は 2-way に落ちる）。CM-1 を IR 側に鏡写しにする案は
二重定義のドリフト源なので採らない。abstain の範囲は fixture 側の注記で機械可読に
しておき、将来「interp が MIR を歩く」改修があれば畳む。

## 9. ゲート配線の一覧

| ゲート | 何を pin するか | 所在 |
|---|---|---|
| 三点 fixture | result + consumed_fuel + trace_hash の両ターゲット一致 | `spec/wasm_cross/fuel_*` |
| charge certificate | render が Charge 列を等長・同順で保存 | `certificate.rs` 族 + 将来 `proofs/` |
| consumed ratchet | 代表プログラムの絶対 fuel 値（コンパイラ改版の意味変更検出） | fixture 内 pin |
| 合流ゲート | 意味論モデルの全数検査（設計側の回帰） | `research/spike/logical-time-race/run-gate.sh` |
| Lean belt | 選択代数 7 定理 | CI `lean-proofs`（配線済み） |
| wasm-opt parity | `-Oz` がカウンタを壊さない | 既存 parity gate（観測拡大のみ） |
| 契約台帳 | CM-1 versioned オブジェクト + 新 C-NNN 群 | `contracts.toml` + `check-contracts.sh` |

## 10. 未決（実装が答えを出す点）

- Charge の粒度（入口+ループのみで始めるか、分岐対称性の閾値をどこに置くか）—
  Stage 1 の probe 計測で決め、CM-1 定数と同じ PR で固定。
- `Dyn` charge の per_unit 定数表 — 同上。
- metered クローンの名前規約と closure table の metered スロット表現 — Stage 2 の
  実装 PR で確定（設計上の答えは「region に流入しうる閉包は metered 変種を持つ」）。
- trace_hash のハッシュ関数（fixture 診断のため、単なる総和ではなく順序敏感なもの。
  位置特定には probe の逐次ダンプモードを併設）。

この文書のどの節も、対応する実装 PR が上書きしてよい — ただし上書きは本文書の
改訂として行い、設計と実装が別々の真実を持つ状態を作らないこと。
