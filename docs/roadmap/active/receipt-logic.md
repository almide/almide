<!-- description: Formal foundation for the trust layer — receipt logic: claim types, threat model, trust bases, falsification procedures, completeness relative to use-case -->
# Receipt Logic — 受領書の論理

> **Goal**: [trust-layer](trust-layer.md) の L0-L4 と receipts を形式的に精密化する。
> 「機械が書いたソフトウェアを、人間が読まずに信頼する」を、反証可能な主張の
> 束として定義し、「rustc 以外を完全にする」の到達条件を固定する。
> **位置づけ**: trust-layer.md のメタ層(主張の論理)。certification-grade.md
> (CG-4 translation validation)と completeness-by-construction.md(意味論台帳)
> はこの文書の §6 にぶら下がる。
> **Status**: Draft — 2026-06-11 起草。数値は同日 origin/develop 検証時点。

## §1 定義 — 「rustc 以外完全」の形式化

- **受領書 R(a)** — 成果物 a に付く型付き主張の束。各主張は 4 つ組
  **(主張文, 検証器, 信頼基底, 反証手続き)**。
- **信頼基底 TB(claim)** — その主張を受け入れるとき、消費者が無検証で信じる
  ものの集合。
- **公理集合 Axioms** = { rustc/LLVM, Lean カーネル, wasmtime + wasm 仕様,
  OS/ハードウェア, ALS の妥当性 }。
- **「rustc 以外を完全にする」** ≝ R(a) の全主張について TB(claim) ⊆ Axioms、
  かつ主張台帳が使用目的に対して閉じている(§6)。

設計制約(明文): **CI/GitHub は公理に入れない。** 全主張は消費者のマシンで
`git clone && make verify` により再導出可能であること。CI は礼儀としての
先行実行にすぎず、信頼の根拠ではない。

公理の最後の項(ALS の妥当性 — 「仕様が我々の意図通りか」)は証明不能で、
interp・dojo・使用によって経験的にしか確かめられない。**消えない底を明記する
ことが、残り全部を「完全」と呼ぶ資格になる。**

## §2 主張の型システム — 論理形式が違うものを混ぜない

L0-L4 の各主張は論理形式が異なる 5 型に分類される。型が違えば TB・検証器・
反証手続き・消費者が違う。混ぜた主張は精査で崩れる。

| 型 | 論理形式 | 検証器 | TB | 対応レベル |
|---|---|---|---|---|
| **C-SAFE** | ∀実行: a は X をしない(成果物単独) | import セクション検査 + manifest 照合 | wasm 仕様 + wasmtime | L0/L1 |
| **C-REPRO** | compile(s) = a(byte 等値) | 再コンパイル + diff | ツールチェーン一式 | L2 |
| **C-FAITHFUL** | behave(a) ≡ sem_ALS(s) | interp 三つ巴 + translation-validation 性質列 | ALS + interp + 公理 | L3 |
| **C-PROVEN** | ∀プログラム: パス P は性質 Q を持つ(全称・狭域) | Lean カーネル(lake build) | Lean カーネル | L3 |
| **C-MEASURED** | 生成工程の統計的主張 | dojo 再現走行 | 公開手順 + ピン留め一式 | L4 |

C-SAFE は**ソース不要・成果物だけで検証できる最強の型**。C-FAITHFUL は ALS を
信頼基底に含む最重の型。受領書の組み立ては軽い型から積む(§7)。

## §3 脅威モデル — 各主張が誰を止めるか

| 脅威 | 内容 | 止める主張 | 健全性の担い手 |
|---|---|---|---|
| **T1** 善意だがバグる AI 著者 | 主流ケース | 型/effect 系 + テスト + MSR ループ | チェッカー |
| **T2** 敵対的著者(prompt 注入されたエージェント) | capability 脱出を意図的に細工 | C-SAFE | 下記 3 層 |
| **T3** バグったコンパイラ | 受理 ⟹ 誤コード | C-FAITHFUL + 性質検証列 | 検証器(コンパイラ自身は信頼不要) |
| **T4** 悪意あるコンパイラ(Thompson 本体) | 自己増殖する汚染 | C-REPRO → DDC → selfhost | 多様性 |
| **T5** 汚染された検証インフラ | CI・検査器自体の侵害 | ローカル再検証 + 検査器の小ささ・多様性 | 消費者 |

### T2 の精密化 — L1 の健全性 3 層

「コンパイル時チェックを著者が騙したら?」への即答を可能にする分離:

1. **wasm import セクション = 構造的上界。** import に無いものは呼べない —
   健全性は wasm 仕様が担保する。言語解析ゼロ、`wasm-objdump` で誰でも検証
   できる。**敵対的著者に対しても健全。**
2. **言語 capability チェック = 粒度。** fs のパススコープ等。健全性ではなく
   精密性を足す層([effect-system-capability](effect-system-capability.md)
   Phase 1-2)。
3. **wasmtime = 実行時の床。** 防御の最終層。

騙しても無駄 — 上界は成果物自体が運んでいる。これが L1 を「コンパイラを
信じてください」から「成果物を見てください」に変える。

### T3 の精密化 — 検査の非対称性

10 万行のコンパイラを証明する必要はない。**生成者を検証する代わりに出力を
毎回検証する**(proof-carrying code の古典的非対称性)。信頼は
「コンパイラ + rustc」から「小さな検査器 + それを建てた rustc」に縮む。

