<a id="native-projection-ownership-2067-2069-2070"></a>

# Native projection の所有権（#2067、#2069、#2070）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](native-projection-ownership-review.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](native-projection-ownership-review.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](native-projection-ownership-review.zh-CN.md)

> 固定原文の非規範的な履歴参照資料であり、原本を正本とする。自動同期しない。記載のテスト・測定・実装状態・保証は原文の主張であり、独立した再実行や現行の検証ではない。引用コードとコマンドは実行していない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/native-projection-ownership-review.md)

match 経由で `o.hit` を読むと Option の payload 全体を clone し、`list.get(xs, i)` や `xs[i].pos` を読むと record 全体をコピーしていた。record の再構築も、入力を所有し、以後使わない場合ですらフィールドを clone していた。

参照コンパイラは Rust で、`../almide-references/rust` に `0d31508599a7814a7044e9a7a871e3dc5f037753` で clone している。その `compiler/rustc_mir_build/src/builder/expr/as_operand.rs` は place と rvalue を区別し、オペランドの一時値のライフタイムを保持する。move は projection を含む Place に作用する。本レビューは、利用側が必要とする期間 place を借用できるかと、その所有者が projection されたフィールドを移譲できるかという 2 つの問いを分けて扱う。

読み取り専用の Option/Result match は、安定したフィールドの対象を借用するか、参照を返す list lookup を使う。パターン束縛は読み取りだけに使われなければならない。キャプチャ、変更、arm 内での元のソースの使用は最適化を見送る。借用したパターン束縛は所有集合から除く。終端 match の直前に隣接する単一使用の alias は、生存性を数える前に元の projection に畳み戻す。束縛と match の間に変更やその他の文がある場合、この畳み戻しを行わない。

安定した添字付き list 経由のメンバーアクセスは、添字で指定された record を借用する。境界と負の添字の振る舞いはいずれも変わらない。一時的な list を生成する呼び出しは、元の所有型の添字マクロを維持する。clone をそのマクロの外へ移動すると、一時的な入力の生存期間を超えてしまうためである。String の match は既存の MatchSubject coercion を維持する。

所有されたフィールドの最後の読み取りは move になる。残るルート使用のすべてが直接的な projection のいずれかである場合、record の再構築は複数の異なるフィールドを move できる。重複するフィールド、後続の record 全体の使用、ループ反復、再利用できるキャプチャでは、必要なコピーを保つ。借用推論はヒープフィールドの再構築入力を消費するものとして印付けし、呼び出し元の生存性解析は再利用される元の値を保持する。TCO 管理下の引数は引き続き TCO 自身の move 計画に従う。

既存の stream-fusion PR は、報告された純粋な list.map の形から Rc コールバックを除く。本変更は、要素ごとの入力フィールドの clone を除く。各出力 record には依然として、それぞれがキャプチャした String 値が必要であり、現在の所有型 String レイアウトではこの出力ごとの clone が必要である。

回帰テストは生成された Rust を確認し、native と WASM の両方を実行する。対象は直接／alias 経由の match、添字読み取り、存在しない／負の optional 添字、所有された再構築、重複フィールド、ソースの再利用、ソースの変更、キャプチャされた record、一時的な list 入力、string match である。

検証：native テストファイル全 439 件、既定 target のファイル全 439 件（WASM 427 件、native fallback 12 件）が合格する。nanopass テスト 43 件、codegen snapshot 13 件、loop-ownership テスト 11 件、projection 回帰テスト、borrow/capture 回帰テストが合格する。codegen は既存の codopsy しきい値で 90/A を維持し、commissioned WASM crate も 90/A を維持する。

CI の cross-target fixture `mg_rebuild_two_phase` により、さらに別のライフタイム境界が判明した。モジュールのグローバル `Var` は安定した Rust の place ではなく、clone された snapshot として出力される。共有されたキャプチャセルにも同じ境界がある。それらのルートに対する添字借用は所有型の添字経路を維持し、通常の局所／引数 projection は参照経路を維持する。固定版 Rust の `as_operand` 実装は、オペランドが一時値スコープの間だけ有効であると記述している。回帰テストは 2 回のグローバル list 再構築をまたぎ、呼び出し引数の読み取りとスカラーフィールドの読み取りの両方を実行する。
