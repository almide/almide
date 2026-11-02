历史描述：设计笔记，将 Perceus / borrow checker 风格的机械完备性保证借用于 Almide 自身的pass。
<a id="completeness-by-construction"></a>

# 通过构造实现完备性

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](completeness-by-construction.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](completeness-by-construction.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](completeness-by-construction.zh-CN.md)

> 非规范性固定原文历史设计读者版。固定原本不变，不自动同步，未独立测试或重新验证实现。接受程序正确、使错误状态不可表达等原则是设计目标/不变量，不是新普遍保证。有限矩阵和字节测试保留被测范围；明确指出的runtime/emitter操作仍在证明覆盖外。6月起草/状态行与7月退役、10月矩阵修订并存：§4完成阶段与后文剩余/当前分散、§6开放设计与后文完成、§9未接第三裁判与后来矩阵第三票均保留，不暗中统一为今日状态。内存泄漏例外仍是O(1)主张的例外。历史CI命令和工作顺序只是记录，不是执行授权。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/completeness-by-construction.md)

> 把 Perceus、Rust borrow checker 所有的“机械担保完备性”性质深入编译器细节。
> 不靠约定而靠验证器，比验证器更进一步靠构造——使错误状态**无法表达**。

状态：**Active**——2026-06-10起草。把已有防线
（[correctness-guarantee-gaps](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/correctness-guarantee-gaps.md)=codegen链，
[determinism-belt](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/determinism-belt.md)=确定性，
[certification-grade](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md)=语义权威，
[almide-perceus-belt](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/almide-perceus-belt.md)=RC）扩展到**前端规则一致性与名称同一性**。

<a id="原則"></a>

## 原则

1. **One rule, one place。**（一条规则，一个位置。） 同一语义规则（类型强制、名称限定、unwrap）复制到N处，
   N−1处迟早偏离。规则只放一个函数，所有语法位置调用它。
2. **Producers over consumers。**（优先修正生产者，而非消费者。） 不在下游补错误值（bare name、target-blind wrap），
   而是限制为一个正确producer。
3. **Accept ⟹ correct, reject ⟹ actionable。**（接受即正确，拒绝即可采取行动。） checker接受的程序要么在两target正确运行，
   要么以 `[COMPILER BUG]` 停止。“已接受程序”到达rustc错误或runtime trap，是设计漏洞。
4. **Verifier first, then tighten the types。**（先用验证器，再收紧类型。） 先用验证gate让漏洞可观察，
   达成零后再用类型（newtype / type-state）使复发不可表达（沿Verified→Canonical前例）。

<a id="実証-2026-06-10-の-1-セッションで踏んだ穴-全て修正済み"></a>

## 实证：2026-06-10一个session踩到的漏洞（全已修复）

当天发现7项，全部准确对应“缺少机械保证”。保留为缺了哪些保证机制的实地样本。

| Bug | 类别 | 缺失机制 |
|------|--------|----------------|
| #484 variant payload以bare名emit（E0425） | 名称限定producer遗漏 | 限定只在resolver一处（§1）+ cross-module形状矩阵（§2） |
| #485 只有 `x = effectCall()` 不unwrap（E001） | 规则跨语法位置漂移 | binding强制唯一瓶颈（§3） |
| #486 cross-module top-let从LazyLock move（E0507） | 表示变化后的旧谓词残骸（`starts_with("ALMIDE_RT_")` 已dead） | storage-class判定统一（§4）+ 矩阵（§2） |
| record字面量类型保持bare进入IrTopLet.ty → native E0425 / **wasm运行时trap** | 名称限定最后一个producer遗漏 | §1 + 禁止emit silent-trap（§5） |
| 括号record构造 `Cfg(name: x)` 默默丢弃args（checker放行） | 接受路径未验证fallthrough | TypeName调用全数验证（§6） |
| `almide fmt` 删除仅用于类型位置的import | formatter语义保持无保证 | fmt语义保持gate（§7） |
| record-variant括号模式native E0164 / wasm接受 | 同上（§6另一半） | §6 |

教训：#484/#486在 **12次发布**间无人察觉，因为spec corpus几乎没有cross-module形状。
corpus不经过的形状等同不存在——所以需要**生成**形状的gate（§2）。

