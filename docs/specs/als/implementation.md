# ALS — 実装不変量（Implementation Invariants）

> Last updated: 2026-07-04

観測（stdout/stderr/exit）に直接は現れないが、メモリ安全・資源・決定性を
支える実装規範。契約台帳から `spec` キーで参照される。

## ALS-I1 参照カウント規律（Perceus）

heap 値の RC は Perceus 規律に従う: pass-through コンビネータは自分の
参照（+1）を返し、補間・spread・Value 構築の共有は +1 で釣り合い、
WASM heap は既定で回収される（プログラム終了時のリーク 0 が規範）。
検証は Lean 証明・emit-time Σ-probe・RC カウンタ fixture が担う。
Contracts: C-041, C-066, C-071, C-086, C-121, C-122, C-146, C-149。

## ALS-I2 コンパイラの決定性と資源

コード生成はホストアーキテクチャに依存せず決定的（同一入力 → byte 同一
出力）。コンパイルは幅広・深いネスト入力で native スタックを溢れさせない
（再帰下降の明示スタック化）。
Contracts: C-040, C-059。

## ALS-I3 v1 lowering エッジの等価証拠

v1（MIR）経路特有の lowering エッジ — scalar 値の端形・tuple/list Ok payload
の往復・自作 base64・条件 keep/skip の filter_map・借用パラメータの所有束縛
クラスタ・stdlib 呼び出し payload の ctor 直束縛・heap-Ok Result の値系
コンビネータ・ctor の if-payload・scalar リストリテラルの非 silent-empty 保証
— は v0 と byte 一致する（v1 検証行脚の固定化契約群）。
Contracts: C-075, C-107, C-109, C-116, C-120, C-138, C-139, C-143, C-144, C-152。
