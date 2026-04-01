<!-- description: Capability-based effect system for AI agent harness containers -->
# Effect System: Capability-Based Agent Containers

Almide の effect system を「AI エージェントが安全にコードを実行するためのハーネスコンテナ」の信頼プロトコルとして設計する。

```
Human → タスク + capability manifest → Agent → Almide コード生成
  → Compiler: capability 準拠をコンパイル時に証明
  → WASM binary: 宣言された capability の WASI import だけ持つ sandbox
  → Runtime: WASI が最終防壁
```

## 設計方針: Invisible Effects

- ユーザー/LLM は `fn` / `effect fn` だけ書く（変更なし）
- capability の粒度は `almide.toml` で宣言（コードに現れない）
- コンパイラが内部的に 14 カテゴリの effect を追跡
- WASM binary には必要な WASI import だけ含まれる

## References

- **Deno**: runtime permission flags with path/domain scoping
- **WASI**: pre-opened file descriptors, capability-based
- **Austral**: capabilities as linear types, compile-time verification (~600 LOC)
- **Pony**: object capabilities + deny capabilities
- **Component Model + WIT**: type-safe capability composition

## Capability Taxonomy（14 カテゴリ）

| Category | Stdlib modules | WASI imports |
|----------|---------------|-------------|
| `FS.read` | fs.read_text, fs.exists, fs.stat, fs.list_dir | path_open(read), fd_read |
| `FS.write` | fs.write, fs.remove, fs.mkdir_p, fs.rename | path_open(write), fd_write(fd>2) |
| `Net.fetch` | http.get, http.post, http.put, http.delete | host-provided fetch |
| `Net.listen` | http.serve | host-provided listen |
| `Env.read` | env.get, env.args, env.cwd, env.os | environ_get, args_get |
| `Env.write` | env.set | environ_set |
| `Proc` | process.exec, process.exit | host-provided proc |
| `Time` | datetime.now, env.millis | clock_time_get |
| `Rand` | random.int, random.float | random_get |
| `Fan` | fan { }, fan.map | (internal) |
| `Log` | log.info, log.debug | fd_write(stderr) |
| `IO.stdin` | io.read_line | fd_read(fd=0) |
| `IO.stdout` | println, io.print | fd_write(fd=1) |

後方互換: `IO` = FS.read + FS.write + IO.stdin + IO.stdout, `Net` = Net.fetch + Net.listen

## Manifest（almide.toml）

```toml
[permissions]
allow = ["Net.fetch", "IO.stdout"]

[permissions.scope]
net.fetch = ["api.weather.com"]    # ランタイム強制（コンパイル時証明不可）

[dependencies.json_parser]
allow = []                          # pure only

[dependencies.http_client]
allow = ["Net.fetch"]
deny = ["Proc"]                     # 明示的拒否
```

## Ty::Fn への Effect 統合（内部のみ）

```rust
Fn { params: Vec<Ty>, ret: Box<Ty>, effects: EffectSet }  // EffectSet = u16 bitset
```

HOF での effect 伝播が型レベルで追跡可能に:
- `list.map(effect_fn, xs)` → 結果は effect
- pure fn 内で effect_fn を引数に渡す → E006 エラー

## WASM Container Model

- `allow = ["Net.fetch", "IO.stdout"]` → fd_write + host fetch のみ import
- 不要な WASI import は binary に含まれない
- ビルド時に `manifest.json` を同時出力（orchestrator 向け監査証跡）

## Multi-Agent: spawn（将来）

```almide
effect fn main() -> Unit = {
  let data = spawn["Net.fetch"] {
    http.get("https://api.weather.com/v1/tokyo")!
  }!
  spawn["FS.write", "IO.stdout"] {
    fs.write("/output/weather.txt", data)!
    println("Done")
  }!
}
```

## やらないこと

- Algebraic effect handlers
- Effect row syntax（`fn[IO, Net]`）
- ユーザー定義 effect カテゴリ
- ランタイム capability request

## Implementation Phases

| Phase | What | Scope |
|-------|------|-------|
| **3A** | Capability taxonomy 14分割 | S-M |
| **3B** | EffectSet in Ty::Fn | L |
| **3C** | Per-dependency restriction | S |
| **3D** | Capability-aware WASM emit | M |
| **3E** | spawn[caps] { body } | L |

3A → 3C は独立して shippable。3B は型システム変更で最も重い。3E は orchestration story が固まってから。

## Supersedes

- on-hold/effect-type-integration.md (Phase 4 → 統合)
- on-hold/secure-by-design.md Layer 3-5 (capability inference → 統合)
