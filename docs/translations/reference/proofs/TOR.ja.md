<a id="tool-operational-requirements-tor--the-almide-verification-chain"></a>

# ツール運用要件（TOR）— Almide 検証チェーン

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](TOR.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](TOR.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](TOR.zh-CN.md)

> 固定原文の非規範読者用スナップショット。原文を正本とし、自動同期しない。歴史的状態・証明・適格性評価の主張は、現在の独立実行・監査・検証をせず保持する。DO-330 に沿った資料や適格性評価準備は認証取得ではない。範囲・仮定・未適格ツール・不足は記述どおり保持し、引用された運用コマンドはここでは実行しない。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/TOR.md)


DO-330 に沿った形の運用契約：**チェーンの主張を自分の利用に適用するため、運用者が何をしなければならず、何を仮定してはいけないか**。これは目標に向けた Stage-5 成果物三点の第三（ツール検証台帳 = `proofs/gate-verification.toml`、既知問題の形式化 = 全リリース seal に必須の `known_problems` フィールド）。境界を説明する関連文書は `proofs/TRUSTED_BASE.md`（信頼するものと証明したものの区別）、`docs/contracts/README.md`（各ゲートの主張範囲と非主張範囲）。これら全体の DO-330 形式パッケージ索引は `proofs/TOOL-QUALIFICATION.md`（#574）。

各要件は強制する手段を `enforced-by:` で示すか、運用者に依存する `procedural` と明記する（将来の手段を予定している場合は名指す）。`scripts/check-tor-refs.sh` が全 `enforced-by:` パスの解決を確認し、削除された手段を指す TOR 行は CI を失敗させる。

<a id="1-tool-identification"></a>

## 1. ツールの識別

