<!-- description: Safe SQL execution with parameterized queries -->
# stdlib: sql [Tier 3]

データベースアクセス。パラメタライズドクエリ中心の安全な SQL 実行。

## 他言語比較

| 機能 | Go (`database/sql`) | Python (`sqlite3`) | Rust (`sqlx`/`rusqlite`) | Deno (`deno-sqlite`) |
|------|--------------------|--------------------|--------------------------|---------------------|
| 接続 | `sql.Open("sqlite3", path)` | `sqlite3.connect(path)` | `SqliteConnection::open(path)` | `new DB(path)` |
| クエリ (rows) | `db.Query("SELECT ...", args)` | `cursor.execute(sql, params)` | `sqlx::query("...").bind(v).fetch_all(&pool)` | `db.query(sql, params)` |
| クエリ (single) | `db.QueryRow("...", args).Scan(&v)` | `cursor.fetchone()` | `.fetch_one(&pool)` | `db.queryEntries(sql)` |
| 実行 (no result) | `db.Exec("INSERT ...", args)` | `cursor.execute(sql, params)` | `sqlx::query("...").execute(&pool)` | `db.execute(sql, params)` |
| パラメータ | `?` placeholder | `?` or `:name` | `$1` or `?` | `?` or `:name` |
| トランザクション | `db.Begin()` → `tx.Commit()` | `conn.commit()` | `pool.begin()` → `tx.commit()` | `db.transaction(() => {})` |
| マイグレーション | 外部ツール | 外部ツール | `sqlx migrate` | 外部ツール |

## 設計方針

Almide は SQL インジェクションを構造的に防ぐ。文字列連結でクエリを組み立てることを不可能にする。

```almide
// パラメタライズドクエリのみ
let users = sql.query(db, "SELECT * FROM users WHERE age > ?", [18])
let user = sql.query_one(db, "SELECT * FROM users WHERE id = ?", [id])
sql.execute(db, "INSERT INTO users (name, age) VALUES (?, ?)", [name, age])
```

## 追加候補 (~15 関数)

### P0 (SQLite)
- `sql.open(path) -> Result[Db, String]` — SQLite 接続
- `sql.close(db)` — 接続クローズ
- `sql.execute(db, query, params) -> Result[Unit, String]` — 実行（結果なし）
- `sql.query(db, query, params) -> Result[List[Map[String, Json]], String]` — 行取得
- `sql.query_one(db, query, params) -> Result[Option[Map[String, Json]], String]` — 1 行取得

### P1 (トランザクション)
- `sql.transaction(db, fn) -> Result[T, String]` — トランザクション内実行
- `sql.batch(db, queries) -> Result[Unit, String]` — バッチ実行

### P1 (便利関数)
- `sql.insert(db, table, record) -> Result[Int, String]` — レコード挿入、ID 返却
- `sql.count(db, query, params) -> Result[Int, String]` — COUNT 取得

### P2 (PostgreSQL)
- `sql.connect(url) -> Result[Db, String]` — PostgreSQL/MySQL 接続
- 同じ API で SQLite と PostgreSQL を切替可能

### P2 (スキーマ)
- `sql.tables(db) -> List[String]` — テーブル一覧
- `sql.columns(db, table) -> List[{ name: String, type: String }]`

## 実装戦略

@extern + x/ package。Rust: `rusqlite` (SQLite) / `sqlx` (PostgreSQL)。TS: `deno-sqlite` / `postgres`。
FFI (Rainbow FFI) が本格化する前は SQLite のみの Phase 1 で出荷可能。

## 前提条件

- FFI（@extern で Rust crate 呼び出し）
- Result[T, E] の E が String 以外（error モジュールと連携）
