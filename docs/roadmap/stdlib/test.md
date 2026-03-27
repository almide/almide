<!-- description: Test utilities (assert_throws, assert_contains, approx equality) -->
# stdlib: test [Tier 3]

テストユーティリティ。現在 `assert`, `assert_eq`, `assert_ne` のみ。

## 他言語比較

| 機能 | Go (`testing`) | Python (`pytest`) | Rust (`#[test]` + assert) | Deno (`@std/assert`) |
|------|---------------|--------------------|----|-----|
| assert | `t.Error()` | `assert` | `assert!()` | `assert()` |
| assert_eq | ❌ (manual) | `assert x == y` | `assert_eq!(a, b)` | `assertEquals(a, b)` |
| assert_ne | ❌ (manual) | `assert x != y` | `assert_ne!(a, b)` | `assertNotEquals(a, b)` |
| assert_throws | ❌ (manual) | `pytest.raises(Ex)` | `#[should_panic]` | `assertThrows(fn)` |
| assert_contains | ❌ (manual) | `assert x in y` | manual | `assertStringIncludes(s, sub)` |
| approx eq | ❌ | `pytest.approx(v)` | `approx` crate | `assertAlmostEquals(a, b, tol)` |
| snapshot | ❌ (external) | `syrupy` | `insta` crate | `assertSnapshot(ctx, val)` |
| bench | `b.Run("name", fn)` | `pytest-benchmark` | `#[bench]` (nightly) | `Deno.bench("name", fn)` |
| mock | ❌ (interfaces) | `unittest.mock` | `mockall` crate | external |

## 追加候補 (~10 関数)

### P0
- `test.assert_throws(fn) -> Result[String, String]` — 例外が投げられることを検証
- `test.assert_contains(list, element)` — リストに要素が含まれることを検証
- `test.assert_string_contains(s, sub)` — 文字列に部分文字列が含まれることを検証

### P1
- `test.assert_approx(a, b, tolerance)` — 浮動小数点近似比較
- `test.assert_gt(a, b)` — a > b
- `test.assert_lt(a, b)` — a < b
- `test.assert_some(option)` — Option が Some であることを検証
- `test.assert_ok(result)` — Result が Ok であることを検証

### P2
- `test.skip(reason)` — テストスキップ
- `test.todo(description)` — 未実装テストのマーカー

## 実装戦略

TOML + runtime。assert 系は現在 `println` に直接出力しているが、テストランナーとの統合が必要。
