# Monomorphization [ACTIVE]

Named rows (`..R`) と container protocols の Rust codegen に必要な、関数のモノモーフィゼーション基盤。

## Why

現在の Rust codegen は 1 関数 = 1 Rust 関数。open record の Phase 1 は field projection（呼び出し側で必要なフィールドだけ AlmdRec に詰める）で回避したが、以下は projection では不可能:

| Feature | 問題 |
|---------|------|
| Named rows `..R` | 関数が残りフィールドを **保持して返す** 必要がある。projection は捨てるので矛盾 |
| Container protocols `F: Mappable` | `List` と `Option` で異なる `.map()` 呼び出しを生成する必要がある |
| Generic bounds `T: { name, .. }` | `T` の具体型ごとに `.name` アクセスのコードが異なる |

共通解: **呼び出し時の具体型ごとに関数を複製（monomorphize）** する。

## Current codegen model

```
fn greet(a: { name: String, .. }) → fn greet(a: AlmdRec0<String>)

// 呼び出し側が projection:
greet(dog) → greet(AlmdRec0 { name: dog.name.clone() })
```

- 1 function : 1 Rust function
- Open record → AlmdRec struct + call-site projection
- 型情報は呼び出し側が持ち、関数本体は AlmdRec しか知らない

## Target codegen model

```
fn rename[R](x: { name: String, ..R }, new: String) -> { name: String, ..R }

// Dog で呼ばれた → Dog 版を生成:
fn rename__Dog(x: Dog, new: String) -> Dog { Dog { name: new, ..x } }

// Person で呼ばれた → Person 版を生成:
fn rename__Person(x: Person, new: String) -> Person { Person { name: new, ..x } }
```

- 1 function × N concrete types = N Rust functions
- 関数本体が具体型を知っている → field access, spread, return が型安全
- Named row `..R` は具体型の残りフィールドとして解決される

## Design decisions

### Monomorphization scope

- **関数単位**: generic param / row variable を持つ関数のみ対象
- **非 generic 関数**: 現行モデル（1:1）のまま
- **Phase 1 open records (anonymous `..`)**: 現行の field projection を維持。モノモーフィゼーション不要

### Instantiation discovery

Call graph を走査して、各 generic/row-polymorphic 関数がどの具体型で呼ばれるかを収集する。

```
collect_instantiations(ir_program) → HashMap<fn_name, Vec<ConcreteTypes>>
```

- Direct calls: `rename(dog, "Jiro")` → `rename` に `(Dog, String)` を記録
- Chain calls: `rename(x, "Jiro")` where `x: { name, breed, ..R }` → R の具体型は呼び出し元から伝播
- Transitive: A が B を呼び、B が C を呼ぶ場合、C の instantiation は A の具体型から決まる

### Name mangling

```
rename[R=Dog] → rename__Dog
rename[R=Person] → rename__Person
rename[R={name: String, age: Int}] → rename__AlmdRec3
```

- Named type → type name suffix
- Anonymous record → AlmdRec suffix
- Multiple type params → underscore 区切り: `transform__List_Int`

### Interaction with borrow inference

現在の borrow inference (`src/emit_rust/borrow.rs`) は関数シグネチャから ownership を決定する。モノモーフィゼーション後は具体型ごとにシグネチャが異なるため、borrow analysis を instantiation ごとに実行する必要がある。

### TypeScript target

TS は structural typing なのでモノモーフィゼーション不要。Named rows は型注釈として出力し、runtime では erased。

## Implementation phases

### Phase 1: Infrastructure

- [ ] IR に instantiation info を付与: `IrFunction` に `type_params: Vec<String>` 追加
- [ ] Call graph walker: `collect_instantiations()` — 各 generic fn の concrete call types を収集
- [ ] Name mangling: `mangle_fn_name(base, concrete_types) -> String`
- [ ] Emitter に instantiation table: `HashMap<(fn_name, concrete_types), mangled_name>`

### Phase 2: Codegen

- [ ] `emit_fn_decl` 分岐: generic fn → instantiation ごとに concrete 版を emit
- [ ] `gen_ir_call` 分岐: generic fn call → mangled name に置換
- [ ] Type substitution: 関数本体の `TypeVar` / row variable を concrete type に置換してから codegen
- [ ] Struct spread (`x { name = new }`) が concrete type で正しく動くことを保証

### Phase 3: Named rows integration

- [ ] Parser: `{ field: Type, ..R }` — row variable name capture
- [ ] Checker: row unification — `R` を remainder fields にバインド
- [ ] Codegen: row variable → concrete type substitution + monomorphized function emission
- [ ] `substitute()` 拡張: `Ty::OpenRecord { row_var: Some("R") }` + bindings → `Ty::Record { fields: merged }`

### Phase 4: Container protocols integration

- [ ] `F: Mappable` → `F` の concrete type ごとに monomorphized function
- [ ] Protocol method dispatch: `xs.map(f)` → `Vec::into_iter().map(f).collect()` or `Option::map(f)` depending on concrete `F`

## Affected files

| File | Change |
|------|--------|
| `src/ir.rs` | `IrFunction` に type_params 追加 |
| `src/lower.rs` | Generic info を IR に伝播 |
| `src/emit_rust/program.rs` | Instantiation-based emission |
| `src/emit_rust/ir_expressions.rs` | Call dispatch to mangled names |
| `src/emit_rust/mod.rs` | Instantiation table, mangling |
| `src/emit_rust/borrow.rs` | Per-instantiation borrow analysis |
| `src/types.rs` | Row variable substitution |
| `src/check/calls.rs` | Row unification in user fn calls |

## Risk

- **Code size explosion**: N types × M functions = N×M Rust functions。実際は small N (< 10) が多いので許容範囲
- **Recursive generics**: `fn f[R](x: { items: List[{ name: String, ..R }], .. })` — nested row variables。Phase 3 では flat row のみサポートし、nested は Phase 4+ に延期
- **Compile time**: Instantiation discovery は O(calls × types)。プログラムが大きくなった時の性能は要監視

## References

- Rust: 全 generic 関数を monomorphize。LLVM が dead code elimination
- Go 1.18: Stenciling + GC shape。辞書ベースで code size を抑制
- MLton (SML): Whole-program monomorphization。Almide に最も近いモデル
