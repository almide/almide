# ALS — Section Index

> Auto-generated from the chapter files and [the contract ledger](../../contracts/contracts.toml).
> Run `bash docs/specs/als/generate-readme.sh > docs/specs/als/README.md` to update.

## bounded.md — 11 section(s)

| ID | Section | Contracts |
|----|---------|-----------|
| [ALS-B1](./bounded.md#als-b1-bounded-属性と有界プロファイル) | `@bounded` 属性と有界プロファイル | C-308 |
| [ALS-B2](./bounded.md#als-b2-サブセットであって方言ではない) | サブセットであって方言ではない | C-309 |
| [ALS-B3](./bounded.md#als-b3-回数付きループのみ) | 回数付きループのみ | C-310 |
| [ALS-B4](./bounded.md#als-b4-ループ内確保の禁止) | ループ内確保の禁止 | C-311 |
| [ALS-B5](./bounded.md#als-b5-break--continue-の禁止) | `break` / `continue` の禁止 | C-312 |
| [ALS-B6](./bounded.md#als-b6-再帰の禁止-—-呼び出しグラフの非循環) | 再帰の禁止 — 呼び出しグラフの非循環 | C-313 |
| [ALS-B7](./bounded.md#als-b7-呼び出し閉包-—-呼べるもの) | 呼び出し閉包 — 呼べるもの | C-314 |
| [ALS-B8](./bounded.md#als-b8-実行時長のヒープ構築の禁止) | 実行時長のヒープ構築の禁止 | C-315 |
| [ALS-B9](./bounded.md#als-b9-効果と-capability) | 効果と capability | C-316 |
| [ALS-B10](./bounded.md#als-b10-浮動小数演算の禁止（暫定）) | 浮動小数演算の禁止（暫定） | C-317 |
| [ALS-B11](./bounded.md#als-b11-早期脱出の制限) | 早期脱出の制限 | C-318 |

## collections.md — 10 section(s)

| ID | Section | Contracts |
|----|---------|-----------|
| [ALS-C1](./collections.md#als-c1-map-の順序規範) | Map の順序規範 | C-013 |
| [ALS-C2](./collections.md#als-c2-set-の順序規範) | Set の順序規範 | C-014 |
| [ALS-C3](./collections.md#als-c3-構造的等価) | 構造的等価 | C-015, C-124, C-185 |
| [ALS-C4](./collections.md#als-c4-範囲外アクセスの縮退規則) | 範囲外アクセスの縮退規則 | C-034 |
| [ALS-C5](./collections.md#als-c5-値意味論（copy-on-write）) | 値意味論（copy-on-write） | C-033, C-125, C-131, C-150, C-213 |
| [ALS-C6](./collections.md#als-c6-型変換コンビネータ) | 型変換コンビネータ | C-039 |
| [ALS-C7](./collections.md#als-c7-レコード・変種・パターンマッチ) | レコード・変種・パターンマッチ | C-036, C-224, C-226 |
| [ALS-C8](./collections.md#als-c8-幅指定整数フィールド) | 幅指定整数フィールド | C-038 |
| [ALS-C9](./collections.md#als-c9-順序系コンビネータの全域性) | 順序系コンビネータの全域性 | C-053, C-055, C-276 |
| [ALS-C10](./collections.md#als-c10-空コレクションの型要件（静的規範）) | 空コレクションの型要件（静的規範） | C-052, C-058, C-277 |

## data-formats.md — 7 section(s)

| ID | Section | Contracts |
|----|---------|-----------|
| [ALS-D1](./data-formats.md#als-d1-json-パス操作) | JSON パス操作 | C-031 |
| [ALS-D2](./data-formats.md#als-d2-value-の-json-テキスト表現) | Value の JSON テキスト表現 | C-060 |
| [ALS-D3](./data-formats.md#als-d3-異種ネスト文書の走査) | 異種ネスト文書の走査 | C-063 |
| [ALS-D4](./data-formats.md#als-d4-正規表現エンジン) | 正規表現エンジン | C-032, C-160, C-285 |
| [ALS-D5](./data-formats.md#als-d5-半精度浮動小数のデコード) | 半精度浮動小数のデコード | C-037 |
| [ALS-D6](./data-formats.md#als-d6-codec-と-json-デコード) | Codec と JSON デコード | C-084, C-085, C-095, C-098, C-103, C-209, C-211, C-216, C-217 |
| [ALS-D7](./data-formats.md#als-d7-バイト列ブリッジ) | バイト列ブリッジ | C-062, C-090 |

## deterministic-time.md — 5 section(s)

| ID | Section | Contracts |
|----|---------|-----------|
| [ALS-DT1](./deterministic-time.md#als-dt1-時間構築子と代数) | 時間構築子と代数 | C-202, C-203 |
| [ALS-DT2](./deterministic-time.md#als-dt2-決定的予算（fanbounded）) | 決定的予算（fan.bounded） | C-204, C-207, C-320 |
| [ALS-DT3](./deterministic-time.md#als-dt3-決定的-race（fanrace）) | 決定的 race（fan.race） | C-205 |
| [ALS-DT4](./deterministic-time.md#als-dt4-settle-の-tuple-契約) | settle の tuple 契約 | C-206 |
| [ALS-DT5](./deterministic-time.md#als-dt5-壁時計期限（fantimeout、oracle-層）) | 壁時計期限（fan.timeout、oracle 層） | C-208 |

## expressions.md — 38 section(s)

| ID | Section | Contracts |
|----|---------|-----------|
| [ALS-E1](./expressions.md#als-e1-整数リテラルexprkindint) | 整数リテラル(`ExprKind::Int`) | C-231 |
| [ALS-E2](./expressions.md#als-e2-真偽リテラルexprkindbool) | 真偽リテラル(`ExprKind::Bool`) | C-232 |
| [ALS-E4](./expressions.md#als-e4-ユニットリテラルexprkindunit) | ユニットリテラル(`ExprKind::Unit`) | C-233 |
| [ALS-E6](./expressions.md#als-e6-括弧式exprkindparen) | 括弧式(`ExprKind::Paren`) | C-234 |
| [ALS-E7](./expressions.md#als-e7-単項演算子exprkindunary) | 単項演算子(`ExprKind::Unary`) | C-235 |
| [ALS-E8](./expressions.md#als-e8-タプルexprkindtuple--exprkindtupleindex) | タプル(`ExprKind::Tuple` / `ExprKind::TupleIndex`) | C-236 |
| [ALS-E9](./expressions.md#als-e9-optionresult-コンストラクタexprkindsome--exprkindnone--exprkindok--exprkinderr) | Option/Result コンストラクタ(`ExprKind::Some` / `ExprKind::None` / `ExprKind::Ok` / `ExprKind::Err`) | C-237, C-286 |
| [ALS-E10](./expressions.md#als-e10-レンジ式exprkindrange) | レンジ式(`ExprKind::Range`) | C-238 |
| [ALS-E11](./expressions.md#als-e11-リストリテラルと索引exprkindlist--exprkindindexaccess) | リストリテラルと索引(`ExprKind::List` / `ExprKind::IndexAccess`) | C-239 |
| [ALS-E12](./expressions.md#als-e12-マップリテラルexprkindmapliteral--exprkindemptymap) | マップリテラル(`ExprKind::MapLiteral` / `ExprKind::EmptyMap`) | C-240 |
| [ALS-ST1](./expressions.md#als-st1-束縛文stmtlet--stmtvar--stmtassign) | 束縛文(`Stmt::Let` / `Stmt::Var` / `Stmt::Assign`) | C-241 |
| [ALS-E13](./expressions.md#als-e13-条件式exprkindif) | 条件式(`ExprKind::If`) | C-242 |
| [ALS-E14](./expressions.md#als-e14-ブロック式exprkindblock) | ブロック式(`ExprKind::Block`) | C-243 |
| [ALS-E15](./expressions.md#als-e15-while-文exprkindwhile) | while 文(`ExprKind::While`) | C-244 |
| [ALS-E16](./expressions.md#als-e16-文字列補間exprkindinterpolatedstring) | 文字列補間(`ExprKind::InterpolatedString`) | C-245 |
| [ALS-E17](./expressions.md#als-e17-識別子exprkindident) | 識別子(`ExprKind::Ident`) | C-246 |
| [ALS-E18](./expressions.md#als-e18-match-式exprkindmatch) | match 式(`ExprKind::Match`) | C-247, C-281 |
| [ALS-E19](./expressions.md#als-e19-for-in-文exprkindforin) | for-in 文(`ExprKind::ForIn`) | C-248, C-279 |
| [ALS-ST2](./expressions.md#als-st2-分解束縛stmtletdestructure) | 分解束縛(`Stmt::LetDestructure`) | C-249 |
| [ALS-E20](./expressions.md#als-e20-パイプと合成exprkindpipe--exprkindcompose) | パイプと合成(`ExprKind::Pipe` / `ExprKind::Compose`) | C-250 |
| [ALS-E21](./expressions.md#als-e21-if-letexprkindiflet) | if let(`ExprKind::IfLet`) | C-251 |
| [ALS-ST3](./expressions.md#als-st3-式文とコメントstmtexpr--stmtcomment) | 式文とコメント(`Stmt::Expr` / `Stmt::Comment`) | C-252, C-280 |
| [ALS-ST4](./expressions.md#als-st4-場所代入stmtindexassign--stmtfieldassign) | 場所代入(`Stmt::IndexAssign` / `Stmt::FieldAssign`) | C-253 |
| [ALS-E22](./expressions.md#als-e22-型注釈式exprkindtypeascription) | 型注釈式(`ExprKind::TypeAscription`) | C-254 |
| [ALS-E23](./expressions.md#als-e23-レコードexprkindrecord--exprkindspreadrecord--exprkindmember) | レコード(`ExprKind::Record` / `ExprKind::SpreadRecord` / `ExprKind::Member`) | C-255 |
| [ALS-E24](./expressions.md#als-e24-break-と-continueexprkindbreak--exprkindcontinue) | break と continue(`ExprKind::Break` / `ExprKind::Continue`) | C-256 |
| [ALS-E25](./expressions.md#als-e25-エラー演算子exprkindunwrap--exprkindtooption--exprkindunwrapor--exprkindtry) | エラー演算子(`ExprKind::Unwrap` / `ExprKind::ToOption` / `ExprKind::UnwrapOr` / `ExprKind::Try`) | C-257, C-271 |
| [ALS-E26](./expressions.md#als-e26-呼び出しとラムダexprkindcall--exprkindlambda) | 呼び出しとラムダ(`ExprKind::Call` / `ExprKind::Lambda`) | C-258 |
| [ALS-E27](./expressions.md#als-e27-コンストラクタ参照exprkindtypename) | コンストラクタ参照(`ExprKind::TypeName`) | C-259 |
| [ALS-DL1](./expressions.md#als-dl1-宣言declmodule--declimport--decltype--declfn--decltoplet--declprotocol--decltest--decltestwheredef) | 宣言(`Decl::Module` / `Decl::Import` / `Decl::Type` / `Decl::Fn` / `Decl::TopLet` / `Decl::Protocol` / `Decl::Test` / `Decl::TestWhereDef`) | C-260 |
| [ALS-E3](./expressions.md#als-e3-浮動小数点リテラルexprkindfloat—-部分節) | 浮動小数点リテラル(`ExprKind::Float`)— 部分節 | C-182, C-261 |
| [ALS-E5](./expressions.md#als-e5-文字列リテラルexprkindstring—-部分節) | 文字列リテラル(`ExprKind::String`)— 部分節 | C-262 |
| [ALS-DL2](./expressions.md#als-dl2-回復ノードexprkinderror--stmterror) | 回復ノード(`ExprKind::Error` / `Stmt::Error`) | C-263 |
| [ALS-E28](./expressions.md#als-e28-オプショナルチェーンexprkindoptionalchain) | オプショナルチェーン(`ExprKind::OptionalChain`) | C-264 |
| [ALS-ST5](./expressions.md#als-st5-guard-文stmtguard) | guard 文(`Stmt::Guard`) | C-265 |
| [ALS-ST6](./expressions.md#als-st6-guard-let-文stmtguardlet) | guard let 文(`Stmt::GuardLet`) | C-266 |
| [ALS-E29](./expressions.md#als-e29-二項演算子exprkindbinary) | 二項演算子(`ExprKind::Binary`) | C-267 |
| [ALS-E30](./expressions.md#als-e30-ホールと未実装マーカーexprkindhole--todo--placeholder) | ホールと未実装マーカー(`ExprKind::Hole` / `Todo` / `Placeholder`) | C-268 |

## implementation.md — 3 section(s)

| ID | Section | Contracts |
|----|---------|-----------|
| [ALS-I1](./implementation.md#als-i1-参照カウント規律（perceus）) | 参照カウント規律（Perceus） | C-041, C-066, C-071, C-086, C-121, C-122, C-130, C-149, C-146, C-159, C-319 |
| [ALS-I2](./implementation.md#als-i2-コンパイラの決定性と資源) | コンパイラの決定性と資源 | C-040, C-059 |
| [ALS-I3](./implementation.md#als-i3-v1-lowering-エッジの等価証拠) | v1 lowering エッジの等価証拠 | C-075, C-107, C-109, C-116, C-120, C-138, C-152, C-143, C-144, C-139, C-156, C-157, C-158, C-212, C-321 |

## runtime.md — 8 section(s)

| ID | Section | Contracts |
|----|---------|-----------|
| [ALS-R1](./runtime.md#als-r1-effect-main-のエラー終了形) | effect-main のエラー終了形 | C-035 |
| [ALS-R2](./runtime.md#als-r2-補間の表示形) | 補間の表示形 | C-008, C-009, C-010, C-011, C-222 |
| [ALS-R3](./runtime.md#als-r3-fan-並行コンビネータの決定性) | fan 並行コンビネータの決定性 | C-004, C-005, C-006, C-199 |
| [ALS-R4](./runtime.md#als-r4-非有限浮動小数の定数表示) | 非有限浮動小数の定数表示 | C-012 |
| [ALS-R5](./runtime.md#als-r5-プロセス環境) | プロセス環境 | C-096, C-112, C-118, C-133, C-189, C-214, C-215, C-290 |
| [ALS-R6](./runtime.md#als-r6-ファイルシステムのパス解決) | ファイルシステムのパス解決 | C-042, C-137, C-220, C-225, C-227, C-228, C-229, C-230, C-270, C-272, C-273, C-278, C-282, C-283, C-284 |
| [ALS-R7](./runtime.md#als-r7-ストリーミング行走査の可謬コールバック) | ストリーミング行走査の可謬コールバック | C-274 |
| [ALS-R8](./runtime.md#als-r8-http-レスポンスヘッダの規範) | HTTP レスポンスヘッダの規範 | C-275 |

## semantics.md — 15 section(s)

| ID | Section | Contracts |
|----|---------|-----------|
| [ALS-M1](./semantics.md#als-m1-パターンマッチの束縛規範) | パターンマッチの束縛規範 | C-044, C-070, C-073, C-091, C-113, C-114, C-201, C-269 |
| [ALS-M2](./semantics.md#als-m2-レコード意味論) | レコード意味論 | C-046, C-072, C-078, C-092, C-123 |
| [ALS-M3](./semantics.md#als-m3-変種型（adt）) | 変種型（ADT） | C-043, C-076, C-079, C-093 |
| [ALS-M4](./semantics.md#als-m4-effect-fn-の脱糖規範) | effect fn の脱糖規範 | C-064, C-068, C-069, C-119, C-135, C-183, C-186, C-187, C-188, C-190, C-191, C-192, C-193, C-194, C-195, C-292, C-293, C-295, C-296, C-297 |
| [ALS-M5](./semantics.md#als-m5-ジェネリクスと推論) | ジェネリクスと推論 | C-080, C-081, C-082, C-089, C-094, C-097, C-126, C-127, C-151, C-145, C-142, C-176, C-178 |
| [ALS-M6](./semantics.md#als-m6-蓄積ループの規範) | 蓄積ループの規範 | C-102, C-104, C-105, C-117, C-174, C-177 |
| [ALS-M7](./semantics.md#als-m7-識別子の独立性) | 識別子の独立性 | C-088, C-175 |
| [ALS-M8](./semantics.md#als-m8-演算子の全域規範) | 演算子の全域規範 | C-083, C-099, C-167, C-170, C-181 |
| [ALS-M9](./semantics.md#als-m9-静的検査（unit-変異子）) | 静的検査（Unit 変異子） | C-057 |
| [ALS-M10](./semantics.md#als-m10-分岐からの-heap-束縛) | 分岐からの heap 束縛 | C-106, C-115, C-163, C-165, C-166, C-287, C-288, C-289, C-291 |
| [ALS-M11](./semantics.md#als-m11-unwrap-の脱糖) | unwrap の脱糖 | C-108 |
| [ALS-M12](./semantics.md#als-m12-heap-要素リスト操作の一般性) | heap 要素リスト操作の一般性 | C-045, C-100, C-101, C-147, C-148, C-141, C-164, C-168, C-172, C-218 |
| [ALS-M13](./semantics.md#als-m13-mut-パラメータの-in-place-変異) | mut パラメータの in-place 変異 | C-061, C-110, C-132, C-136 |
| [ALS-M14](./semantics.md#als-m14-整数リテラルの型域（静的規範）) | 整数リテラルの型域（静的規範） | C-173, C-179, C-180 |
| [ALS-M15](./semantics.md#als-m15-effect-fn-型スロット) | effect fn 型スロット | C-221 |

## strings.md — 6 section(s)

| ID | Section | Contracts |
|----|---------|-----------|
| [ALS-S1](./strings.md#als-s1-コードポイント意味論) | コードポイント意味論 | C-016, C-065 |
| [ALS-S2](./strings.md#als-s2-空パターンの検索規則) | 空パターンの検索規則 | C-017 |
| [ALS-S3](./strings.md#als-s3-文字種述語) | 文字種述語 | C-018, C-019 |
| [ALS-S4](./strings.md#als-s4-バイト列との相互変換) | バイト列との相互変換 | C-022 |
| [ALS-S5](./strings.md#als-s5-split-の区切り規範) | split の区切り規範 | C-050 |
| [ALS-S6](./strings.md#als-s6-規模不変性) | 規模不変性 | C-074 |

## text-and-numbers.md — 25 section(s)

| ID | Section | Contracts |
|----|---------|-----------|
| [ALS-T1](./text-and-numbers.md#als-t1-stringtrim) | `string.trim` | C-021, C-294 |
| [ALS-T2](./text-and-numbers.md#als-t2-floatparse) | `float.parse` | C-024, C-210, C-300, C-301 |
| [ALS-T3](./text-and-numbers.md#als-t3-jsonparse) | `json.parse` | C-087, C-298, C-299 |
| [ALS-T4](./text-and-numbers.md#als-t4-listchunk--listwindows) | `list.chunk` / `list.windows` | C-129, C-171 |
| [ALS-T5](./text-and-numbers.md#als-t5-stringto_upper--stringto_lower) | `string.to_upper` / `string.to_lower` | C-020, C-162 |
| [ALS-T6](./text-and-numbers.md#als-t6-整数演算の終了規約（termination-convention）) | 整数演算の終了規約（termination convention） | C-001, C-002, C-047, C-067, C-154, C-155, C-161, C-169, C-184, C-196, C-197, C-198, C-200, C-219 |
| [ALS-T7](./text-and-numbers.md#als-t7-トップレベル-let-の評価時機) | トップレベル let の評価時機 | C-007, C-077, C-111 |
| [ALS-T8](./text-and-numbers.md#als-t8-整数パースの文法とエラー規範) | 整数パースの文法とエラー規範 | C-028, C-029 |
| [ALS-T9](./text-and-numbers.md#als-t9-固定小数表示) | 固定小数表示 | C-025 |
| [ALS-T10](./text-and-numbers.md#als-t10-数学関数の決定性) | 数学関数の決定性 | C-026, C-051, C-134 |
| [ALS-T11](./text-and-numbers.md#als-t11-バイナリテキスト符号化) | バイナリテキスト符号化 | C-027, C-030 |
| [ALS-T12](./text-and-numbers.md#als-t12-非-abort-整数除算の一致) | 非 abort 整数除算の一致 | C-003 |
| [ALS-T13](./text-and-numbers.md#als-t13-浮動小数の文字列化) | 浮動小数の文字列化 | C-023 |
| [ALS-T14](./text-and-numbers.md#als-t14-wrap--rotate-のマスク飽和) | wrap / rotate のマスク飽和 | C-048 |
| [ALS-T15](./text-and-numbers.md#als-t15-符号と-minmax-の-nan-規則) | 符号と min/max の NaN 規則 | C-049, C-140 |
| [ALS-T16](./text-and-numbers.md#als-t16-個数・添字の-i64-クランプ) | 個数・添字の i64 クランプ | C-054, C-056 |
| [ALS-T17](./text-and-numbers.md#als-t17-datetimeformat-の指定子置換) | datetime.format の指定子置換 | C-128 |
| [ALS-T18](./text-and-numbers.md#als-t18-assert-の-abort-形（非-test-位置）) | assert の abort 形（非 test 位置） | C-153 |
| [ALS-T19](./text-and-numbers.md#als-t19-数値決定性ファミリー) | 数値決定性ファミリー | C-302 |
| [ALS-T20](./text-and-numbers.md#als-t20-丸めと縮約の禁止) | 丸めと縮約の禁止 | C-303 |
| [ALS-T21](./text-and-numbers.md#als-t21-非正規数の保存) | 非正規数の保存 | C-304 |
| [ALS-T22](./text-and-numbers.md#als-t22-超越関数の誤差上限) | 超越関数の誤差上限 | C-305 |
| [ALS-T23](./text-and-numbers.md#als-t23-符号付きゼロ) | 符号付きゼロ | C-306 |
| [ALS-T24](./text-and-numbers.md#als-t24-float-→-int-変換) | Float → Int 変換 | C-307 |
| [ALS-T25](./text-and-numbers.md#als-t25-正準-fast-exp) | 正準 fast-exp | C-223 |

128 sections across 11 chapters.