現在稼働中のコンパイル毎検証(translation-validation 性質台帳の seed、5 性質):

| # | 性質 | 検証器 | モード |
|---|---|---|---|
| 1 | Perceus RC 均衡 | perceus_verify_function | hard-error(`--emit-unverified` waiver) |
| 2 | wasm モジュール妥当性 | wasmparser::validate | 常時・致死 |
| 3 | 名前解決全域性 | verify_names::assert_names_resolvable | 常時・両ターゲット |
| 4 | 型 concretize 完了 | AllTypesConcrete | hard |
| 5 | スタック平衡 | StackBalancePass | by construction |

「完全」への道はこの台帳を伸ばすこと(§6 U₂)であって、コンパイラ全証明
(CompCert 級、データ付きで棄却済み)ではない。

## §4 二重 rustc 非対称性 — native の信頼は wasm 経由で導出される

- **native 成果物**: rustc が **2 回**入る(コンパイラのビルド + 出力 Rust の
  ビルド)+ runtime/rs。
- **wasm 成果物**: rustc は 1 回(コンパイラのビルドのみ)。IR → バイト列まで
  全経路 Almide 所有。

帰結 1: **TCB 最小の成果物は wasm。** 受領書は wasm 成果物から付ける。
サンドボックス/エージェント戦略と信頼戦略はここで同じ答えに収束する。

帰結 2: **xtarget byte 一致ゲート(spec/wasm_cross、107 fixtures / 3-way)は
信頼の橋である。** TCB の大きい native 成果物は「wasm と観測等価」を経由して
信頼を輸入する。byte 一致は品質ゲートであると同時に、native の信頼導出経路
そのもの。

## §5 反証手続き — 主張の負空間を公開する

反証方法が公開されている主張だけが資産になる(反証不能な主張は信頼層では
負債)。各主張型に「これが観測されたら偽」を併記する:

| 主張 | 反証となる観測 |
|---|---|
| C-SAFE | import に無い操作の実行 PoC(1 件で死) |
| C-REPRO | 異ホストでの byte 不一致(1 件で死) |
| C-FAITHFUL | 受理プログラムでの interp/native/wasm 三者不一致 |
| C-PROVEN | `sorry`、lake build 失敗、または反例プログラム |
| C-MEASURED | 公開手順での再現走行の数字不一致 |

見つかった反証は台帳の次の行になる(バグバウンティと同じ力学)。

## §6 完全性の定義 — 使用目的に相対化して閉じる

「性質リストが観測意味論を被覆」は精密でない(絶対意味論の被覆は CompCert 級
に発散する)。正しくは:

> **完全性(U)** ≝ 使用目的 U に必要な主張集合 C(U) が列挙され、全主張が
> 検証器を持ち、全 TB が公理集合に含まれる状態。

- **U₁ = サンドボックス内エージェント実行**:
  C(U₁) = { capability 上界, エラー時終端挙動, 決定性, 再現性 } —
  ほぼ C-SAFE/C-REPRO で構成され、現有資産で四半期内に閉じうる。
- **U₂ = 無監督の本番運用**:
  C(U₂) = C(U₁) + C-FAITHFUL の被覆 — §3 の性質台帳の長征
  (現在 5/n)+ [completeness-by-construction](completeness-by-construction.md)
  の意味論台帳完済 + CG-1 oracle 反転。

**「Almide は完全」とは言わない。「U₁ に対して完全、U₂ に対して 5/n」と言う。**
この相対化が主張を反証可能かつ漸進可能にする。

想定異論: 「完全性の相対化は逃げではないか」。立場: 相対化しない完全性主張は
反証不能で、反証不能な主張こそ信頼層では逃げである。

## §7 着手順(受領書の積み方 — 軽い型から)

1. **Receipt v1**(C-SAFE + C-REPRO + C-PROVEN 現状範囲): wasm import 上界
   manifest + byte 再現ハッシュ + Lean 証明参照(44 定理、RC サブシステム
   スコープを明記)。capability Phase 1-2 が前提。
2. **Receipt v2**(+ C-FAITHFUL): CG-1 oracle 反転 + 性質台帳の公開と拡張。
3. **Receipt v3**(T4 対応): 再現ビルド → 2 バックエンドを使った DDC 型
   相互検証 → selfhost(`research/selfhost/`)。rustc そのものへの攻撃は
   v1/v2 完了後に初めて意味を持つ。

## trust-layer.md への反映(追補 4 点)

trust-layer.md 改訂時に取り込む(本文書はその形式基盤として残す):

1. 脅威モデル節(T1-T5)
2. レベル表に **TB 列**(各レベルの主張を受けるとき何を信じるか)
3. 反証手続き節(§5)
4. 公理節(CI 排除の明文化 + §4「信頼の橋」)

## 何をしないか

- **絶対意味論の完全被覆** — CompCert 級の発散。完全性は使用目的に相対化する(§6)。
- **公理の隠蔽** — rustc/LLVM・Lean カーネル・wasmtime・ALS 妥当性は消えない。
  消えない底の明記が、残りを「完全」と呼ぶ資格。
- **検証器の肥大** — 検査は生成より簡単であり続けること。検証器が育ちすぎたら
  それ自体が次の信頼問題になる(T5)。小ささと多様性で守る。
