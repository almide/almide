# Structured Concurrency

## Overview
async/await の上位レイヤーとして、タスクのライフサイクル管理を提供する。

## API Design

```almide
// 全タスクの完了を待つ
let results = await parallel([
  fetch_data(url1),
  fetch_data(url2),
])

// 最初に完了したものを返す
let fastest = await race([
  fetch_from_cache(key),
  fetch_from_db(key),
])

// タイムアウト付き
let data = await timeout(5000, fetch_data(url))
```

## Design Principles
- fire-and-forget 禁止（全タスクがスコープ内で完結）
- キャンセル伝播（parentがキャンセルされたらchildも停止）
- AI生成コードでタスクリークが起きない

## Implementation Notes
- Rust: async task group + JoinHandle
- TS/Deno: Promise.all / Promise.race / AbortController
- WASM: シングルスレッド前提、cooperative scheduling

## Status
Not started. async/await基盤は実装済み。
