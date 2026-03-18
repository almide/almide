# Showcase 2: Todo API (HTTP API)

**領域:** HTTP API server
**目的:** REST API。http.serve + json + codec の実用例。

## 仕様

```
almide run showcase/todo-api.almd
# GET  /todos       → 一覧
# POST /todos       → 作成
# GET  /todos/:id   → 取得
# DELETE /todos/:id → 削除
```

- インメモリストア (Map)
- JSON request/response
- effect fn による I/O 分離

## 使う機能

- `http.serve`, `http.response`, `http.json`
- `json.parse`, `json.stringify`
- `map.get`, `map.set`, `map.remove`
- `effect fn`
- `match` (リクエストルーティング)
- `string.starts_with`, `string.split`

## 成功基準

- [ ] Tier 1 (Rust) で動作
- [ ] curl でCRUD操作可能
- [ ] 80行以内
- [ ] README に使い方記載