<a id="フロンティア-優先順"></a>

## 前沿（优先顺序）

<a id="1-qualifiedref-名前修飾を構築時に強制--433484-クラスの根絶"></a>

### §1 QualifiedRef：构造时强制名称限定 — 根除#433/#484类
- **不变量**：名称解析后的IR没有bare cross-module类型/函数名。
- **(a) ✅ 完成（2026-06-10）**：`verify_names.rs::assert_names_resolvable` 在codegen入口
  （pipeline前，两target、两profile）扫描IrProgram所有Ty位置；
  对“没有bare声明、只有限定声明的bare `Ty::Named`”以 `[COMPILER BUG]` 停止。
  **安装瞬间发现真问题**：derive签名（`register_derive_sigs`）的value类型仍bare到达caller的var_table
  （无症状潜伏）。修复producer，验证器以零offender常驻。纯函数检测器 + 3个单测。
- **(b) 剩余**：持续验证零后，替换成只有resolver能构造的 `QualifiedRef` newtype，
  删除 `MODULE_METHOD_FNS` 等逐pass重新推导。
- **先例**：rustc DefId / GHC Name vs OccName。工作量：(b)数周。

<a id="2-cross-module-形状マトリクスゲート--rustc-エラー--コンパイラバグの機械化"></a>

### §2 Cross-module形状矩阵gate — 机械化“rustc错误=编译器bug”
- **不变量**：生成形状矩阵（定义位置 × 引用形状 × 类型类别 × 绑定位置）的所有cell
  都能以 `--target rust` 编译，wasm不trap。
- **依据**：#484/#486/N2/N3都是矩阵cell。一天踩到4个空白cell。
- **✅ 完成（2026-06-10）**：`tests/crossmod_matrix_test.rs` 在临时project生成20cell
  （定义位置 × 引用形状），硬gate native构建（rustc错误=gate红）+执行+wasmtime字节一致。
  配 `KnownBroken` ratchet（修复必须去flag，只可单调减少）。
  **首跑发现真漏洞**：cross-module括号named构造 `m.Cfg(name:)` 得E002——
  #488归一化只处理 `TypeName` callee，没看Member callee（`m.Cfg(...)`）。同日修复，20/20green。
- **扩展空间**：双package配置（#433同名类型类）、generic record、effect形状新cell。
- **生成矩阵化（2026-10-04，#3309）**：手写20cell外，实际应用移植两天踩11项
  （#3283–#3307）。`tests/shape_matrix_test.rs` + `tests/shape_matrix/gen.rs`
  按族枚举布局（单一/sibling/path依赖package）×声明位置×使用位置×修改形×绑定形×命名
  （大小写差异、生成binder名、mangled static拼写）×同字段record的直积；
  构造时就有各cell期待输出（两leg独立判定 + interp第三票）。
  已知red在 `proofs/shape-matrix-baseline.txt`（cell→open issue，只可缩小）。
  PR固定1/4切片，全量由 `shape-matrix-nightly.yml` 运行。
  xtarget-fuzz的 `--family shape` 将同词汇随机到边界之外。

<a id="3-binding-強制の単一チョークポイント--485-クラスの根絶"></a>

### §3 Binding强制唯一瓶颈 — 根除#485类
- **不变量**：`let x = e` 被接受 ⟺ `x = e` 被接受（除可变性）。
  effect-Result unwrap/coercion在所有binding位置相同。
- **现状**：半完成（2026-06-10）——checker归入 `effect_unwrap_rhs` 一函数，lowering归入
  `coerce_to_target` 一函数。但checker↔lowering两阶段一致仅靠fixture（C-064）固定，未做变形验证。
- **剩余**：复用xtarget-fuzz项生成器，对接受程序互改binding形并assert接受等价的metamorphic CI。
  lambda主体内缺失auto-?（checker保持auto_unwrap，auto_try跳过Lambda——已知共享bug）也会被此gate捕获。
- **先例**：EMI / metamorphic compiler testing。工作量：一周。

<a id="4-top-let-storage-class-決定表--486-クラスの根絶"></a>

