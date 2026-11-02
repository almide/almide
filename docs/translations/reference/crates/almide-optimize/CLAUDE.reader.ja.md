[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE.reader.zh-CN.md)

> エージェント指示を人が読むための、非規範・原文固定の写し。正本 CLAUDE.md は変更しない。CLAUDE.reader 名は自動読込される指示ファイル名を意図的に避ける。歴史的な規則であり、新しい指示階層でも、実装や安全性を独立検証した保証でもない。自動同期せず、コードと inline は原文を保つ。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-optimize/CLAUDE.md)

> 引用された指示は歴史的な参照資料であり、コマンド実行・実装変更・commit・push・公開の権限ではありません。

<a id="almide-optimize"></a>

# almide-optimize

IR レベルの最適化パス：単相化、不要コード除去、定数伝播。

<a id="monomorphization-mono"></a>

## 単相化（`mono/`）

具体的な型引数に対し、ジェネリック関数を特殊化する。

- **frontier に基づく発見** — 最初のラウンドは全関数からジェネリック呼び出し箇所を探索する。次のラウンド以降は新しく作った特殊化だけを探索し、新しいインスタンスがなくなると収束する。
- **構造的制約** — `T: { name: String, .. }` の制約を具体的な型（Dog、Person など）へ解決する。
- **暴走防止** — 特殊化が1000を超えると警告して停止する（無限展開の可能性）。

<a id="optimization-optimize"></a>

## 最適化（`optimize/`）

- **DCE** — 呼び出し箇所がゼロの関数と未使用の変数束縛を除去。
- **定数伝播** — 定数式を畳み込み、既知の値を伝播する。

<a id="rules"></a>

## 規則

- **Mono は codegen より前。** mono 後は IR に `TypeVar` が残らないようにする（到達不能経路を除く）。
- **Mono は関数を複製する。** 特殊化した関数は型を置換したコピー。元のジェネリック関数は非ジェネリックな呼び出し箇所があれば残す。
- **DCE は保守的に行う。** 到達不能と証明された関数／変数だけを除去する。副作用のある式（effect 関数、代入）は結果が未使用でも残さなければならない。
- **最適化で意味を変えない。** 各最適化パスは観測可能な振る舞いを保持する必要がある。境界事例の回帰テストを追加する。
