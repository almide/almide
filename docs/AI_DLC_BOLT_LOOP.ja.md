# Bolt ループ手順書 — 1 iteration でやること

> English (canonical): [AI_DLC_BOLT_LOOP.md](./AI_DLC_BOLT_LOOP.md) — 正は英語版。ループは英語版に従って動きます。

`/loop` で回す前提の手順書です。1 回の呼び出し = 1 iteration。
毎回「同期 → いまの段階を見る → 一手を打つ → 記録する」の順に進みます。
この手順書が許可している操作（push、issue の更新、普通の minor リリース）は、いちいち人間に確認しません。
人間を呼ぶのは [AI_DLC.ja.md](./AI_DLC.ja.md) の Mob ポイント（M0〜M6）だけです。

## 0. 同期と健康確認

```bash
git switch develop && git fetch origin && git pull --ff-only
gh run list --branch develop --limit 3
```

- develop の CI が赤なら、この iteration の仕事は赤の修理です。前に進める形で直します（fix forward）。
  自分が触っていないファイルの revert / checkout は絶対にしない — 他のエージェントの作業かもしれません。
- `git status` に見覚えのない変更があったら、触らずに M6 で人間を呼びます。

## 1. いまの Unit を確かめる

- リリース済み = 最新の `v*` タグ。いまの Unit = 台帳（ROAD_TO_1_0.md）で次の未リリース minor の行。
- その行・リンクされた issue・`docs/ai-dlc/units/<version>/`（あれば）を読みます。
- 行の Issue 欄が「—」なら、先に issue を作り、同じ commit で台帳にリンクを埋めます。
  作成が 403 で失敗したら `gh auth switch --user O6lvl4` してやり直し。

## 2. いまの段階を見て、一手を決める

Unit は「計画書（inception.md）」と「実行台帳（construction.md）」の対で進みます。
計画書の承認前に、作業は始めません。

- **`units/<version>/` がまだ無い → 計画書を書く回。**
  台帳の行と issue を根拠に、[inception-template.md](./ai-dlc/inception-template.ja.md) の形で書きます。
  数字は issue から引くこと。創作しない。commit + push したら、`mob` issue と通知で承認（M0）を頼み、
  この Unit は止めます。
- **計画書はあるが、承認記録が空 → 承認を確認する回。**
  `mob` issue を見て、承認されていたら承認者と日付を計画書に記録し、計画書の「Bolt 案」から
  実行台帳を作って（[construction-template.md](./ai-dlc/construction-template.ja.md)）、下の 3 へ。
  まだなら止めます（やってよいのは CI 赤の修理だけ）。
- **承認済み → Bolt を実行する回。**
  まず前回の Bolt の後始末: CI の結果を確認し、実行台帳のその行に証拠（commit SHA と CI run の URL）を
  書き込みます。それから、次の未着手 Bolt を 1 つ実行します。

## 3. Bolt を 1 つ実行する

- 着手前に、その Bolt の完了条件を実行台帳から言葉にします。
- 検証は最小限に。全量の審査は CI の仕事です:
  - 触った crate の `cargo test` がエラーゼロになってから push
  - 言語や stdlib から見える変更なら、該当ディレクトリの `almide test`
  - コンパイラを触ったら `make install`（PATH のバイナリを最新にする）
- **止まるべき地雷が 2 つあります:**
  - ターゲット間で観測できる挙動が変わるのに、同じ commit に contract（C-NNN）が無い → 止めて M2
  - ratchet や wall をゆるめないと緑にならない → 止めて M4。ゆるめる操作は選択肢に無い
- 計画とのずれ: 計画書の「やること」の範囲内なら、実行台帳の実行メモに書いて続行。
  範囲を超えるずれは M6 で人間を呼びます。

## 4. push と記録

- commit は英語 1 行、prefix なし。実行台帳の状態更新（状態 → 完了。証拠は次回の後始末で記入）も
  同じ commit に入れて push します。確認は取りません。

## 5. リリース判定

- 実行台帳の「Unit 完了判定」が全部埋まったら:
  - **普通の minor** → 自動でリリースします。`Cargo.toml` を bump → push → develop→main の PR →
    CI 緑でマージ（force-merge 禁止）→ マージ commit にタグ → release.yml に任せる →
    バイナリ 5 個と checksums を確認 → 済んだ issue を閉じる。
    （詳細な手順は `.claude/commands/almide-release.md`）
  - **節目（0.50 / 0.60 / 0.70 / 0.80 / 0.90）** → 監査ブリーフを issue に書いて M1。承認なしにリリースしない。
- リリース後、次の iteration は次の Unit の計画書から始まります。

## 6. 次にいつ動くか

- CI 待ち → 480〜600 秒後に起きる。それより細かく覗かない。
- 夜間 run など外部の時計待ち → 1 イベントにつき 1 回だけ起きる。待ち時間の先回りは
  「次の Unit の計画書を起草する」まで。実行台帳は作らない（承認前だから）。
- やれる Bolt が残っている → すぐ続ける。
- 全部が人間待ち → 通知する。単発実行ならここで停止。`/loop` で回しているなら、open な `mob` issue に
  監視を張り（新しいコメントとクローズが起床の合図）、長い心拍（30 分程度）で待機する —
  どこから返事をしても、ループはそこから再開する。

## 人間の呼び方

`mob` ラベルの issue に 4 点を書きます: **何が起きたか / 根拠 / 選択肢 / おすすめ**。
issue は英語で書きます（repo の慣例）。日本語で読める文書があるときは `.ja.md` をリンクします。通知を送ります。
独立の作業があれば続け、なければ止まります。
呼んだ結果「人間は要らなかった」となったら、それはこの手順書の欠陥です — 記録して、この文書を直します。
