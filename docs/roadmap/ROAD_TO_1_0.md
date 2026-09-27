# Road to 1.0 — 0.65 → 0.99 バージョンラダー

> **現在 v0.64.0(2026-09-27)**。0.65 から 1.0 までの全 minor を decade(0.6x 残り / 0.7x / …)単位の
> アークに割り、各 minor に「顔となる大機能」を 1 つずつ載せる台帳。**open issue 88 件はすべてこの
> 台帳のどこかに割付済み**(下の「台帳の完全性」)。
>
> **これは時間軸の文書**。同じ作業を評価軸(速度 / 性能 / 正しさ / LLM writability / DX / エコシステム)
> から見た勝ち筋と勝利条件は [ALL_AXES.md](ALL_AXES.md) にある。作業単位(Bolt)の粒度は
> [v1 Bolt Backlog](active/v1-bolt-backlog.md) にあり、本書の行はその Bolt 群を版に束ねたもの。

## 2026-09-27 に引き直した理由

旧台帳(2026-07-30 起草、0.41 → 0.99 の 59 行)は、0.6x = cranelift、0.7x = 証明と Critical プロファイル、
0.8x = 資格化キット、0.9x = 規範仕様、という順序だった。実際には次のように進んだ。

- **0.7x〜0.9x の中身が前倒しで閉じた**。flight-grade アーク(2026-08-27〜28)で #566 / #567 / #568 / #569 /
  #570 / #572 / #573 / #574 / #575 / #576 / #776 が、続いて #571(08-30)/ #865(09-18)/ #586(09-21)が閉じた。
  ALS の規範化 #530 と interp の規範意味論化 #564 も 08-27 に閉じた。旧台帳が引用していた issue 約 80 件のうち、
  open なのは 7 件(#924 / #1003 / #1005 / #1315 / #1331 / #1388 / #2146)だけになった。
- **0.6x は cranelift ではなく structural leg に使われた**。greenfield で新造した wasm エンジンが v0.60.0 で
  既定になり、0.61〜0.64 はその仕上げ(借用解析の一本化、leak 修正、http layer、`almide survive`)だった。
  #1005(cranelift)は起票以来コメントがない。
- **プログラムトラック(#577〜#585)は 2026-08-13 に閉じた(not planned)**。法務・商業・組織の項目で、
  この repo で拾えるエンジニアリング作業ではないため、#586 と
  [flight-organization](active/flight-organization.md) に移した。何かが完了したという主張ではない。

したがって、行を個別に直すのではなく、**現在 open の issue から 0.65 以降を組み直した**。0.41〜0.64 の実績は
下の「出荷記録」に残す。

## 読み方と運用ルール

- **decade = アーク**。テーマと出口ゲートを持つ。decade 境界(0.70, 0.80, 0.90)はゲートリリース —
  その decade の出口監査を固定する 1 リリース。
- **各 minor = 行 1 つ**。パッチ(0.65.x)は自由。decade 内で番号が前後にずれるのは構わない — 不変条件は
  decade ゲートであってバージョン番号ではない。findings や dogfooding が割り込むときは既存行を decade 内で
  後ろへ送り、**注記を残す**(静かな番号の付け替えをしない)。
- **新 issue を立てたら、同じ PR でこの台帳のどこかに割り付ける**。載らない issue を作らない。
- **Unit が閉じるのはタグが存在した時**(下の 2026-08-01 の記録)。
- **実行は AI-DLC で回す** — Intent = decade / Unit = 行 / Bolt = 作業サイクル、人間は Mob ポイント
  (`mob` ラベル)のみ。Bolt の一覧と状態は [v1 Bolt Backlog](active/v1-bolt-backlog.md)。

### 順序の根拠

1. **bug > chore > feature、regression と release blocker は最上位** — 各 decade の先頭行に置く
2. **出荷物に信頼を載せるのが先** — 証明書は今 incumbent leg にしかなく、既定の structural leg には載って
   いない。incumbent を消すのは、証明を structural に移した**後**(順序を逆にすると証明が消える)
3. **性能系を試行回数系より先に**(2026-09-08 の方針)— 表現と速度が動いている間に MSR を測っても、
   測った数字がすぐ古くなる。MSR の実測と公開は 0.8x
4. **計測器が手術より先** — perf の計器(0.77)が native 最適化の結果を読む前提。survive A/B(0.84)は
   その後の数字を読む
5. **破壊的な表面変更は仕様凍結の直前にまとめる**(0.95 = edition 行)
6. **仕様凍結は最後** — 凍結は完成の宣言であって願望ではない

---

## 0.6x 残り — 出荷物に信頼を載せる

既定で出荷している structural leg を、証明書を持つ唯一の leg にする。incumbent wasm emitter(約 2.6 万行)を
退役させる。**律速は既定 leg の証明被覆**(certified wasm_cross は現在 789 本中 1 本、`d9945cd57`)。

| Version | 大機能 | Issue |
|---|---|---|
| 0.65 | **release blocker と bug の焼却** — `process.exit` 126..255(regression)、wasm `bytes.repeat` と native `matrix.linear_f32_row_no_bias` の上限検査漏れ(I-divergence 2 件)、値を返す分岐の `panic`、誤誘導診断、static musl バイナリ | [#2780](https://github.com/almide/almide/issues/2780), [#2782](https://github.com/almide/almide/issues/2782), [#2783](https://github.com/almide/almide/issues/2783), [#2769](https://github.com/almide/almide/issues/2769), [#2771](https://github.com/almide/almide/issues/2771), [#2772](https://github.com/almide/almide/issues/2772), [#2777](https://github.com/almide/almide/issues/2777) |
| 0.66 | **HTTP serving 前半**(ADR-0020 step 0–4)— 宣言 fn 型を経由する effect fn 値、router の壁、E008 の一般化(epoch 6)、instance-closed な serve 引数、`kv` モジュール、native の並行 serve | [#2705](https://github.com/almide/almide/issues/2705), [#2696](https://github.com/almide/almide/issues/2696), [#2697](https://github.com/almide/almide/issues/2697), [#2698](https://github.com/almide/almide/issues/2698), [#2699](https://github.com/almide/almide/issues/2699), [#2665](https://github.com/almide/almide/issues/2665) |
| 0.67 | **HTTP serving 後半**(ADR-0020 step 5–8)— SIGTERM での drain と flush、HTTP 意味での serve 比較、stock `incoming-handler@0.2` export、LLM 向け docs(最後)。#2705 を閉じる | [#2692](https://github.com/almide/almide/issues/2692), [#2700](https://github.com/almide/almide/issues/2700), [#2659](https://github.com/almide/almide/issues/2659), [#2701](https://github.com/almide/almide/issues/2701) |
| 0.68 | **structural lowering の残件を 0 に** — route census(91 行、shrink-only #2741)を空にする。incumbent 直行ルートの撤去 | [#2743](https://github.com/almide/almide/issues/2743)–[#2751](https://github.com/almide/almide/issues/2751), [#2752](https://github.com/almide/almide/issues/2752) |
| 0.69 | **structural witness ラダー** — call 引数・分岐 frame・loop frame・closure / effect fn・call-modes・name totality・capability の各 witness、receipt の再生成と各 `.wasm` の SHA-256 pin、incumbent 専用 CI gate の移設 | [#2755](https://github.com/almide/almide/issues/2755), [#2756](https://github.com/almide/almide/issues/2756), [#2757](https://github.com/almide/almide/issues/2757), [#2758](https://github.com/almide/almide/issues/2758), [#2759](https://github.com/almide/almide/issues/2759), [#2760](https://github.com/almide/almide/issues/2760), [#2753](https://github.com/almide/almide/issues/2753) |
| 0.70 | **ゲートリリース — incumbent wasm emitter を削除**(one-leg router、`ALMIDE_WASM_INCUMBENT` 撤去、`render_wasm*` 削除) | [#2761](https://github.com/almide/almide/issues/2761), [#2739](https://github.com/almide/almide/issues/2739), [#1696](https://github.com/almide/almide/issues/1696), [#1584](https://github.com/almide/almide/issues/1584) |

**Gate 0.70**: wasm leg が 1 本 / C-SAFE・C-PROVEN receipt が既定 leg の build から出る / certified wasm_cross が
grow-only ラチェットで全数に届く / ADR-0020 の serving 契約が両 leg で成立。

## 0.7x — native の性能と表現

wasm が native に勝っている箇所(Map / JSON / Value で 1.5〜2.3 倍、#2157)を解消し、native leg の「数字の話」を
揃える。表現の裁定(ADR-0018)が先頭なのは、その後の最適化がすべてその表現を対象にするため。

| Version | 大機能 | Issue |
|---|---|---|
| 0.71 | **`Value` の表現** — ADR-0018(`Value::Str` = `AlmideStr`、2026-09-08 裁定済み)の実装。decode の 68% を占める文字列コピーを消す | [#1679](https://github.com/almide/almide/issues/1679) |
| 0.72 | **native の Map / Value を wasm 双子以上に** — ループ内で Map に積むと native だけ二次になる問題を含む | [#2157](https://github.com/almide/almide/issues/2157), [#2393](https://github.com/almide/almide/issues/2393) |
| 0.73 | **native の `fan` に並列性を** — `fan.map` が逐次、effect callback が 1 本ずつ走る問題 | [#2044](https://github.com/almide/almide/issues/2044), [#2594](https://github.com/almide/almide/issues/2594) |
| 0.74 | **fusion の一本化** — self-hosted HOF 本体の特殊化として fusion を作り直し、Rust target でも `|>` 連鎖を融合する。推奨綴りを最速綴りにする | [#2289](https://github.com/almide/almide/issues/2289), [#2045](https://github.com/almide/almide/issues/2045), [#2331](https://github.com/almide/almide/issues/2331) |
| 0.75 | **bytes / 数値ワークロード** — `string.to_bytes` が `Vec<i64>` である問題(lexer の 84% が malloc/free)、死んでいる BLAS 経路 | [#2078](https://github.com/almide/almide/issues/2078), [#2512](https://github.com/almide/almide/issues/2512) |
| 0.76 | **サイズと起動** — native hello 468 KB の比較測定、main 前の 0.166 ms、小さな wasm の大半が program でない問題 | [#2092](https://github.com/almide/almide/issues/2092), [#2162](https://github.com/almide/almide/issues/2162), [#2312](https://github.com/almide/almide/issues/2312) |
| 0.77 | **perf の計器を全数に** — runtime ラチェットを 13/13 program に、`almide profile`、fuzz の perf-class finding と下流の 4〜5% 退行 | [#2142](https://github.com/almide/almide/issues/2142), [#2598](https://github.com/almide/almide/issues/2598), [#2302](https://github.com/almide/almide/issues/2302), [#2415](https://github.com/almide/almide/issues/2415) |
| 0.78 | **native Rust の生産者を 1 本に**(`codegen` と `mir/render_native`)— Critical プロファイルの native 経路もこれに乗る。**Mob** | [#1480](https://github.com/almide/almide/issues/1480) |
| 0.79 | **debug パスの判断行** — rustc が今も編集ループの律速かを測り、cranelift(#1005)と wasm DWARF(#1315)の着工/不着工を決める。0.45 と同じ「測って決める」行。コードを書かないことが正解になりうる | [#1005](https://github.com/almide/almide/issues/1005), [#1315](https://github.com/almide/almide/issues/1315) |
| 0.80 | ゲートリリース — native 性能監査を固定 | — |

**Gate 0.80**: runtime ラチェットが 13/13 program を監視 / 同じ program で native が wasm 双子に負けない /
対 handwritten Rust の勝利宣言(#1330)がラチェット下で維持。

## 0.8x — エージェント編集ループと MSR の公開

v0.64.0 で出荷した `almide survive` / `repair` 診断を、エージェントが使う道具一式に広げ、その効果を
**数字で**示す。MSR の実測はここ(順序の根拠 3)。

| Version | 大機能 | Issue |
|---|---|---|
| 0.81 | **`almide query`** — hover / definition / references / inlay hints を CLI サブコマンドに | [#2148](https://github.com/almide/almide/issues/2148) |
| 0.82 | **agent-connect と生成系 code action** — 言語ガイドをバイナリに同梱、missing pattern・label 補完・decoder 生成・pipe 変換を MSR 順に | [#2153](https://github.com/almide/almide/issues/2153), [#2640](https://github.com/almide/almide/issues/2640) |
| 0.83 | **インクリメンタルの基盤** — `.almdi` interface の消費と early cutoff、モジュール単位キャッシュ、entry point を IR から決める | [#2151](https://github.com/almide/almide/issues/2151), [#1003](https://github.com/almide/almide/issues/1003), [#2372](https://github.com/almide/almide/issues/2372) |
| 0.84 | **survive A/B を実測する** — dojo#58/#59、勝利条件 = 平均試行回数 ≤ 0.8×control。数字が出たら #2147 を閉じる | [#2147](https://github.com/almide/almide/issues/2147) |
| 0.85 | **契約相対生存アークの完結** — 契約保存変更バンク(dojo#3)の飽和 verdict、resource oracle 用の live-allocation カウンタ | [#1998](https://github.com/almide/almide/issues/1998), [#2581](https://github.com/almide/almide/issues/2581) |
| 0.86 | **protocol を DDD の port に使えるように** — generic protocol、protocol-as-type(#1998 アークの最後)。stdlib の `_str` 双子 119 本を protocol で畳むかの判断(**Mob**) | [#1589](https://github.com/almide/almide/issues/1589), [#1460](https://github.com/almide/almide/issues/1460) |
| 0.87 | **MSR の現行計測** — 147 日古い thesis の数字を更新。frontier モデルの行は手動計測(CI に API 鍵を置かない) | [#1963](https://github.com/almide/almide/issues/1963) |
| 0.88 | **公開 MSR ハーネス** — 同条件で他言語も走らせられ、第三者が再現できる。README の鮮度ラチェット | [#2146](https://github.com/almide/almide/issues/2146) |
| 0.89 | **開発体験の穴埋め** — fuzzing / coverage / benchmark の標準導線、再コンパイルしない REPL | [#1490](https://github.com/almide/almide/issues/1490) |
| 0.90 | ゲートリリース — エージェント編集ループ監査を固定 | — |

**Gate 0.90**: MSR が第三者再現可能で 90 日以内 / survive の効果が統制 A/B の数字で出ている(勝ち・負けとも公開)/
エージェントが CLI・MCP・LSP の同じ面で query・survive・repair を使える。

## 0.9x — エコシステム・表面凍結・1.0 エンドゲーム

外で書かれたパッケージが公開まで辿り着ける道を作り、破壊的な表面変更を 1 か所にまとめてから凍結する。

| Version | 大機能 | Issue |
|---|---|---|
| 0.91 | **パッケージ公開の道** — publish 経路、MVS が選んだ版を lock に書く、interface artifact から API docs を生成 | [#2332](https://github.com/almide/almide/issues/2332), [#2532](https://github.com/almide/almide/issues/2532), [#2597](https://github.com/almide/almide/issues/2597) |
| 0.92 | **プロセスと暗号** — 子プロセスを自分の group で起動・非ブロック poll・木ごと停止する handle、`process` を WIT resource の host capability に、純 Almide の AEAD / Ed25519 / HMAC | [#2587](https://github.com/almide/almide/issues/2587), [#2589](https://github.com/almide/almide/issues/2589), [#2596](https://github.com/almide/almide/issues/2596) |
| 0.93 | **Component Model の残件** — host 境界 determinism、env/process の p3、wasi:http@0.3 client、P3 handler export と stock `kv` binding | [#1628](https://github.com/almide/almide/issues/1628), [#1710](https://github.com/almide/almide/issues/1710), [#2702](https://github.com/almide/almide/issues/2702) |
| 0.94 | **承認済み ADR の残り** — ADR-0005(演算子 desugar、`unwrap_or` 撤去、`?.` 公式化)、ADR-0021 の docs | [#1107](https://github.com/almide/almide/issues/1107), [#2728](https://github.com/almide/almide/issues/2728) |
| 0.95 | **edition 行(破壊的変更の最後の窓)** — `fan {}` の並列レコードリテラル化、非 ASCII 識別子(型/値の大文字小文字分割の置き換え)、ソースで宣言できる所有権保証(fip/fbip 形)。各 **Mob** | [#1388](https://github.com/almide/almide/issues/1388), [#1456](https://github.com/almide/almide/issues/1456), [#1479](https://github.com/almide/almide/issues/1479) |
| 0.96 | **仕様凍結** — 構文・演算子・stdlib 境界の最終監査。ALS は規範化済み(#530)なので、ここでは凍結の宣言と edition 方針の発効 | — |
| 0.97 | **版と方言のコミットメント** — `proofs/dialect-epochs.toml` と interface diff(`scripts/check-interface-diff.sh`)を 1.x の互換性約束として文書化し発効 | — |
| 0.98 | **既知乖離ゼロ監査** — contract ledger `flagged-for-revision` 0(現在 0 / 369)、wall 0、claim-drift 0、fuzz nightly の full-budget 連続緑 | [#924](https://github.com/almide/almide/issues/924) |
| 0.99 | RC 硬化 — 全ゲート緑のままフリーズ。残タスクは 1.0 リリースのみ | — |

**Gate 1.0**: 下の「1.0 の定義」が全項目成立。

## 1.0 以降

| Issue | 内容 | 1.0 に入れない理由 |
|---|---|---|
| [#1331](https://github.com/almide/almide/issues/1331) | `fan` → WGSL:1 ソースで CPU / GPU leg、同一出力 | 新しいバックエンドの追加。1.0 の定義のどの項目にも効かず、凍結後に足しても互換を壊さない |

## パッチ枠(行を持たない chore)

| Issue | 内容 |
|---|---|
| [#2618](https://github.com/almide/almide/issues/2618) | stamped ledger counts のドリフト — リリース手順 7 の `gen-ledger-counts.sh` で解消する |

---

## 1.0 の定義

2026-09-27 時点の達成状況を併記する。

| 項目 | 条件 | 状況 |
|---|---|---|
| **仕様** | ALS が規範(#530)、almide-interp が規範意味論(#564)、仕様は凍結済み | 規範化は ✅(08-27)、凍結は 0.96 |
| **正しさ** | fuzz 常時緑、hole-hunt findings 0、contract ledger flagged 0、**既定 leg の**全ビルドに translation-validation 証明書 | hole-hunt ✅(#912)、flagged 0 ✅、証明書は incumbent のみ → 0.69–0.70、fuzz 連続緑は 0 日 → 0.98 |
| **速度** | 編集ループが規模非依存、対 Rust ギャップは実測・ラチェット管理 | 規模非依存 ✅(#1334)、ラチェットは 7/13 program → 0.77。debug パスの rustc 除去は 0.79 で要否を判断 |
| **信頼** | critical profile + qualification dossier が製品として渡せる、reference app が証拠 | ✅(#567 / #571 / #574 / #776)。組織側(署名法人など)は repo 外([flight-organization](active/flight-organization.md)) |
| **数字** | build-speed / runtime-perf / safety の三点が README で実測公開 | ✅(#999)、90 日の鮮度ゲート付き |
| **指標** | MSR が第三者再現可能 — dojo ハーネスが公開され、他言語でも同条件で測れる | 0.88(#2146) |

---

## 出荷記録(0.41〜0.64)

旧台帳の行と実際のタグの対応。**行と版は 1:1 ではない**(下の 2026-08-01 の記録)。

| 版 | 日付 | 顔 |
|---|---|---|
| v0.41–v0.44 | 07-31〜08-01 | 計測器(fuzz nightly、perf scoreboard)、単一ドライバ(#925)、concurrency 立場決定と fan 族(#1000) |
| v0.45–v0.49 | 08-01 | rtlib 化・クエリ基盤・モジュールキャッシュは「測って着手しない」と決定、10k 行 dogfood 完了(#1001) |
| v0.50 | 08-01 | **ゲート** — 三点の数字が公開・導出・ラチェット化(#999) |
| v0.51–v0.53 | 08-02〜03 | effect 表面規則の統一、JSON interop の決定(#1062)、プラットフォーム conformance(Windows・Wasm 3.0) |
| v0.54–v0.56 | 08-06 | エラー表面の裁定と出荷 — `-> T!`、auto-`?` 撤去、`T?`、try_ 族撤去 |
| v0.57–v0.59 | 08-11〜27 | 整数ドメイン掃討(218 セル)、3-way oracle 87%、protocol による静的 DI、`@bounded` プロファイル |
| v0.60 | 08-30 | **structural leg が既定に**(greenfield Stage 2) |
| v0.61–v0.62 | 08-30〜09-08 | patch train、残りを束縛するパターン、docs が走る、wasm leg の leak 修正 |
| v0.63 | 09-24 | 借用と clone を 1 つの解析が決め、出力 IR と照合する(#2186 / #2231) |
| v0.64 | 09-27 | 適用前に編集を検査する(`almide survive`、#2147)、診断に repair、http layer |

旧台帳の「リサーチ発 issue 割付(2026-08-13)」「軸ロードマップ発 issue の割付」の 23 件は、#1315 / #1331 / #1388 を
除いてすべて閉じた。残り 3 件は上の 0.79 / 1.0 以降 / 0.95 に割り付けてある。旧台帳の全文は git 履歴にある
(`git show 8de6d7473:docs/roadmap/ROAD_TO_1_0.md`)。

## 台帳の完全性

- **バージョン行 35 / 35** — 0.65〜0.99 の全 minor に行がある(大機能 32 + ゲートリリース 3)
- **open issue 88 / 88 割付済み**(2026-09-27)— バージョン行に 86、1.0 以降に 1、パッチ枠に 1
- Issue 欄が「—」の行(ゲートリリース 0.80 / 0.90、0.96 / 0.97 / 0.99)は、着工時に issue を立てて同じ PR で
  リンクを埋める
- この台帳と issue リストの乖離は負債 — 新 issue は同じ PR でここに割り付け、閉じたら issue リンクが closed に
  なることで進捗が見える

## Release-order deviation, 2026-08-01 — recorded so the ladder can be read honestly

**The ladder's rule is one Unit, one release, in order.** It was broken during the 0.4x decade
and this note is the correction rather than a quiet renumber.

What happened: v0.45.0 shipped, `Cargo.toml` was bumped to 0.46.0, and then the work and
records for Units **0.47, 0.48, 0.49 and 0.50 all landed on `develop` before v0.46.0 was
tagged**. The tree therefore carried five Units' worth of change under one unreleased version
number.

Why it happened, plainly: Units 0.45, 0.47 and 0.48 resolved by *measurement* rather than by
implementation — each concluded "the trigger does not fire, here are the numbers" — so they
produced documents rather than artifacts, and a document feels like it does not need a release.
It does. A release is what makes a row's conclusion citable and dated, and a measurement whose
conclusion is "we are not building this" is exactly the kind of decision that needs a fixed
point someone can point at later.

**The correction**: releases resume in order from **v0.46.0**, one per row, and no row is
described as shipped before its tag exists. Where a release carries records that landed early,
its notes say so instead of pretending the ordering held.

**The rule, sharpened for next time**: a Unit is not done when its `construction.md` is
written. It is done when the tag exists. Starting the next Unit before that is what produced
this, and the ladder is only auditable if the two stay coupled.

## Merged past unverified CI, 2026-08-01 — the mechanism that should have stopped it

**v0.47.0, v0.48.0 and v0.49.0 were tagged on commits whose CI never completed.** Not failed —
**CANCELLED**, each superseded by the next merge while still in flight. Recorded here because
the cause is a missing mechanism, not a missing intention.

**What happened.** The first two release PRs were merged after explicitly polling their checks
to green. Polling took ~50 minutes per release, so the remaining three switched to
`gh pr merge --merge --auto`, expecting auto-merge to hold until checks passed. `main` requires
a pull request but has **no required status checks**, so `--merge` executed immediately and
`--auto` was a no-op. The command returned `MERGED` and the release proceeded.

**A flag was substituted for a verification.** That is the whole failure. The intention was
identical in all five releases; only the enforcement differed.

**The damage, measured rather than assumed**: `git diff v0.49.0 v0.50.0` restricted to
`crates/ src/ stdlib/ runtime/ spec/ tests/ tools/ .github/` is **empty** — every difference is
documentation — and v0.50.0 (`c75f2ee8`) is green on `main`. So the three tags are verified
transitively and nothing shipped is unverified in substance. Each release note now says so
rather than leaving the gap to be discovered.

**Not retracted, and the reason matters.** The release-deletion procedure in CLAUDE.md is for a
BROKEN release. These are not broken; they lack a completed run on their own commit, which is a
process defect. Deleting them would break anyone who pinned a tag, and removing 0.49 alone
would restore the 0.48 → 0.50 gap that this ladder exists to prevent.

### The fix is a mechanism, not a resolution

**Required status checks on `main` are NOT configured and should be.** With them, `--merge`
would have been refused by GitHub regardless of what the operator intended:

```bash
gh api -X PUT repos/almide/almide/branches/main/protection --input - <<'JSON'
{
  "required_status_checks": {
    "strict": true,
    "contexts": [
      "Test Rust", "Test WASM", "Emit & Format",
      "Cross-Target (Rust vs WASM)",
      "Coq proofs + axiom audit + PCC gate",
      "WASM host-arch determinism"
    ]
  },
  "enforce_admins": false,
  "required_pull_request_reviews": null,
  "restrictions": null,
  "required_linear_history": false,
  "allow_force_pushes": false,
  "allow_deletions": false
}
JSON
```

`"strict": true` also forces the branch to be up to date before merging, which would have
caught the second half of this: `develop` moved under two of these PRs while they were open, so
the checks that were cancelled were cancelled for a reason worth surfacing.

This is the same discipline the repository already applies to everything else — the contract
ledger, the ratchets, the down-only counts. **A rule that depends on the operator remembering
is not a rule.** Until it is configured, the release procedure below is the fallback, and it is
strictly weaker.

### Until then: the release procedure has one added step

Between "merge" and "tag": **confirm every check on the PR reached `SUCCESS`**, by reading the
conclusions, not by trusting a merge flag.

```bash
gh pr view <N> --json statusCheckRollup \
  --jq '[.statusCheckRollup[]|select(.conclusion!="SUCCESS" and .conclusion!="SKIPPED")|{name,conclusion,status}]'
```

Empty output, and only empty output, is permission to tag. `CANCELLED` counts as not-verified.
