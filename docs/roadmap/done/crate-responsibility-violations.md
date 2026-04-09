<!-- description: Fix 33 CLAUDE.md rule violations found by crate patrol audit -->
<!-- done: 2026-04-06 -->
# Crate Responsibility Violations

2026-04-06 のパトロール監査で 33 件の CLAUDE.md ルール違反を検出。**全 33 件対応完了。**

## Must Fix — ALL DONE

### ~~1. almide-ir: fold.rs に 14 ノードの再帰漏れ~~ DONE
### ~~2. almide-codegen: walker に target-specific チェック 5 件~~ DONE
### ~~3. almide-optimize: DCE/propagation が top_lets を処理しない~~ DONE
### ~~4. almide-optimize: mono の型上書き問題~~ DONE
### ~~5. almide-tools: ABI レイアウト不整合~~ DONE

## Should Fix — ALL DONE

### ~~6. almide-base: Sym の Ord~~ Not a violation (string comparison is intentional for deterministic field ordering)

## Accepted → DONE

### ~~7. almide-base: diagnostic に表示ロジック混在~~ DONE (rendering moved to `src/diagnostic_render.rs`)
### ~~8. almide-frontend: checker と lowering の pipe desugar 重複~~ DONE (`ImportTable::resolve_dotted_path` unified)
### ~~9. almide-frontend: lowering 内の型推論フォールバック~~ DONE (checker fix: eager `resolve_ty` in `infer_pipe_direct`, 75-line workaround deleted)
### ~~10. almide-tools: ModuleInterface.version が常に None~~ DONE (`extract_with_version` from almide.toml)

## Additional Bugs Found & Fixed

### ~~mono substitute_expr_types missing 12 variants~~ DONE
`UnwrapOr`, `Unwrap`, `ToOption`, `Clone`, `Deref`, `Borrow`, `BoxNew`, `RcWrap`, `ToVec`, `OptionalChain`, `Fan`, `RustMacro` が mono の型置換で走査されていなかった。generic `list.get[T] ?? default` が WASM で壊れる原因。
