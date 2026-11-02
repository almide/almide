<a id="tool-qualification-data-package--the-index-574-do-330-shape"></a>

# ツール適格性評価データパッケージ — 索引（#574、DO-330 に沿った形）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](TOOL-QUALIFICATION.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](TOOL-QUALIFICATION.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](TOOL-QUALIFICATION.zh-CN.md)

> 固定原文の非規範読者用スナップショット。原文を正本とし、自動同期しない。歴史的状態・証明・適格性評価の主張は、現在の独立実行・監査・検証をせず保持する。DO-330 に沿った資料や適格性評価準備は認証取得ではない。範囲・仮定・未適格ツール・不足は記述どおり保持し、引用された運用コマンドはここでは実行しない。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/TOOL-QUALIFICATION.md)


適格性評価エンジニアが監査へ持参するパッケージ。これは新たな成果物ではなく索引であり、各節は既存の CI ゲート付き文書・台帳を指す。先にそれらを作るのは、適格性評価を再編成でなく組み立てにするためである。不足は末尾に名前付きで列挙する。このパッケージは認証状態を主張しない（判断は申請者と当局が行う）。

<a id="1-tool-identification-and-environment"></a>

## 1. ツールの識別と環境

- ツール、版の識別、主張範囲：`proofs/TOR.md` §1。リリースそのものは git tag であり、証拠状態は seal（`proofs/releases/<tag>.toml`）。CI が永続的に tag に照らして再測定する。
- ツールの下にあるコンパイラ：Ferrocene レーン（`scripts/check-ferrocene-lane.sh`、週次 workflow）は、記録された upstream 互換 pin である通常の rustc 1.95.0 で生成 Rust をテストする。このレーンは適格性評価済み Ferrocene バイナリへのアクセス待ちを明記する。upstream の版が一致しても通常の rustc に適格性が与えられるわけではない。
- リリースの完全性：各 asset は Sigstore attestation（`gh attestation verify <asset> -R almide/almide`）と checksum を持ち、依存ツリーを毎週 RustSec と照合監査する（`SECURITY.md`、#1534）。

<a id="2-tool-operational-requirements"></a>

## 2. ツール運用要件

`proofs/TOR.md` — 各要件はそれを強制する手段を名指しし、削除された手段を行が指すと `scripts/check-tor-refs.sh` が CI を失敗させる。証拠は構成上、運用者が再導出できる（TOR-2：CI は信頼基盤の外。ローカルで `make verify-trust && make receipt` を実行）。

<a id="3-tool-verification-data"></a>

## 3. ツール検証データ

- ゲート台帳：`proofs/gate-verification.toml`。判定を出す各ゲートを、その失敗の仕方で分類する（KERNEL_PROVEN / MUTATION_TESTED / NEGATIVE_TESTED / EXERCISED、UNVERIFIED 上限 0、減少のみ）。失敗する証拠を示していないゲートは出荷できない。
- 構造的カバレッジ：`proofs/coverage.sh` の後退防止ゲートとファイル別の安全性下限（`proofs/coverage-safety-baseline.txt`）。安全性集合の MC/DC 相当証拠は `proofs/mcdc-ledger.toml`。未解決の複数条件決定は 0 で、独立性ペアのベクトルを演算子交換 mutant で機械検証する（`proofs/mcdc-mutation.sh`、nightly）（#566、closed）。
- 証明基幹：`make verify-trust`（Rocq kernel + coqchk + axiom audit + PCC gate + corpus wall）。receipt は `proofs/receipt.sh` で、ツリーと toolchain の fingerprint を正直に扱う制約を伴う。

<a id="4-qualification-test-cases-derived-from-the-als"></a>

## 4. ALS から導出する適格性評価テストケース

採択済み ALS 節と契約へのリンクはリポジトリの適合性台帳で追跡する。Almide 側の導出は次のとおり：

