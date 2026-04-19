<!-- description: End-to-end Llama inference demo on Almide, from 1-block to full token generation -->
# Llama Inference Demo

Almide で Llama / Mistral 系 decoder-only モデルを端から端まで動かし、「LLM が Almide 上で動く」を旗として示す arc。Matrix perf arc (fusion stack + NumPy 勝ち) の投資回収段階で、Mission *"the language LLMs can write most accurately"* に直接効く成果物。

## 現在地 (2026-04-19)

- Matrix perf stack: 3²〜1024² 全 shape で NumPy 勝ち、Transformer / Llama 1-block bench も NumPy 超え (`project_matrix_perf_numpy_win.md`)
- Llama 1-block を Almide で組んだ spec + example が merged (PR #218)
  - `spec/stdlib/matrix_llama_block_test.almd` — shape / uniform / residual 3 test
  - `examples/llama_block.almd` — 1 layer を 1 fn に凝縮した demo
  - 使用 intrinsic: `rms_norm_rows`, `linear_row_no_bias`, `masked_multi_head_attention`, `swiglu_gate`, `matrix.add`
- egg saturation が全 fusion を single driver で駆動 (imperative `MatrixFusionPass` / `StreamFusionPass` は廃止済、`project_mlir_egg_stage1_step2.md`)

## Stage 計画

### Stage 0 — 1-block demo 済 ✅

PR #218 で完了。

### Stage 1 — N-block chain

Llama layer を for-loop で重ねる。weight を per-layer の List で持つ形。

- [ ] `fn llama_forward(x, weights: List[LayerWeights], n_layers, ...)` 的な shape で書ける
- [ ] spec: N=2/3 で shape 収束を検証
- [ ] example: `examples/llama_forward.almd` で N-layer 出力

### Stage 2 — NumPy bench 更新 (1-block + N-block)

bench harness の所在を先に再確認 (main repo / 別リポ / local スクリプト)。

- [ ] `project_matrix_perf_numpy_win.md` の 1-block 数値を egg flip 後の現状で refresh
- [ ] N-block (N=4, 8, 12) での数値追加
- [ ] 結果を roadmap 完了セクション or 公開できる README に反映

### Stage 3 — Weight loader

既存 `matrix.from_bytes_f32_le` / `from_bytes_f16_le` / `from_bytes_f64_le` を活用。

- [ ] toy weight file (f32 raw) から matrix を load する example
- [ ] safetensors reader: header JSON parse + tensor offset 解決 (stdlib json を使う)
- [ ] gguf reader (optional、Llama.cpp 互換): Mistral などはこちら
- [ ] Llama 7B / TinyLlama 1.1B の weight file を実際に load

### Stage 4 — Tokenizer (BPE)

Llama / Mistral は SentencePiece BPE。tokenizer.model から vocab + merge rule を読み込み、prompt → token ID sequence に。

- [ ] SentencePiece model format parser (protobuf)
- [ ] BPE encode / decode
- [ ] 英語プロンプトの正確な tokenize
- [ ] special tokens (`<s>`, `</s>`, `<|user|>` など) 対応

### Stage 5 — Inference loop

- [ ] KV-cache (layer ごとに `[seq, d_head]` を追記で保持)
- [ ] causal mask 適用 (既存 `masked_multi_head_attention` で取れているか再確認)
- [ ] sampling: greedy / top-k / top-p / temperature
- [ ] token generation ループ: prompt → N token 生成 → decode
- [ ] `almide run examples/llama_chat.almd "What is Almide?"` で応答が出る

### Stage 6 — 公開資料

- [ ] README.md に「Llama on Almide」セクション
- [ ] blog post 下書き (数値 + demo コード)
- [ ] Dojo の task bank に Llama 関連タスク (inference を LLM に書かせる)

## 規模感

Stage ごとの見積:

| Stage | 想定 session 数 | 代表工数 |
|---|---|---|
| 1: N-block chain | 1 | fn refactor + spec |
| 2: bench 更新 | 1 | harness refresh |
| 3: weight loader | 2-3 | safetensors parser (JSON + binary) |
| 4: tokenizer | 2-3 | BPE + protobuf |
| 5: inference loop | 2-3 | KV-cache + sampling |
| 6: 公開資料 | 1-2 | 編集作業 |

合計 **9-13 session** / 2-3 ヶ月。

## 非ゴール

- LLM の訓練 (forward のみ、back-prop は Stage 7 以降 or 別 arc)
- GPU backend (Stage 5 Rust target で CPU 推論、GPU は `mlir-backend-adoption` Stage 3 と合流)
- 量子化 (GGUF Q8/Q4 などは Stage 3.5 で検討)

## 参照

- 現状の Matrix fusion stack: `docs/roadmap/active/mlir-backend-adoption.md` (Stage 1A 完了)
- bench baseline memo: `project_matrix_perf_numpy_win.md` (私的 memory)
- Whisper E2E: `docs/roadmap/active/whisper-almide.md` の実例
