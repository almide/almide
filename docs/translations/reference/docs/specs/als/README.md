<a id="als--section-index"></a>

# ALS — Section Index

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> Non-normative reader snapshot of the [mixed-language original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/README.md), commit `35297a612fb29e3f78f6587aba253aa91d51f466`. Not automatically synchronized, not generator input, and not a replacement for the canonical ALS. Requirements, evidence and historical claims have not been independently executed, measured or certified here.

> Source-generation note: the original index was generated from the chapter files and [contract ledger](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml). Its update instruction is `bash docs/specs/als/generate-readme.sh > docs/specs/als/README.md`. This is a historical instruction for the canonical index, not a command to regenerate this reader copy. Navigation below uses actual headings of pinned canonical chapters where the source index’s punctuation differs from GitHub fragments. Section IDs, contract references, ordering and counts are preserved.

<a id="boundedmd--12-sections"></a>

## bounded.md — 12 section(s)

| ID | Section | Contracts |
|----|---------|-----------|
| [ALS-B1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/bounded.md#als-b1-bounded-属性と有界プロファイル) | The `@bounded` attribute and bounded profile | C-308 |
| [ALS-B2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/bounded.md#als-b2-サブセットであって方言ではない) | A subset, not a dialect | C-309 |
| [ALS-B3](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/bounded.md#als-b3-回数付きループのみ) | Counted loops only | C-310 |
| [ALS-B4](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/bounded.md#als-b4-ループ内確保の禁止) | No allocation inside loops | C-311 |
| [ALS-B5](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/bounded.md#als-b5-break--continue-の禁止) | No `break` / `continue` | C-312 |
| [ALS-B6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/bounded.md#als-b6-再帰の禁止--呼び出しグラフの非循環) | No recursion—an acyclic call graph | C-313 |
| [ALS-B7](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/bounded.md#als-b7-呼び出し閉包--呼べるもの) | Call closure—what may be called | C-314 |
| [ALS-B8](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/bounded.md#als-b8-実行時長のヒープ構築の禁止) | No heap construction with runtime lengths | C-315 |
| [ALS-B9](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/bounded.md#als-b9-効果と-capability) | Effects and capabilities | C-316 |
| [ALS-B10](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/bounded.md#als-b10-浮動小数演算の禁止暫定) | No floating-point operations (provisional) | C-317 |
| [ALS-B11](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/bounded.md#als-b11-早期脱出の制限) | Restrictions on early exit | C-318 |
| [ALS-B12](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/bounded.md#als-b12-md5sha-1-は-stdlib-にない) | MD5 and SHA-1 are not in stdlib | C-424 |

<a id="collectionsmd--10-sections"></a>

## collections.md — 10 section(s)

| ID | Section | Contracts |
|----|---------|-----------|
| [ALS-C1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/collections.md#als-c1-map-の順序規範) | Map ordering requirements | C-013, C-380 |
| [ALS-C2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/collections.md#als-c2-set-の順序規範) | Set ordering requirements | C-014 |
| [ALS-C3](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/collections.md#als-c3-構造的等価) | Structural equality | C-015, C-124, C-185, C-426 |
| [ALS-C4](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/collections.md#als-c4-範囲外アクセスの縮退規則) | Out-of-range fallback rules | C-034, C-354 |
| [ALS-C5](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/collections.md#als-c5-値意味論copy-on-write) | Value semantics (copy-on-write) | C-033, C-125, C-131, C-150, C-213, C-337, C-340, C-342, C-343, C-344, C-345, C-346 |
| [ALS-C6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/collections.md#als-c6-型変換コンビネータ) | Type-changing combinators | C-039 |
| [ALS-C7](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/collections.md#als-c7-レコード変種パターンマッチ) | Records, variants, and pattern matching | C-036, C-224, C-226 |
| [ALS-C8](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/collections.md#als-c8-幅指定整数フィールド) | Sized-integer fields | C-038 |
| [ALS-C9](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/collections.md#als-c9-順序系コンビネータの全域性) | Totality of ordering combinators | C-053, C-055, C-276 |
| [ALS-C10](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/collections.md#als-c10-空コレクションの型要件静的規範) | Type requirements for empty collections (static requirement) | C-052, C-058, C-277 |

<a id="data-formatsmd--7-sections"></a>

## data-formats.md — 7 section(s)

| ID | Section | Contracts |
|----|---------|-----------|
| [ALS-D1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/data-formats.md#als-d1-json-パス操作) | JSON path operations | C-031, C-433 |
| [ALS-D2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/data-formats.md#als-d2-value-の-json-テキスト表現) | JSON text representation of Value | C-060, C-356 |
| [ALS-D3](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/data-formats.md#als-d3-異種ネスト文書の走査) | Traversing heterogeneous nested documents | C-063 |
| [ALS-D4](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/data-formats.md#als-d4-正規表現エンジン) | Regular-expression engine | C-032, C-160, C-285, C-347, C-395 |
| [ALS-D5](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/data-formats.md#als-d5-半精度浮動小数のデコード) | Decoding half-precision floating point | C-037 |
| [ALS-D6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/data-formats.md#als-d6-codec-と-json-デコード) | Codec and JSON decoding | C-084, C-085, C-095, C-098, C-103, C-209, C-211, C-216, C-217, C-357, C-427, C-428, C-429, C-435 |
| [ALS-D7](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/data-formats.md#als-d7-バイト列ブリッジ) | Byte-sequence bridge | C-062, C-090, C-376, C-446 |

<a id="deterministic-timemd--5-sections"></a>

## deterministic-time.md — 5 section(s)

| ID | Section | Contracts |
|----|---------|-----------|
| [ALS-DT1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/deterministic-time.md#als-dt1-時間構築子と代数) | Time constructors and algebra | C-202, C-203 |
| [ALS-DT2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/deterministic-time.md#als-dt2-決定的予算fanbounded) | Deterministic budgets (fan.bounded) | C-204, C-207, C-320 |
| [ALS-DT3](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/deterministic-time.md#als-dt3-決定的-racefanmin) | Deterministic race (fan.min) | C-205 |
| [ALS-DT4](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/deterministic-time.md#als-dt4-settle-の-tuple-契約) | Tuple contract for settle | C-206 |
| [ALS-DT5](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/deterministic-time.md#als-dt5-壁時計期限fantimeoutoracle-層) | Wall-clock deadlines (fan.timeout, oracle layer) | C-208 |

<a id="expressionsmd--39-sections"></a>

## expressions.md — 39 section(s)

| ID | Section | Contracts |
|----|---------|-----------|
| [ALS-E1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e1-整数リテラルexprkindint) | Integer literal (`ExprKind::Int`) | C-231 |
| [ALS-E2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e2-真偽リテラルexprkindbool) | Boolean literal (`ExprKind::Bool`) | C-232 |
| [ALS-E4](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e4-ユニットリテラルexprkindunit) | Unit literal (`ExprKind::Unit`) | C-233 |
| [ALS-E6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e6-括弧式exprkindparen) | Parenthesized expression (`ExprKind::Paren`) | C-234 |
| [ALS-E7](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e7-単項演算子exprkindunary) | Unary operator (`ExprKind::Unary`) | C-235 |
| [ALS-E8](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e8-タプルexprkindtuple--exprkindtupleindex) | Tuple (`ExprKind::Tuple` / `ExprKind::TupleIndex`) | C-236 |
| [ALS-E9](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e9-optionresult-コンストラクタexprkindsome--exprkindnone--exprkindok--exprkinderr) | Option/Result constructors (`ExprKind::Some` / `ExprKind::None` / `ExprKind::Ok` / `ExprKind::Err`) | C-237, C-286 |
| [ALS-E10](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e10-レンジ式exprkindrange) | Range expression (`ExprKind::Range`) | C-238 |
| [ALS-E11](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e11-リストリテラルと索引exprkindlist--exprkindindexaccess) | List literal and indexing (`ExprKind::List` / `ExprKind::IndexAccess`) | C-239 |
| [ALS-E12](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e12-マップリテラルexprkindmapliteral--exprkindemptymap) | Map literal (`ExprKind::MapLiteral` / `ExprKind::EmptyMap`) | C-240 |
| [ALS-ST1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-st1-束縛文stmtlet--stmtvar--stmtassign) | Binding statements (`Stmt::Let` / `Stmt::Var` / `Stmt::Assign`) | C-241 |
| [ALS-E13](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e13-条件式exprkindif) | Conditional expression (`ExprKind::If`) | C-242 |
| [ALS-E14](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e14-ブロック式exprkindblock) | Block expression (`ExprKind::Block`) | C-243 |
| [ALS-E15](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e15-while-文exprkindwhile) | while statement (`ExprKind::While`) | C-244 |
| [ALS-E16](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e16-文字列補間exprkindinterpolatedstring) | String interpolation (`ExprKind::InterpolatedString`) | C-245, C-425, C-444 |
| [ALS-E17](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e17-識別子exprkindident) | Identifier (`ExprKind::Ident`) | C-246, C-387 |
| [ALS-E18](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e18-match-式exprkindmatch) | match expression (`ExprKind::Match`) | C-247, C-281, C-323, C-332, C-333, C-352 |
| [ALS-E19](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e19-for-in-文exprkindforin) | for-in statement (`ExprKind::ForIn`) | C-248, C-279 |
| [ALS-ST2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-st2-分解束縛stmtletdestructure) | Destructuring binding (`Stmt::LetDestructure`) | C-249, C-373 |
| [ALS-E20](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e20-パイプと合成exprkindpipe--exprkindcompose) | Pipe and composition (`ExprKind::Pipe` / `ExprKind::Compose`) | C-250 |
| [ALS-E21](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e21-if-letexprkindiflet) | if let (`ExprKind::IfLet`) | C-251 |
| [ALS-ST3](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-st3-式文とコメントstmtexpr--stmtcomment) | Expression statements and comments (`Stmt::Expr` / `Stmt::Comment`) | C-252, C-280 |
| [ALS-ST4](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-st4-場所代入stmtindexassign--stmtfieldassign) | Place assignment (`Stmt::IndexAssign` / `Stmt::FieldAssign`) | C-253 |
| [ALS-E22](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e22-型注釈式exprkindtypeascription) | Type-ascription expression (`ExprKind::TypeAscription`) | C-254 |
| [ALS-E23](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e23-レコードexprkindrecord--exprkindspreadrecord--exprkindmember) | Records (`ExprKind::Record` / `ExprKind::SpreadRecord` / `ExprKind::Member`) | C-255 |
| [ALS-E24](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e24-break-と-continueexprkindbreak--exprkindcontinue) | break and continue (`ExprKind::Break` / `ExprKind::Continue`) | C-256 |
| [ALS-E25](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e25-エラー演算子exprkindunwrap--exprkindtooption--exprkindunwrapor--exprkindtry) | Error operators (`ExprKind::Unwrap` / `ExprKind::ToOption` / `ExprKind::UnwrapOr` / `ExprKind::Try`) | C-257, C-271 |
| [ALS-E26](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e26-呼び出しとラムダexprkindcall--exprkindlambda) | Calls and lambdas (`ExprKind::Call` / `ExprKind::Lambda`) | C-258, C-388 |
| [ALS-E27](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e27-コンストラクタ参照exprkindtypename) | Constructor reference (`ExprKind::TypeName`) | C-259, C-322 |
| [ALS-DL1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-dl1-宣言declmodule--declimport--decltype--declfn--decltoplet--declprotocol--decltest--decltestwheredef) | Declarations (`Decl::Module` / `Decl::Import` / `Decl::Type` / `Decl::Fn` / `Decl::TopLet` / `Decl::Protocol` / `Decl::Test` / `Decl::TestWhereDef`) | C-260, C-430 |
| [ALS-E3](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e3-浮動小数点リテラルexprkindfloat-部分節) | Floating-point literal (`ExprKind::Float`)—partial section | C-182, C-261 |
| [ALS-E5](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e5-文字列リテラルexprkindstring-部分節) | String literal (`ExprKind::String`)—partial section | C-262 |
| [ALS-DL2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-dl2-回復ノードexprkinderror--stmterror) | Recovery nodes (`ExprKind::Error` / `Stmt::Error`) | C-263 |
| [ALS-E28](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e28-オプショナルチェーンexprkindoptionalchain) | Optional chaining (`ExprKind::OptionalChain`) | C-264 |
| [ALS-ST5](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-st5-guard-文stmtguard) | guard statement (`Stmt::Guard`) | C-265 |
| [ALS-ST6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-st6-guard-let-文stmtguardlet) | guard let statement (`Stmt::GuardLet`) | C-266 |
| [ALS-E29](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e29-二項演算子exprkindbinary) | Binary operators (`ExprKind::Binary`) | C-267 |
| [ALS-E30](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e30-ホールと未実装マーカーexprkindhole--todo--placeholder) | Holes and unimplemented markers (`ExprKind::Hole` / `Todo` / `Placeholder`) | C-268 |
| [ALS-E31](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md#als-e31-スコープ付き領域exprkindscoped--scoped-fn) | Scoped regions (`ExprKind::Scoped` / `scoped fn`) | C-362, C-363, C-364 |

<a id="implementationmd--4-sections"></a>

## implementation.md — 4 section(s)

| ID | Section | Contracts |
|----|---------|-----------|
| [ALS-I1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/implementation.md#als-i1-参照カウント規律perceus) | Reference-counting discipline (Perceus) | C-041, C-066, C-071, C-086, C-121, C-122, C-130, C-149, C-146, C-159, C-319, C-334 |
| [ALS-I2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/implementation.md#als-i2-コンパイラの決定性と資源) | Compiler determinism and resources | C-040, C-059 |
| [ALS-I3](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/implementation.md#als-i3-v1-lowering-エッジの等価証拠) | Equivalence evidence for v1 lowering edges | C-075, C-107, C-109, C-116, C-120, C-138, C-152, C-143, C-144, C-139, C-156, C-157, C-158, C-212, C-336 |
| [ALS-I4](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/implementation.md#als-i4-コンパイルコストの予算) | Compilation-cost budgets | C-440 |

<a id="runtimemd--9-sections"></a>

## runtime.md — 9 section(s)

| ID | Section | Contracts |
|----|---------|-----------|
| [ALS-R1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/runtime.md#als-r1-effect-main-のエラー終了形) | Error termination of effect-main | C-035 |
| [ALS-R2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/runtime.md#als-r2-補間の表示形) | Interpolation display forms | C-008, C-009, C-010, C-011, C-222 |
| [ALS-R3](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/runtime.md#als-r3-fan-並行コンビネータの決定性) | Determinism of fan concurrent combinators | C-004, C-005, C-006, C-199, C-321, C-349, C-386 |
| [ALS-R4](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/runtime.md#als-r4-非有限浮動小数の定数表示) | Constant display of nonfinite floating-point values | C-012 |
| [ALS-R5](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/runtime.md#als-r5-プロセス環境) | Process environment | C-096, C-112, C-118, C-133, C-189, C-214, C-215, C-290, C-327, C-330, C-328, C-329, C-331, C-366, C-367, C-370, C-374, C-375, C-377, C-378, C-379, C-382, C-383, C-385, C-389, C-390, C-394, C-404, C-405, C-407, C-408, C-409, C-415, C-416, C-417, C-418, C-420, C-421, C-431, C-434, C-437, C-438, C-439, C-441, C-442, C-443, C-445, C-447, C-448 |
| [ALS-R6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/runtime.md#als-r6-ファイルシステムのパス解決) | Filesystem path resolution | C-042, C-137, C-220, C-225, C-227, C-228, C-229, C-230, C-270, C-272, C-273, C-278, C-282, C-283, C-284, C-392, C-393, C-396, C-397, C-398 |
| [ALS-R7](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/runtime.md#als-r7-ストリーミング行走査の可謬コールバック) | Fallible callbacks in streaming line traversal | C-274, C-335 |
| [ALS-R8](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/runtime.md#als-r8-http-レスポンスヘッダの規範) | HTTP response-header requirements | C-275, C-368, C-406, C-432, C-436 |
| [ALS-R9](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/runtime.md#als-r9-プロセス終了コードの値域) | Process exit-code range | C-350, C-351 |

<a id="semanticsmd--15-sections"></a>

## semantics.md — 15 section(s)

| ID | Section | Contracts |
|----|---------|-----------|
| [ALS-M1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/semantics.md#als-m1-パターンマッチの束縛規範) | Binding norms for pattern matching | C-044, C-070, C-073, C-091, C-113, C-114, C-201, C-269 |
| [ALS-M2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/semantics.md#als-m2-レコード意味論) | Record semantics | C-046, C-072, C-078, C-092, C-123 |
| [ALS-M3](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/semantics.md#als-m3-変種型adt) | Variant types (ADTs) | C-043, C-076, C-079, C-093 |
| [ALS-M4](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/semantics.md#als-m4-effect-fn-の脱糖規範) | Desugaring norms for effect fn | C-064, C-068, C-069, C-119, C-135, C-183, C-186, C-187, C-188, C-190, C-191, C-192, C-193, C-194, C-195, C-292, C-293, C-295, C-296, C-297 |
| [ALS-M5](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/semantics.md#als-m5-ジェネリクスと推論) | Generics and inference | C-080, C-081, C-082, C-089, C-094, C-097, C-126, C-127, C-151, C-145, C-142, C-176, C-178, C-365 |
| [ALS-M6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/semantics.md#als-m6-蓄積ループの規範) | Norms for accumulator loops | C-102, C-104, C-105, C-117, C-174, C-177 |
| [ALS-M7](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/semantics.md#als-m7-識別子の独立性) | Identifier independence | C-088, C-175 |
| [ALS-M8](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/semantics.md#als-m8-演算子の全域規範) | Operator norms across all operand forms | C-083, C-099, C-167, C-170, C-181 |
| [ALS-M9](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/semantics.md#als-m9-静的検査unit-変異子) | Static checking (Unit mutators) | C-057 |
| [ALS-M10](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/semantics.md#als-m10-分岐からの-heap-束縛) | Heap bindings from branches | C-106, C-115, C-163, C-165, C-166, C-287, C-288, C-289, C-291 |
| [ALS-M11](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/semantics.md#als-m11-unwrap-の脱糖) | Desugaring unwrap | C-108, C-369 |
| [ALS-M12](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/semantics.md#als-m12-heap-要素リスト操作の一般性) | Generality of heap-element list operations | C-045, C-100, C-101, C-147, C-148, C-141, C-164, C-168, C-172, C-218 |
| [ALS-M13](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/semantics.md#als-m13-mut-パラメータの-in-place-変異) | In-place mutation of mut parameters | C-061, C-110, C-132, C-136, C-324, C-325, C-326 |
| [ALS-M14](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/semantics.md#als-m14-整数リテラルの型域静的規範) | Type ranges of integer literals (static norm) | C-173, C-179, C-180 |
| [ALS-M15](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/semantics.md#als-m15-effect-fn-型スロット) | effect fn type slots | C-221 |

<a id="stringsmd--6-sections"></a>

## strings.md — 6 section(s)

| ID | Section | Contracts |
|----|---------|-----------|
| [ALS-S1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/strings.md#als-s1-コードポイント意味論) | Code-point semantics | C-016, C-065 |
| [ALS-S2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/strings.md#als-s2-空パターンの検索規則) | Search rules for empty patterns | C-017 |
| [ALS-S3](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/strings.md#als-s3-文字種述語) | Character-class predicates | C-018, C-019 |
| [ALS-S4](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/strings.md#als-s4-バイト列との相互変換) | Conversion to and from byte sequences | C-022, C-348 |
| [ALS-S5](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/strings.md#als-s5-split-の区切り規範) | Delimiter rules for split | C-050 |
| [ALS-S6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/strings.md#als-s6-規模不変性) | Scale invariance | C-074 |

<a id="text-and-numbersmd--31-sections"></a>

## text-and-numbers.md — 31 section(s)

| ID | Section | Contracts |
|----|---------|-----------|
| [ALS-T1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t1-stringtrim) | `string.trim` | C-021, C-294 |
| [ALS-T2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t2-floatparse) | `float.parse` | C-024, C-210, C-300, C-301, C-413 |
| [ALS-T3](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t3-jsonparse) | `json.parse` | C-087, C-298, C-299, C-391, C-400, C-401 |
| [ALS-T4](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t4-listchunk--listwindow) | `list.chunk` / `list.window` | C-129, C-171 |
| [ALS-T5](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t5-stringto_upper--stringto_lower) | `string.to_upper` / `string.to_lower` | C-020, C-162, C-399, C-412 |
| [ALS-T6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t6-整数演算の終了規約termination-convention) | Termination convention for integer arithmetic | C-001, C-002, C-047, C-067, C-154, C-155, C-161, C-169, C-184, C-196, C-197, C-198, C-200, C-219, C-353, C-419 |
| [ALS-T7](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t7-トップレベル-let-の評価時機) | Evaluation timing of top-level let | C-007, C-077, C-111 |
| [ALS-T8](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t8-整数パースの文法とエラー規範) | Integer-parsing grammar and error norms | C-028, C-029 |
| [ALS-T9](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t9-固定小数表示) | Fixed-point formatting | C-025 |
| [ALS-T10](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t10-数学関数の決定性) | Determinism of mathematical functions | C-026, C-051, C-134 |
| [ALS-T11](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t11-バイナリテキスト符号化) | Binary-to-text encodings | C-027, C-030, C-411 |
| [ALS-T12](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t12-非-abort-整数除算の一致) | Agreement of non-aborting integer division | C-003 |
| [ALS-T13](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t13-浮動小数の文字列化) | Floating-point string conversion | C-023 |
| [ALS-T14](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t14-64-bit-レジスタと-wrap-のマスク飽和) | 64-bit registers and saturating wrap masks | C-048 |
| [ALS-T15](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t15-符号と-minmax-の-nan-規則) | NaN rules for sign and min/max | C-049, C-140 |
| [ALS-T16](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t16-個数添字の-i64-クランプ) | i64 clamping of counts and indices | C-054, C-056 |
| [ALS-T17](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t17-datetimeformat-の指定子置換) | Specifier substitution in datetime.format | C-128 |
| [ALS-T18](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t18-assert-の-abort-形非-test-位置) | Abort form of assert (outside tests) | C-153, C-339, C-381 |
| [ALS-T19](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t19-数値決定性ファミリー) | Numeric-determinism family | C-302, C-358 |
| [ALS-T20](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t20-丸めと縮約の禁止) | Rounding and prohibition of contraction | C-303 |
| [ALS-T21](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t21-非正規数の保存) | Preservation of subnormals | C-304 |
| [ALS-T22](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t22-超越関数の誤差上限) | Error bounds for transcendental functions | C-305 |
| [ALS-T23](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t23-符号付きゼロ) | Signed zero | C-306, C-355 |
| [ALS-T24](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t24-float--int-変換) | Float → Int conversion | C-307, C-338, C-341, C-414 |
| [ALS-T25](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t25-matrix-の超越計算と積の順序) | Transcendental computation and product order in matrix | C-223 |
| [ALS-T26](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t26-datetime-の暦フィールドと-parse_iso-の文法) | Calendar fields and parse_iso grammar in datetime | C-359, C-360, C-410 |
| [ALS-T27](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t27-urlparse-の-authority-規範) | Authority norms for url.parse | C-361, C-402, C-403 |
| [ALS-T28](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t28-float32-演算の-binary32-丸め) | binary32 rounding of Float32 operations | C-371 |
| [ALS-T29](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t29-float32-の表示) | Float32 formatting | C-372 |
| [ALS-T30](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t30-testingassert_aborts--子で走る-abort-表明) | testing.assert_aborts—abort assertions running in a child | C-384 |
| [ALS-T31](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md#als-t31-hmac-と定数時間比較) | HMAC and constant-time comparison | C-422, C-423 |

138 sections across 11 chapters.
