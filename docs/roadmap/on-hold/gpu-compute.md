<!-- description: Matrix primitive type with compiler-driven CPU/GPU execution -->
# GPU Compute — Matrix Type and Compiler-Driven GPU Execution

**Priority:** Phase 3 (Runtime Foundation)
**Prerequisites:** Bytes type completed, WASM export completed, nanopass pipeline completed
**Principle:** Users write ordinary Almide code. They don't think about GPUs. `--target cuda` runs on GPU, otherwise CPU.
**Syntax constraint:** No new keywords. Only a `Matrix` primitive type and a `grad` built-in function are added.

> "You write Almide. Whether it runs on CPU or GPU is the compiler's decision."

---

## Design Philosophy

### Why

The largest untapped area for Almide's mission "the language LLMs can write most accurately" is ML. The most frequent bugs when LLMs write ML code:
1. Tensor dimension mismatches
2. Device placement errors (CPU/GPU)
3. Gradient flow disconnection

Almide's type system + compiler optimization can eliminate these.

### Design Principles

1. **Add no new concepts** — `Matrix` is a primitive just like `Bytes`. `grad` is a built-in just like `println`
2. **Write with operators** — `x * w` is matrix multiplication. Not `tensor.matmul(x, w)`
3. **Hide the GPU** — The backend changes based on the target flag. Code stays the same
4. **Ride the existing Rust ecosystem** — burn/candle already provide autograd, GPU execution, and memory management. No reinventing the wheel
5. **Fuse via nanopass** — Same mechanism as `map |> map` fusion turns element-wise ops into fused kernels

---

## Phases

### Step 1: Matrix Primitive Type

Same pattern as the Bytes type. Minimal compiler changes.

**Language side:**
```
let w = matrix.zeros(512, 1536)
let x = matrix.randn(32, 512)
let y = x * w + bias
let shape = matrix.shape(y)  // → (32, 1536)
```

**Compiler implementation:**
- [ ] Add `Ty::Matrix` to the `Ty` enum
- [ ] Register `TypeConstructorId::Matrix`
- [ ] Add `"Matrix" => Ty::Matrix` to parser/checker type resolution
- [ ] Rust codegen: `ndarray::Array2<f64>` (CPU)
- [ ] `stdlib/defs/matrix.toml` — zeros, randn, shape, transpose, from_lists, to_lists
- [ ] `runtime/rs/src/matrix.rs` — ndarray wrapper
- [ ] Tests

**Operator overloading:**
- [ ] `*` for `(Matrix, Matrix)` → matrix multiplication
- [ ] `+` `-` for `(Matrix, Matrix)` → element-wise
- [ ] `*` `/` for `(Matrix, Float)` → scalar operations
- [ ] Add Matrix patterns to codegen Binary op dispatch

**Verification:** Matrix operations run on CPU via `almide run`

### Step 2: GPU Backend

Switch to burn/candle with `--target cuda`. No user code changes.

**codegen dispatch:**
```
Ty::Matrix codegen →
  target == Rust:  ndarray::Array2<f64>      (CPU)
  target == Cuda:  burn::Tensor<CudaBackend, 2>  (GPU)
```

**Implementation:**
- [ ] Add `codegen::pass::Target::Cuda`
- [ ] Branch Matrix type rendering in Rust codegen based on target
- [ ] `runtime/rs/src/matrix_gpu.rs` — burn wrapper (matmul, add, transpose, etc.)
- [ ] Add burn dependency to `Cargo.toml` template
- [ ] `almide build model.almd --target cuda` generates GPU binary
- [ ] Tests in CUDA environment

**Verification:** Same .almd returns identical results on both CPU and GPU

### Step 3: grad and Training Loops

`grad(loss, w)` expands to burn's autodiff.

**Language side:**
```
fn train_step(x: Matrix, w: Matrix, target: Matrix, lr: Float) -> Matrix =
  let pred = x * w |> leaky_relu(0.5) |> square
  let loss = cross_entropy(pred, target)
  let dw = grad(loss, w)
  w - lr * dw
```

**Compiler implementation:**
- [ ] Recognize `grad(loss, param)` as a built-in function
- [ ] codegen: expand to `loss.backward()` + `grads.get(&param)` (burn autodiff)
- [ ] Mark variables tracked for gradients (burn's `require_grad`)
- [ ] Add activation functions to stdlib: leaky_relu, relu, softmax, sigmoid, tanh
- [ ] Add loss functions to stdlib: cross_entropy, mse
- [ ] Test: small MLP training converges

**Verification:** XOR problem training works

### Step 4: Nanopass Fusion

Fuse element-wise op chains into a single kernel. Directly impacts Parameter Golf.

**Example:**
```
x * w |> leaky_relu(0.5) |> square
// ↓ nanopass rewrite
fused_elementwise(x * w, [leaky_relu(0.5), square])
// ↓ codegen
// 1 fused CUDA kernel (3 kernel launches → 1)
```

**Implementation:**
- [ ] IR pattern: `map(map(x, f), g)` → `map(x, f >> g)` (existing)
- [ ] Element-wise numeric pattern detection pass
- [ ] Fused kernel codegen (Triton or CUDA C)
- [ ] Benchmark: fused vs non-fused speed comparison

**Verification:** Parameter Golf baseline forward pass is faster

### Step 5 (Future): Type-Level Dimension Checking

```
fn linear(x: Matrix[B, 512], w: Matrix[512, 1536]) -> Matrix[B, 1536] =
  x * w
```

Detect dimension mismatches at compile time. A subset of dependent types. Phase 3 and beyond.

---

## Target Mapping

```
Almide type    Rust (CPU)              CUDA (GPU)              WASM
──────────────────────────────────────────────────────────────────────
Int            i64                     i64                     i64
Float          f64                     f64                     f64
String         String                  String                  i32 ptr
Bytes          Vec<u8>                 Vec<u8>                 i32 ptr
Matrix         ndarray::Array2<f64>    burn::Tensor<Cuda, 2>   loops (small scale)
```

## Dependency Crates

| Step | CPU | GPU |
|------|-----|-----|
| 1 | ndarray | — |
| 2 | — | burn (burn-cuda or burn-candle) |
| 3 | — | burn-autodiff |
| 4 | — | burn-fusion or Triton |

## Non-Goals (For Now)

| Item | Reason |
|------|--------|
| N-dimensional tensors | Matrix (2D) is sufficient. 3D+ is future work |
| Custom CUDA kernels | burn/candle kernels are sufficient |
| Distributed training (multi-GPU) | Parameter Golf uses 8xH100, but start with 1 GPU |
| Custom autograd | Delegate to burn. Compiler AD is future consideration |
| TPU/ROCm support | Automatically available once burn supports them |

---

## In One Sentence

> Add a Matrix type as a primitive like Bytes, write matrix operations with operators, take gradients with grad, and the compiler emits CPU/GPU code depending on the target. The user doesn't know GPUs exist.
