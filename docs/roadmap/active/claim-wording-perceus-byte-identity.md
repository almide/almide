<!-- description: Two claim-wording fixes so the public pitch is 100% backed by measurement -->
# Claim wording: Perceus phrasing and the byte-identity guarantee scope

公開クレーム（ツイート/README claims block）を実測どおり 100 点で言い切るための
表現調整 2 点。実装作業は不要 — 文言の確定と反映のみ。GitHub issue は EMU 制約で
このセッションから作成できないため、本文をここに正式記録（そのまま issue に転記可）。

## 1. 「Perceus RC で自動管理」→「Perceus 方式の所有権推論で自動管理」

厳密には:
- **wasm レッグ**: Perceus MIR の判定を RC（rc_inc/rc_dec + free-list）で実行。
- **Rust レッグ**: 同じ Perceus MIR の判定（Alloc/Dup/Drop/Consume、
  `verify_ownership` 認証済み）を Rust の clone/drop で実現。RC という**実行機構**は
  wasm 側の実現。

推奨文言: 「メモリ管理は Perceus 方式の所有権推論が自動決定（wasm は RC 実行、
Rust 出力は同判定を Rust の drop で実現）」。これなら両ターゲットに言い切れる。

## 2. 「出力バイト一致を保証」の範囲明示

推奨文言: 「観測可能出力（stdout/stderr/exit）のバイト一致を、契約台帳
（C-001..C-133、flagged 0）＋クロスターゲット差分ゲート（spec/wasm_cross 246
fixtures）＋ org 実プロジェクト両ターゲット検証で**継続的に保証**」。

明示除外（台帳で管理）:
- stdin / 乱数 / 実時間など本質的非決定性（各契約の記載どおり）。
- wasm 未実装 API（fs.glob 等）は**コンパイル/実行拒否**であり、間違ったバイトは
  出ない（honest-wall）。

## 背景（言い切りを支える今日の事実）

- 最後の既知乖離（mutable module var の snapshot alias — `--no-verified` v0-wasm が
  alias、native/デフォルト v1 は COW）は e088c25d 時点で**デフォルト経路で解消**し、
  C-033 の fixture `spec/wasm_cross/module_var_alias_cow.almd` として pin 済み。
  乖離が残るのは明示的 opt-out（`--no-verified`）のみ。
- 網羅的 match は checker が E010 で強制（実測確認済み）。
