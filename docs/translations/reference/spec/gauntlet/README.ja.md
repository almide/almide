# DDD gauntlet

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)



> 原文: [spec/gauntlet/README.md @ 35297a](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/spec/gauntlet/README.md)。非規範的な読者向けスナップショット翻訳です。自動同期されません。計測値・主張の独立した再検証は行っていません。原文を正本とします。
>
> 原文上の注意: これは複数時点の記録です。「5 件未起票」などの見出し・古い件数と、表内の解決済み状態が一致せず、「五つの issue」に対して七つの ID が列挙されています。訳は原文を訂正せず保持しています。現在の未解決件数・状態を示すものとして読まないでください。

2026-08-22 から 2026-08-26 までに既存実装を壊した全形状を、一つの実行可能なマトリクスにまとめています。出発点は一つの試みでした。**普通の ports-and-adapters プログラムを書く**ことです。値オブジェクト、集約ルート、リポジトリ protocol、二つの adapter、application service を composition root で接続し、コンパイラがどう処理するかを見ました。

```sh
./run.sh /path/to/almide      # check / native / wasm for every cell
```

この試みから 24 件の欠陥を追跡しました（#1539、#1547、#1549–#1583）。既存実装では 22 件が解決し、2 件が未解決です。価値は修正そのものではありません。**適合性コーパスを再実行するだけでは一件も到達できなかった**こと、そして複数件が姿を変えた同じ誤りだったことです。

## 1. 完了条件が移動した

`NON-CARRYOVER.md` は完了を "the burn-up replays the whole 599-fixture corpus" と定義します。試み開始時のコーパスは 599 でした。現在は **`spec/wasm_cross` の 610 フィクスチャと `tests/*.rs` の新しい 4 件の native pin** であり、新規行はすべてこの試みで見つかった欠陥です。

```
spec/integration/modules/{convmut,genlib,layports}/…      spec/lang/protocol_fallible_return_test.almd
spec/integration/modules/cross_module_generic_chain_test  spec/lang/mut_param_whole_assign_test.almd
spec/integration/modules/cross_module_layered_ports_test  spec/lang/mut_param_protocol_bound_test.almd
spec/integration/modules/cross_module_mut_receiver_test   spec/lang/effect_option_qq_test.almd
spec/integration/modules/cross_module_receiver_mode_test  spec/wasm_cross/bang_in_record_literal_field.almd
spec/wasm_cross/fallible_tuple_value_tail.almd            spec/wasm_cross/record_pair_result_return.almd
spec/wasm_cross/result_option_call_payload_return.almd    spec/wasm_cross/result_tuple_variant_payload.almd
spec/wasm_cross/result_tuple_list_slots.almd              spec/wasm_cross/unwrap_or_option_payload.almd
spec/wasm_cross/variant_pair_result_return.almd           spec/wasm_cross/qq_record_fallback.almd
spec/wasm_cross/mut_param_effect_never_err.almd           tests/native_mut_param_pins_test.rs
spec/wasm_cross/variant_record_literal_equality.almd      tests/generic_letter_type_name_test.rs
spec/wasm_cross/fuel_cut_in_arm_loop.almd                 tests/nested_package_import_test.rs
```

**教訓は「数字を増やせ」ではありません。** コーパスは、すでに判断した事項の閉じた列挙です。誰も発見するプログラムを書いていない欠陥は含められません。それに依拠して完了を主張するなら、その限界を明示すべきです。Greenfield 版のゲートには第二軸が必要です。*tracker から収穫するのでなく、言語に対して書く、実プログラム形状の受入スイート*です。このディレクトリが出発点です。

## 2. 欠陥のクラスと、それぞれを排除する不変条件

### A. `check` は受理するが backend は出力できない — 最大のクラス

圧倒的に多く、ミッションで最も重要なクラスです。コンパイラは書き手にプログラムが正しいと伝えた後、その人が書いていない生成コードの rustc エラーを返します。

| セル | 内容 |
|---|---|
| `c1_cross_module_method` | convention method の receiver borrow mode がモジュール境界を越えず、参照渡し定義に対して呼出し側が値渡しした |
| `c2_mut_param_whole_assign` | `mut` param への `p = Record { … }` が `&mut` 束縛に対して `p = …` を出力した |
| `c3_mut_param_protocol_bound` | protocol bound の下で `mut` param を単相化すると参照渡し規約が失われた |
| `c4_derived_variant_repr` | derived-`Repr` variant の `.repr()` が derive の生成していない関数を呼んだ |
| `c5_generic_to_generic` | 別モジュールの generic fn を呼ぶ generic fn で、呼ばれる側が単相化されなかった |
| `c6_borrowed_param_field_store` | 借用 `String` param を field/slot に格納すると、所有値が必要な場所に `&str` を出力した |
| `c7_generic_letter_type_name` | `T`（または `A B C E F K U V`）というユーザー型が、モジュール境界を越えると型変数扱いされた |
| `c8_generic_applied_at_letter` | `Q` という型に `go[Q]` を適用すると単相化に失敗した |

