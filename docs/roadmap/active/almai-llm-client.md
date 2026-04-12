<!-- description: almai — multi-provider LLM client library, 8 providers shipped -->
# almai — Multi-Provider LLM Client

## Status

v0.1.0 shipped (2026-04-12). 8 providers, tool calling, JSON mode, conversation builder.

Repository: [github.com/almide/almai](https://github.com/almide/almai)

## What's done

- 8 providers: Anthropic, OpenAI, OpenRouter, Cloudflare Workers AI, Azure OpenAI, Google Gemini, AWS Bedrock, CLI (Claude/Codex)
- Tool/function calling with JSON Schema helpers (`almai.tools`)
- JSON mode (`with_json_mode`)
- Conversation builder for multi-turn (`almai.conv`)
- Provider submodule architecture (one file per provider)
- Options builder pattern (`defaults() |> with_max_tokens(8192) |> ...`)
- API error extraction from response JSON
- README with tested examples

## What's next (v0.2)

- Streaming support
- Retry with exponential backoff (429, 500, 503)
- Typed error variants
- Bedrock SigV4 signing (needs `bytes.hmac_sha256` in stdlib)
- Vertex AI provider
- Token counting / cost estimation

## Almide Dojo integration

almide-dojo uses almai as its LLM backend via `import almai`. The dojo's `all` command runs 30 tasks against any provider string.
