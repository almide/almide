[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](almide-stats.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](almide-stats.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](almide-stats.reader.zh-CN.md)

> 過去のリポジトリコマンドを読むための非活性・非規範スナップショット。正本は変更しない。この参照パスと .reader 名はエージェントコマンドを登録しない。引用された指示は資料であり、実行・支出・公開・push・削除の権限ではない。自動同期せず、現在の workflow や測定主張を独立検証したものではない。コードとコメントは原文を保持する。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/.claude/commands/almide-stats.md)

<a id="update-published-measurement-stats"></a>

# 公開測定統計の更新

先頭 README は `docs/benchmarks/README.md` にリンクする。そこに生成ブロックと日付付き測定を置く（`scripts/lib/readme-targets.sh`）。公開数値はすべて、tree 内の出典からスクリプトで描画するか、測定日を付ける。日付等のない数値があれば `scripts/check-readme-numbers.sh` が CI を失敗させる。したがって「統計の更新」とは再生成を意味し、測定するのは記録済み baseline が出典となる場合だけである。

<a id="regenerate-no-measurement--derives-from-the-tree"></a>

## 再生成（測定せず、tree から導出）

```bash
bash scripts/gen-readme-stats.sh        # stdlib functions/modules (from docs/stdlib signature indexes),
                                        # spec/ test files, contract count; renders the Hello, world
                                        # size table from docs/benchmarks/wasm-size.txt
bash scripts/gen-claims.sh              # contract-ledger claims block + proofs/STAGE-STATUS.md
almide run tools/almide-gates/src/main.almd -- bench docs/benchmarks --readme
                                        # build-speed block from docs/benchmarks/build-speed.txt
```

原文コメントの説明：最初のコマンドは stdlib の関数・モジュール数（docs/stdlib のシグネチャ索引から）、spec/ のテストファイル数、契約数を扱い、docs/benchmarks/wasm-size.txt から Hello, world のサイズ表を描画する。次は契約台帳の主張ブロックと proofs/STAGE-STATUS.md。最後は docs/benchmarks/build-speed.txt からビルド速度ブロックを生成する。

lefthook の pre-commit hook は入力が変わるたびに最初の二つを実行する。手動実行は新しい checkout や stdlib 変更後のためである（まず `make stdlib-docs` でシグネチャ索引を再生成）。

<a id="re-measure-only-when-the-compiler-changed-the-thing-being-measured"></a>

## 再測定（コンパイラが測定対象を変えた場合だけ）

```bash
bash scripts/gen-readme-stats.sh --measure     # Hello, world on the structural wasm leg → docs/benchmarks/wasm-size.txt
almide run tools/almide-gates/src/main.almd -- bench docs/benchmarks
                                               # build-speed baseline → docs/benchmarks/build-speed.txt
```

原文コメントの説明：最初は structural wasm 経路の Hello, world を docs/benchmarks/wasm-size.txt に記録し、次はビルド速度 baseline を docs/benchmarks/build-speed.txt に記録する。

どちらの baseline にもバイナリ版と日付を含める。CI は Hello, world を再ビルドし、記録したバイト数との一致を求める。そのため emitter が変わったらここで再記録し、生成された測定ブロックを手編集しない。

<a id="msr"></a>

## MSR

ここではなく [almide-dojo](https://github.com/almide/almide-dojo) が測定する。新しい実行結果が入ったら、`docs/benchmarks/README.md` の scorecard 表と日付を更新する（その行に `YYYY-MM-DD` を保持しなければ数値 gate が拒否する）。

<a id="check"></a>

## 検査

```bash
bash scripts/gen-readme-stats.sh --check && bash scripts/check-readme-numbers.sh && bash scripts/gen-claims.sh --check
```

コミットは1行、例えば `Regenerate README stats after the stdlib change` とする。
