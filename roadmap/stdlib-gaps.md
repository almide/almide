# 標準ライブラリの不足

## string
- `regex.match(pattern, str)` / `regex.find_all` / `regex.replace` — 正規表現がない
- `string.repeat(s, n)` — 繰り返し
- `string.index_of(s, needle)` — 位置検索
- `string.to_chars(s)` — 文字リストへの分解（現状はto_bytesのみ）
- `string.from_bytes(bytes)` — バイト列→文字列の復元

## list
- `list.zip(a, b)` — 2リストの結合
- `list.enumerate(xs)` — `[(0, x), (1, y), ...]`
- `list.flatten(xss)` — ネスト解除
- `list.group_by(xs, fn)` — グループ化
- `list.partition(xs, fn)` — 条件で2分割
- `list.sort_by(xs, fn)` — カスタム比較関数でのソート
- `list.take(xs, n)` / `list.drop(xs, n)` — 先頭N個の取得/スキップ
- `list.unique(xs)` — 重複排除
- `list.sum(xs)` / `list.product(xs)` — 集計

## math
- 現状モジュールが存在しない
- `math.sin`, `math.cos`, `math.tan`, `math.log`, `math.exp`, `math.pow`
- `math.pi`, `math.e` — 定数
- `math.min(a, b)` / `math.max(a, b)`
- `math.abs(n)` — 整数版（float.absはある）

## random
- モジュールが存在しない
- `random.int(min, max)` — 範囲指定のランダム整数
- `random.float()` — 0.0〜1.0
- `random.choice(list)` — リストからランダム選択
- `random.shuffle(list)` — シャッフル

## env
- `env.get(name)` — 環境変数の取得（Option[String]）
- `env.set(name, value)` — 環境変数のセット
- `env.cwd()` — カレントディレクトリ

## process
- モジュールが存在しない
- `process.exec(cmd, args)` — コマンド実行
- `process.exit(code)` — プロセス終了
- `process.stdin_lines()` — 標準入力の行読み取り

## time
- モジュールが存在しない（env.unix_timestampのみ）
- `time.now()` — 現在時刻
- `time.sleep(ms)` — スリープ
- `time.format(timestamp, pattern)` — フォーマット

## set
- データ構造が存在しない
- `Set[T]` 型 + `set.new()`, `set.add`, `set.contains`, `set.union`, `set.intersection`

## Priority
regex > env.get > list拡充 > process > math > random > time > set
