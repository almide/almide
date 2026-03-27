<!-- description: Add concurrency limit parameter to fan.map -->
# fan.map Concurrency Limit

## 概要

`fan.map(xs, limit: n, f)` — 同時実行数の上限付き fan.map。

## 動機

`fan.map(urls, fetch)` で 1000 件 URL を処理すると 1000 スレッド同時 spawn → リソース枯渇。
`limit: 16` なら最大 16 並行で、残りはキューイング。

## 構文

```almide
let results = fan.map(urls, limit: 16, (url) => http.get(url))
```

## 実装方針

### Rust (thread backend)

セマフォ的な制御。`std::thread::scope` 内でチャンク分割 or ワーカープール。

```rust
std::thread::scope(|s| {
    let (tx, rx) = std::sync::mpsc::channel();
    let semaphore = Arc::new(std::sync::Semaphore::new(limit));
    for item in xs {
        let permit = semaphore.acquire();
        s.spawn(move || { let r = f(item); tx.send(r); drop(permit); });
    }
    // collect in order
})
```

注: `std::sync::Semaphore` は unstable。代替: `Arc<Mutex<usize>>` + `Condvar`、またはチャンク方式。

### TS

```typescript
// p-limit パターン or 手書き concurrency limiter
async function fanMapLimit(xs, limit, f) {
    const results = new Array(xs.length);
    let idx = 0;
    const workers = Array.from({ length: limit }, async () => {
        while (idx < xs.length) { const i = idx++; results[i] = await f(xs[i]); }
    });
    await Promise.all(workers);
    return results;
}
```

## 前提条件

- `fan.map` の named args サポート（`limit:` はデフォルト引数）
- チェッカーで `limit` パラメータの型チェック（Int）

## 優先度

低。現在の全件同時 spawn は小〜中規模で問題なし。大規模バッチ処理が必要になったら実装。