### §4 Top-let storage-class决策表 — 根除#486类
- **Stage 0 ✅（2026-06-11）**：形状矩阵增加（mutability × module-origin）cell，立即抓到4个实际有害bug：
  #500（cross-module `var` 两target失败，wasm以typed-zero静默忽略）、
  #501（module fn内self-append向丢弃clone push）、#502（spread base Unknown）、
  #505（直接赋值 `m.x = v` 用VarId(0) fallback变成假lvalue）。
  全修复（v0.27.1），矩阵29/29。读/赋值/捕获的global解析统一到共享helper
  （`lookup_global` / `module_top_let_var`）。
- **Stage 1 ✅（2026-06-11）**：`almide_ir::top_let_storage` = 唯一决策表
  （CopyClass单谓词/classify_storage全域match/static_name唯一格式化点/
  init_can_abort唯一abort谓词/alias解析全域性）。`TopLetStoragePass` 在两pipeline末尾算一次，
  **walker侧agreement verifier** 每次构建assert与legacy谓词
  （lazy_top_let_names / const_top_let_vars / eager_force order / var_storage）一致；
  下次drift不再silent miscompile，而以 `[COMPILER BUG]` 拒绝构建。
  无法解析的module-origin引用由pass以结构化错误拒绝（#500类的全域性）。
- **Stage 2 ✅（2026-06-11，v0.27.2 + endgame）**：全部consumer切换，legacy完全删除。
  移除walker pre-index/register/agreement-verifier，删除lazy_vars / lazy_top_let_names /
  const_top_let_vars / eager_force_top_lets / var_storage_by_name字段。
  `get_var_storage` 仅VarId（名称fallback消失），**VarStorage仅locals**
  （删除ModuleCell/ModuleRc variant，module-global storage只由TopLetStorage负责）。
  pass_clone的 `__cap_`/`__licm` 名称前缀谓词改为producer侧VarId快照
  （`always_clone_vars`，CaptureClone/LICM标记）。全部切换byte-diff zero ×342。
- **Stage 2c（剩余，带行为评审）**：CopyClass规范化（Float32/Unit/numeric-tuple的Cell化等4谓词统一）。
- **不变量**：top-let native存储（const / LazyLock / ModuleCell / ModuleRc）及引用方式
  （move / clone / borrow）由（类型Copy性 × 同/异module × 可变性 × 使用方式）的**全域match**
  只决定一次，emit仅消费属性。
- **现状**：判定分散在pass_clone（always/eligible）、walker（lazy_top_let_names、
  const_top_let_vars、var_storage_by_name）、lowering（module_origin）。#486正因分散漏掉。
- **形式**：沿VarStorageClassification（v0.22）前例汇总到 `TopLetStorage` 属性pass。
  §2矩阵把cell变fixture。工作量：数天。

<a id="5-silent-trap-禁止-emit-の-miss-arm-を-compiler-bug-に"></a>

### §5 禁止silent-trap：emit miss-arm改为 `[COMPILER BUG]`
- **不变量**：emit_wasm查找失败（record_fields miss、func_map miss等）是编译期错误，
  不作为 `unreachable` 指令潜伏到运行时。
- **依据**：N3 wasm trap来自emit_member的field_offset miss默默emit `unreachable`。
  resolved类型上的miss 100%是编译器bug。
- **✅ 完成（2026-06-10）**：转换6处——emit_member field miss
  （仅resolved类型，Unknown作为dead-code残留保持trap）、user-fn func_map miss、
  **不支持IrExprKind的catch-all `_ => unreachable`**（最大漏洞）、FnRef wrapper miss、
  ClosureCreate table miss、value dispatch catch-all。全变 `[ICE]` 构建失败。
  corpus 264/264零误伤。**剩余**：equality.rs:797-798（Unknown-in-dead-code风险高而暂缓），
  calls.rs 994/1139（closure-call非Fn类型else）。

<a id="6-コンストラクタ呼び出しの全数検証--accept-then-explode-の根絶"></a>

### §6 构造器调用全数验证 — 根除accept-then-explode
- **不变量**：`TypeName(...)` / `TypeName { ... }` 只有两选：
  (a)归一化为验证过的构造，或(b)以可操作E-code拒绝。不存在绕过checker到rustc / wasm trap的路径。