- 契約台帳：`docs/contracts/contracts.toml`。cross-target 契約（現在の総数は台帳、日付付き集計は STAGE-STATUS）それぞれが ALS 節（`spec = "ALS-…"`）を引用する。各規範節を一つ以上の契約が引用し、各 fixture は双方向に結び付く（`scripts/check-contracts.sh`）。
- 実行可能なコーパス：`spec/`（run/check/ast parity manifest、630 以上の fixture に対する批准済み hash）、wall corpus、差分 fuzz 経路、3-way oracle（native / wasm / reference interpreter、abstain 台帳は縮小のみ）。
- 独立参照評価器（als の `ref/`、同じ規約で Ferrocene に pin）— N-version の経路。

<a id="5-formal-credit"></a>

## 5. 形式手法のクレジット

`proofs/FORMAL-CREDIT.md`（#575）— 七つの Rocq 証明 family を目標に対応した主張（FC-1..FC-7）として整理し、それぞれの仮定とカバレッジ境界、各行のモデル／実装の区別を示す。Lean の補助検査はクレジットから明示的に除外する。

<a id="6-known-limitations"></a>

## 6. 既知の制約

- リリースごと：seal の必須 `known_problems` フィールド。
- 恒常的：`docs/contracts/README.md` の信頼する行（特に IR→MIR）、設計上の wall（wall はエラーでなく拒否 — TOR-3、target-availability doctrine #1423）、契約台帳の要改訂一覧（現在 0、件数は減少のみ）。

<a id="7-configuration-management"></a>

## 7. 構成管理

git tag を識別子とし、seal を永続的に tag と照合して再測定する（CI の `scripts/release-seal.sh check`）。release-blocker gate（#1482）は未解決の I-severity issue がある final tag を拒否する。asset と dossier の Sigstore attestation、lockfile の変更ごとの依存監査レーンを備える。

<a id="8-the-assembled-bundle"></a>

## 8. 組み立てた bundle

`scripts/gen-dossier.sh <tag>`（#571）がリリース別 bundle を出力する：receipt + seal + formal credit + stage block + gate summary + KNOWN PROBLEMS 付き contract summary + MC/DC 状態 + sha256 input manifest。trust-spine の tag 実行で attestation を付け、リリースに添付する。

<a id="gaps-by-name-the-honest-bottom-line"></a>

## 名前付きの不足一覧（率直な結論）

| 不足 | Issue | 状態 |
|---|---|---|
| Critical profile の WCET 解析可能な codegen | #569 | 記述とゲート付き参照 kernel（`docs/project/WCET-STORY.md`）、ターゲット較正は外部 |
| 生成 Rust の行単位ソーストレーサビリティ | #572 | closed — `--trace-map` + review guide |
| 出荷時の runtime（allocator、free-list、RC op）の Lean/Rocq 証明範囲 | #576 | closed — StructuralRuntime の展開（`StructuralRuntime/Alloc/Decode/Run.v`）。出力バイトを証明済みの木へ decode し、木がモデル遷移を実現し、全実行が具体メモリ上の `RINV` を保持する。F-class は出力コードの補題違反となる。grow retry／未分類 bump は宣言した抽象化境界に残る |
| 適格性評価済み／最小の wasm 実行環境 | #865 | 未着手 |
| Critical-profile subset（`almide check --profile critical`） | #567 | closed — 全 fn の bounded profile、`--allow` 許可付き deny-all capability、subset 性質を CI が強制 |
| Critical profile の静的メモリモード | #568 | closed — 参照アプリは critical-clean、kernel 出力は割り当て不要、強制された固定 `--heap-cap` 予算内で完了。host 要件は `docs/project/PARTITIONED-RUNTIME.md` |
| `make verify` を通す飛行参照アプリ | #776 | closed — G-F4 chain を端から端まで証拠化。receipt が六段階すべてを導出する（`proofs/receipt.sh`） |
| 最初の dossier 付きリリース（#571/#1534 の closure を発動） | #571, #1534 | 組立・attestation の仕組みは稼働。個々のリリースに dossier が添付されたことを確認する |

このパッケージを読む適格性評価エンジニアは §2（運用者の作業）から始め、§3 の台帳を再導出できること（`make verify-trust`）を確認し、§4 のコーパスをサンプリングする。不足一覧が現時点で主張できる範囲の境界である。