- ツール：Almide コンパイラと、証明を携える検証チェーン（`make verify-trust` = `proofs/check.sh` + `proofs/gate.sh` + `proofs/structural-wall.sh` + `proofs/corpus-wall.sh`（native MIR 証明書）+ `proofs/native-wall.sh` + MIR と almide-verify のテスト。`make receipt` = `proofs/receipt.sh`）。
- 成果物の識別（#2760）：receipt の C-SAFE 行は `proofs/structural-artifacts.sha256` と、そのファイル自身の digest を名指す。証明書の対象 fixture ごとに一行で、`almide build --target
  wasm` が出荷するバイト列の SHA-256 を示す。各証明書 bundle はその行を固定し、`almide-verify` はファイルから再計算する。主張対象はビルドではなく、そのバイト列。
- 版の識別：リリースは git TAG。その証拠状態は `proofs/releases/<tag>.toml` の seal（CI が tag に照らして永続的に再測定）。
- 主張範囲：receipt の主張表（C-PROVEN / C-SAFE / C-REPRO / C-WCET / C-FAITHFUL）。その外は何も主張しない。

<a id="2-operational-requirements"></a>

## 2. 運用要件

**TOR-1 — 証拠は stamp されたツリーに結び付く。** 各判定は、生成した際の正確なツリー + toolchain fingerprint についてだけ有効。PATH のバイナリが workspace build と一致しないとゲートは実行を拒否する。`make install` を再実行し、再検証する。ツリー間で判定を移植しない。
enforced-by: proofs/lib/stamp.sh

**TOR-2 — 自分の機械で再導出する。** CI は信頼基盤の外。チェーンの保証を主張する運用者はローカルで `make verify-trust &&
make receipt` を実行しなければならず、receipt が主張の記録となる（CI は便利なミラーであり証拠ではない）。
enforced-by: proofs/receipt.sh

**TOR-3 — wall は拒否であり、回避するべきエラーではない。** 検証済み部分集合の外のコードには明示的 wall が出て、主張はない。構造的経路で lower できないプログラムは E082 拒否となり、witness が対象としない frame は理由を計数して DECLINE する。したがって、そのプログラムは証明書対象集合の外にあり C-SAFE 主張を持たない。wall が消えるまでコードを書き換えることは、主張する部分集合へ入れること。wall の抑制・パッチによる迂回・無視は、そのプログラムの全主張を無効にする。
enforced-by: proofs/structural-wall.sh

**TOR-4 — 主張は receipt の該当行そのものだけを意味する。** C-SAFE は、STRUCTURAL build の wasm について、該当行が数える証明書の対象 fixture のメモリ・名前・capability の健全性を表す。機能的正しさは意味しない（PID controller はメモリ安全でも誤った対象を制御し得る）。証明書対象集合外のプログラムには C-SAFE 主張が一切ない。C-SAFE と C-PROVEN は receipt の主張で、docs/contracts/contracts.toml の行ではない。receipt の信頼される記録／projector の仮定と helper の除外を保持し、任意の出荷プログラムについて四つすべての `MemorySafe` 節を証明するものではない。要件レベル検証（MC/DC、要件トレース）は運用者の領域プロセスであり、このツールの範囲外。
enforced-by: proofs/receipt.sh

**TOR-5 — 依存する前に、そのリリースの既知問題を確認する。** 全リリース seal が `known_problems` entry を持つ（checker が強制し、空にはしない）。運用時はそれをレビューし、対処方針を適用する。そこで開示した問題はいかなる主張の対象にもならない。
enforced-by: scripts/release-seal.sh

**TOR-6 — seal が名指す toolchain を固定する。** seal はリリース検証時の Rust と wasmtime の版を記録する（Coq の pin は TRUSTED_BASE）。版の置換は証拠を無効にする。逸脱が必要なら TOR-1/TOR-2 に従って再検証する。
enforced-by: scripts/release-seal.sh

**TOR-7 — ゲート検証台帳に応じて判定の重みを考える。** 失敗方向を一度も実証していないゲートは UNVERIFIED に分類する（減少のみの上限、burn-down #1244）。UNVERIFIED の green は NEGATIVE_TESTED/KERNEL_PROVEN の green より弱い証拠であり、台帳がその区別を示す。
enforced-by: scripts/check-gate-verification.sh

**TOR-8 — 実行可能仕様が不一致を裁定する。** 参照インタープリタが投票する箇所（三者コーパス）では、interp の挙動が仕様の解釈となる。ターゲット二者が互いに一致しても、それと異なるターゲットは誤り。棄権は分類し縮小のみとする。棄権領域には仕様テキストと契約はあるが、実行可能な第三票はない。
enforced-by: scripts/check-abstain-classes.sh

**TOR-9 — wasm 実行環境は適格性評価されていない。** wasmtime は pin されているが、適格性の裏付けを持たない（現時点で DO-330 の道筋はない）。Critical-profile 配備は、自らの safety case で wasm runtime を未適格 COTS component として扱わなければならない。手続き的要件 — #865 で追跡。

**TOR-10 — 成果物を、それを作った build でなく bundle と照合する。** 証明書 bundle（version 2）は witness が記述するファイルの SHA-256 を示す。出荷された `.wasm` に依拠する前に、その隣で `almide-verify bundle <bundle>` を実行する。build 後に一バイト変わるなどの不一致は、witness の内容にかかわらず REJECT となる。`artifact` 記録のない bundle（version 1）が証明するのは witness であり、ファイルではない。
enforced-by: crates/almide-verify/src/bundle.rs

<a id="3-known-limitations-operational-view"></a>

## 3. 既知の制約（運用面）

| 制約 | 運用上の帰結 |
|---|---|
| このゲート群は領域レベルの MC/DC 適格性を確立しない | 行／条件カバレッジ、安全性集合の決定ベクトルは範囲を限定した証拠であり、領域カバレッジの包括的クレジットではない。必要な評価を領域プロセスで計画する |
| C-WCET は未完了（counted-loop の要石） | 割り当て／時間上限の主張はない。receipt から時間予算を導出しない |
| wasm 経路の C-FAITHFUL は部分的（runtime の四 RC routine は bytes-to-semantics が証明済みだが、ほかの全出力本体と、プログラム命令から所有権 witness への結び付きは未証明） | RC runtime より外側の structural build の挙動は、証明済みでなく差分テスト済み（native / wasm / interp のバイト一致）として扱う |
| native 経路の C-FAITHFUL は未完了（production Rust renderer + Ferrocene） | native のオブジェクトコードへの信頼は適格 toolchain でなく rustc を通る |
| fuzz の耐久連続記録はまだ短い | 耐久指標（90日 milestone）は積み上げ中。引用前に現在の連続記録を確認する |

<a id="4-traceability"></a>

## 4. トレーサビリティ

`scripts/check-tor-refs.sh`（CI）が TOR 行と強制手段の対応を正直に維持する。各リリースの証拠経路は、seal → tag → その tag のツリーでの `make verify-trust` + `make receipt` → 運用方法を示すこの TOR。

> 原文範囲の注記：TOR-8 はこの原文の断定的な裁定規則を再現する。一方、同じ固定時点のインタープリター指示は、不一致がインタープリターのバグを示す場合も認めている。原文間の表現差をここでは解消せず、現在の動作も確定しない。