- **当前漏洞**（2026-06-10实测）：check/calls.rs:131无条件 `else` fallthrough。
  括号record构造**默默丢弃**named args，brace形式也没有unknown/duplicate/missing-field验证
  （泄漏rustc E0560/E0063）。record-payload variant的括号模式native/wasm行为分叉。
- **形式**：(1)checker将paren-NAMED构造AST归一化到brace pipeline
  （LLM先验频繁产生 `Cfg(name: x)`，MSR上接受才正确）。
  (2)新E-code拒绝positional-on-record（字段重排改变语义，是anti-MSR形）。
  (3)Record arm加入field-set验证（unknown/did-you-mean/duplicate/missing-without-default），
  由brace和归一化paren共享。(4)calls.rs:131 fallthrough改诊断。
  (5)拒绝record-payload case括号模式，提示 `SetEmotion { .. }`。
- 工作量：一周。**下一候选#3**（根因调查和修复设计已完成）。

<a id="7-fmt-意味保存ゲート"></a>

### §7 fmt语义保持gate
- **不变量**：fmt不改语义——输入过类型检查，输出也过。
- **依据**：fmt错误把只用于类型位置的import当“unused”删除（2026-06-10修复）。
  roundtrip gate只看“可重新parse + fixpoint”，会放行改变语义的格式化——设计盲点。
- **✅ 完成（2026-06-10）**：`fmt_output_typechecks_single_file_specs` 将341个单文件spec的fmt输出
  送 `almide check` 的gate，加当天bug类4个直接unit pin。
  **安装瞬间发现5类语义破坏**（在昨日2项之外）：
  (1)match-subject使用 `json.parse` 也删import（AST walker另一漏洞）→ unused判定改为
  **token扫描的超集**（构造上不漏；删除重recall、添加重precision的非对称设计）。
  (2)丢 `mut` 参数→E007。(3)module级 `var` 改成 `let`。
  (4)**整段test `where` 子句删除**。(5)对与stdlib模块同名的局部变量UFCS注入假import
  → ADD侧加bundled-source函数存在验证gate。fmt套件48/48。
  **剩余**：multi-module文件重新类型检查（需sibling解析）、`Decl::TestWhereDef` printer
  （module范围where，未用语法）。

<a id="8-stdlib-意味論マニフェスト--419-クラスの根絶"></a>

### §8 Stdlib语义manifest — 根除#419类
- **不变量**：stdlib函数文档契约维度（index单位codepoint/byte、边界clamp、错误variant、Unicode范围）
  是stdlib定义的结构字段；文档由它生成，每维度property fixture在两target验证主张。
- **依据**：#419——docs明说 `string.len` 是“字符数”，实现却为字节数。
  `index_of`（byte）和 `take/drop`（char）混用单位会在multibyte静默坏掉。
  截至当时没有机器检查doc↔impl一致。