**不変条件**: 型の正しいプログラムは出力可能でなければなりません。backend が lower できない形状は、*check 時にソースレベルの診断で拒否*します。受理してから後段で失敗させてはいけません。上の各行は、何が言語なのかについて frontend と backend が不一致な例です。Greenfield は一致を強制する場所を示せるべきであり、ユーザーを待たず、不一致を fuzz するゲートを持つべきです。

### B. 間違った名前空間から名前を解決 — 繰り返す系列

上の `c7`/`c8` に加え、`#1558`（リンクされた stdlib fn と裸の名前が同じユーザー fn が、書換えから黙って除外された。`fn replace` は wall、`fn bump` は実行）と、既存実装の以前の `#1087` です。毎回同じ形状です。**裸の名前を、別モジュールも書き込むテーブルで検索しています。** 影響範囲は見えず（どのモジュールをリンクしたかに依存）、安定しません（`import` の追加で無関係な関数が変わる）。

**不変条件**: 解決のキーは解決済みの識別情報であり、裸の表記ではありません。パスが名前をキーにする必要があれば、キーにスコープを含めます。一文字の型名も普通の名前です。

### C. 黙って間違った値 — ちょうど一件

`w1_qq_record_fallback`: record literal の fallback を持つ `Result[record, String]` に対する `??` が、間違った値を読みました。この集合で拒否でなく誤答した唯一の行です。fuzzer ではなく、*フィクスチャを広げる*ことで見つかりました。

**不変条件**: payload クラスに対応できない経路は処理を断ります。router の失敗は wall であるべきで、誤読であってはなりません。

### D. 書き手がつまずく表層の穴 — 2 件修正、5 件まだ未起票

| セル | 状態 |
|---|---|
| `s1_protocol_fallible_return` | 修正済み（protocol method 内の `-> T!` / `-> T!E` が parse error だった） |
| `s2_multiline_tuple` | 修正済み（tuple literal は改行できなかったが list/record/map はできた） |
| `s3_module_qualified_protocol` | **解決済み（#1589 stage 1）** — `ports.Store` を bound と conformance list で解析でき、型と同じく修飾子は宣言モジュールを指す必要がある。裸の cross-module 名も解決するが、修飾表記への machine fix を伴う deprecation warning になる |
| `s4_bare_mut_self` | **解決済み（#1589 stage 1）** — receiver を書くどこでも `mut self` は `mut self: Self` の省略形 |
| `s5_match_self` | **解決済み（#1590）** — `self` が parser の拒否識別子表に入り、識別子使用の先読みには `{` の場合がなかった。現在 `self` は本来の普通の識別子として解析され、セルは check 合格 |
| `s6_generic_protocol` | **解決済み（#1589 stage 1）** — `type UserRepo: Repository[User]` と `[R: Repository[User]]` は正確な引数置換で受理され、呼出しは宣言された一実装を実行する（C-365、`spec/integration/generic_protocol_ports/`） |
| `s7_protocol_as_type` | **正直に診断（#1590）、表現力は未解決（#1589）** — E029 は現在 "'Policy' is a protocol, not a type" と伝え、派生する `E002`s を抑制する。`List[Policy]` 形状では、まだ element-mismatch `E001` が最初に一つ出る（#1590 に記載）。existential dispatch は #1589 のまま |

**greenfield の不変条件**: これは bug ではなく設計判断です。意図的に決めてください。`self` の意味はどの位置でも同じにします。protocol の意味も一様にします（bound だけか、型でもあるか）。どの表記を正規形とするにせよ、他方が解析できる場所では正規形も解析できなければなりません。

### E. クロスターゲット同等性はマトリクスであり、セルごとに閉じられた

`x01`–`x16` は native で動き、wasm 側が追いつく必要があった形状です。学びになるのは*バーンダウンの形*です。五つの別 issue（`#1547 → #1564 → #1578 → #1579 → #1580 → #1581 → #1583`）が、結局一つのマトリクス（payload 種類 × producer × 表記 × 位置）のセルを一つずつ閉じました。毎回、完了コミットの文言（"completing the matrix cells"）が実際にカバーした範囲を超えて一般化し、次の probe が隣のセルを発見しました。