- **形式**：第一步修#419本身（len/index_of/last_index_of统一codepoint，native+wasm同时，
  multibyte spec/wasm_cross fixture + 契约）。第二步stdlib/*.almd @semantics注释
  → 生成文档 + 派生property fixture（与runtime-registry regen gate同样regen-and-diff）。
  工作量：第一步数天，第二步数周。

<a id="9-almide-interp-第三審を配線--nativeoracle-の循環を切る"></a>

### §9 接入almide-interp第三裁判 — 切断native=oracle循环
- 与certification-grade CG-1相同（详见彼处）。fuzzer的 `Oracle` trait零实现，interp零consumer，
  两方投票对“两者同样错误”的bug结构性盲目。工作量：一周。

<a id="10-release-parity-debug-only-検証器の常時化"></a>

### §10 Release-parity：debug-only验证器常驻
- **第一步 ✅（2026-06-10）**：wasmparser::validate改为**始终+致命**。
  §2矩阵CI刚证实实际危害后立即实施：Unit tail var幻影栈值使**emitter出货非法module**；
  仅有wasm-opt的环境偶然修复，无wasm-opt的CI被wasmtime拒绝
  （validate原先print-only，不能阻止出货）。emitter也修复
  （Unit型Var不emit值，遵守ty_to_valtype契约）。raw module批量验证：wasm_cross 106/106 valid。
- **剩余**：逐pass postcondition / MonoVerify提升至release，post-emit RC计数
  （当前debug-only）。工作量：数天。

<a id="残存地図-post-v0270-2026-06-11"></a>

## 剩余地图（post-v0.27.0，2026-06-11）

v0.27.0（true Perceus：wasm frees默认ON）时**不能说**“完备性已确保”的台账。
支柱已立，但这是方向而非宣告，以下仍在。

<a id="メモリ回収の既知有界例外-1-つ-契約-c-066-に明記済み"></a>

### 内存回收的一个已知有界例外（契约C-066已明确）
1. **TCO自身递归循环逐迭代泄漏**——等待Stage C重设计
   （见 [wasm-frees-ownership-discipline](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/wasm-frees-ownership-discipline.md)。
   cherry-pick旧M2污染free list，已否决；acceptance =
   `spec/churn/tco_loop_churn.almd`）。
   安全方向（泄漏而非破坏）——O(1)内存主张须带此脚注。

   ~~2. construct-from-temp引用+1泄漏~~——**退役（#911，2026-07-28）**。
   `emit_stored_field` 随v0 emitter整体删除（#782）；v1 MIR路径在 `verify_ownership` 下
   一次决定所有权，此类未重新推导出来。实测RSS平坦（200k/2M/10M次，含嵌套record形）。
   pin = `spec/churn/construct_from_temp_churn.almd`。

<a id="フロントエンド規則一貫性の残り-本文--優先順"></a>

### 前端规则一致性剩余（正文§，优先顺序）
- **§4** top-let storage-class决策表（结构性根除#486类）——数天
- **§3** metamorphic binding gate（`let x = e` ⟺ `x = e` 接受等价）——一周
- **§1b** QualifiedRef类型化（构造时bare名不可表达）——数周
- **§9** 接入interp第三裁判（两方投票看不到双方同罪bug）——一周
- **核心#433**：类型同一性按package命名空间化；bare名的type identity是端到端结构债。
  §1b是其机械化——数周

<a id="証明の被覆ギャップ"></a>

### 证明覆盖缺口
- **Lean Perceus belt仅到IR级Inc/Dec平衡**——0.27.0具现的runtime frees主体
  （free-list push/reuse、sentinel、region reset的allocator状态不变量）**在证明范围外**。
  churn/byte gate当前是唯一守卫 → [almide-perceus-belt](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/almide-perceus-belt.md) 下一阶段候选。
- emitter级rc操作（stored-field dup、runtime内inc/dec）对IR验证器**不可见**，它只数IR节点。

<a id="着手順序-更新-2026-06-10-第-2-ラウンド"></a>

## 着手顺序（更新2026-06-10第2轮）

1. ~~§2矩阵gate + §5禁止silent-trap~~ ✅
2. ~~§6构造器全数验证~~ ✅（v0.26.20）
3. ~~§7 fmt语义保持 + §8第一步（#419）~~ ✅
4. ~~§1(a) NameResolutionTotal验证器~~ ✅ → 下一步：§4决策表 → §3 metamorphic
5. §1(b) QualifiedRef类型化、§9 interp第三裁判、§10 release-parity——防线永久化

<a id="進捗ログ"></a>

## 进展日志

- 2026-06-10：起草。修复#484/#485/#486 + record-literal限定 + fmt删除import（PR #487）。
  统一 `canonical_user_type_sym`，将 `effect_unwrap_rhs` / `coerce_to_target` 变为瓶颈，
  C-064契约。§6根因调查完成（check/calls.rs:131 fallthrough、lower/calls.rs:83-98默默丢named args）。

- 2026-06-10（第2轮）：§5、§1a、§2、§7四gate安装完成。
  安装本身发现8个潜伏bug（§1a：derive签名bare名producer / §2：Member-callee括号构造E002 /
  §7：fmt语义破坏5类 + 检测器常驻）。实证“bug会继续住在corpus不经过的形状里”的假设。