**不変条件**: 系列は、報告された一例の修正ではなく、*セルを列挙する実行可能なマトリクスゲート*で完了します。既存実装は stdlib API について `CLAUDE.md` にこの規則を掲げています（"API families are extended by matrix, never point-wise… a gated matrix cannot drift"）。単に codegen 系列には適用されていませんでした。Greenfield は構造的な規則にすべきです。lowering 経路が対応セルを宣言し、ゲートがその宣言を検査します。

## 3. 検証プロセスの教訓 — 最も転用できる部分

これらの発見には、個別 bug より費用がかかりました。

1. **誤った経路で動く回帰テストは何も固定しません。** この batch 用の四フィクスチャのうち二つは、*修正前*のコンパイラでも合格しました。`almide test` は形状が対応すればファイルを wasm 側で実行し、bug は native codegen にあったためです。repo には `tests/effect_tail_generic_bound_test.rs` に規則がすでにありました。*"a corpus file would run on the wasm leg and assert nothing"*。三日後にそれに違反しました。**全 pin は信用する前に壊れたコンパイラとの A/B 検証が必要です。**

2. **pin のない修正は、再び壊しても green のままです。** batch の四項目の一つはフィクスチャも `proofs/` 項目もなく取り込まれ、標本をコミットするために issue が必要でした。

3. **コミットメッセージだけにある標本は標本ではありません。** その項目は文章から五回再構成しても wall を再現できませんでした。実際の発火条件は説明より狭かったのです。記録から再現例を再構築できなければ、記録は証拠ではありません。

4. **部分 revert の A/B は、バイナリ全体の A/B より強い証拠です。** 一つの hunk を戻してフィクスチャの wall を確認すれば単一原因を示せます。親コミットでコンパイラ全体を再ビルドする方法が示すのは、そのコミットの*何か*が重要だったことだけです。「このフィクスチャがこの修正を固定する」の証拠基準は前者にします。

5. **wall メッセージは機構でなく原因を示すべきです。** `#1558` の wall は "mut params are not in this brick" と言いましたが、実際の修正は*関数名を変える*ことでした。そのメッセージから書き手は対処にたどり着けません。

## 4. 現在のスナップショット（既存実装 @ `4e5a15696`）

```
clean rows: 25    rows with a check/native/wasm gap: 9
```

9 件の内訳は、五つの未起票の表層の穴（`s3`–`s7`）、意図的な wasm decline 一つ（`x09`、`List[record]` 要素）、`x13`（record-literal field 内の call/operator/interpolation の下に入れ子になった `!` — **#1583、未解決**）、そして二つの layered package です。後者は `x13`（functional port）と `#1576`（mut port — `mut`-param の err-path 意味論に正式決定が必要。callee が変更後に `err` を返したとき caller の引数は何を保持するのか）で止まっています。`#1576` は構造的経路（0.61.2）で正式決定され解決しました。err が write-back より前に伝播するため、caller の slot は呼出し前の束縛を保持し、`pkg/mut_port` は下記 greenfield gate で native とバイト同一に動きます。#1871 は 2026-09-30 に逆方向（(B)、write visible、#2917）を採用しました。err arm も buffer を運ぶため、err より前の書込みは native と同様 caller に届きます。`cells/x18` がこれを固定します。

## 5. ここで何をするか

- greenfield が表層を扱えるようになったらすぐ `./run.sh` を実行します。各行は合成した edge case ではなく、実プログラムが生んだ形状です。
- §2 の不変条件を bug リストではなく設計レビューの問いとして扱います。frontend と backend が一つの言語に一致し、識別情報で解決し、系列をマトリクスで閉じる設計では、これらの多くのクラスは表現できなくなります。
- 五つの未起票の表層の穴（`s3`–`s7`）は、既存実装の未解決の言語上の問いです。Greenfield は現在の偶然を引き継がず、一度、意図的に答えを決められます。

## 6. greenfield gate（2026-08-26）

このディレクトリは greenfield の各経路でゲートされています。`crates/almide-wasm/tests/gauntlet.rs` が各セルを spine-front → emit → host で再実行し、各判定（`run` はバイト完全一致の stdout hash、`wall` は拒否理由、`reject` は front の最初の行）を `crates/almide-wasm/tests/golden/gauntlet-manifest.txt` に固定します。後退でも改善でも、差分は正式承認まで red です。

```sh
ALMIDE_UPDATE_GAUNTLET=1 cargo test --release -p almide-wasm --test gauntlet
```

`RUN_FLOOR`（増加のみ、burn-up 型）は、実行できるセルを降格する際に二か所の編集を必要にします。`run.sh` は既存実装側の比較 runner として残ります。初日に gate は二つの実際の greenfield 欠陥を捕まえました。mono clone が `mutated_params` を落とす問題（c3、黙って誤値）と、`io.print` に lowering がまったくない問題です。どちらも同日修正し pin を追加しました。
