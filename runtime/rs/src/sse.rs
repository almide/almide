// Streaming wrappers for OpenAI-compatible Chat Completions SSE.
//
// These intrinsics own the SSE parsing + delta accumulation so that the
// almide-level caller (e.g. `almai.providers.openai.call_streaming`)
// only has to hand off a `(text_delta) -> Unit` callback for live
// rendering and consume the final LLMResponse-shaped JSON.
//
// `Value` and `almide_http_request_stream` resolve via flat inlining
// into the user program (the runtime crate isn't a workspace member —
// every module's source is concatenated into a single file at compile
// time). No `use crate::...` imports here.

// ── Public intrinsic ──
//
// Calls an OpenAI-compatible /chat/completions endpoint with `stream:
// true` already set in body_json, parses every SSE `data:` event as
// JSON, accumulates assistant text + tool_calls + usage, and returns a
// JSON string shaped like a non-streaming LLMResponse:
//
// {
//   "content": "...", "model": "...",
//   "usage": {"prompt_tokens": N, "completion_tokens": N, "total_tokens": N},
//   "finish_reason": "...",
//   "tool_calls": [{"id": "...", "name": "...", "arguments": "..."}]
// }
//
// The almide caller `json.parse`s this and slots fields into
// `almai.LLMResponse`.

pub fn almide_sse_openai_chat(
    base_url: &str,
    api_key: &str,
    body_json: &str,
    mut on_text_delta: impl FnMut(String),
) -> Result<String, String> {
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let mut headers: HashMap<String, String> = HashMap::new();
    headers.insert("Authorization".to_string(), format!("Bearer {}", api_key));
    headers.insert("Content-Type".to_string(), "application/json".to_string());
    headers.insert("Accept".to_string(), "text/event-stream".to_string());

    let mut sse_buffer = String::new();
    let mut content = String::new();
    let mut tool_calls: Vec<ToolCallAcc> = Vec::new();
    let mut prompt_tokens: i64 = 0;
    let mut completion_tokens: i64 = 0;
    let mut total_tokens: i64 = 0;
    let mut finish_reason = String::new();
    let mut model_id = String::new();
    let mut done = false;

    almide_http_request_stream("POST", &url, body_json, &headers, |chunk: String| {
        if done {
            return;
        }
        sse_buffer.push_str(&chunk);
        while let Some(idx) = sse_buffer.find("\n\n") {
            let event_block: String = sse_buffer.drain(..idx + 2).collect();
            // Each block can have multiple `data:` lines; merge into one payload.
            let mut data_payload = String::new();
            for line in event_block.lines() {
                if let Some(rest) = line.strip_prefix("data:") {
                    let trimmed = rest.trim_start();
                    if !data_payload.is_empty() {
                        data_payload.push('\n');
                    }
                    data_payload.push_str(trimmed);
                }
            }
            if data_payload.is_empty() {
                continue;
            }
            if data_payload.trim() == "[DONE]" {
                done = true;
                continue;
            }
            handle_sse_data(
                &data_payload,
                &mut content,
                &mut tool_calls,
                &mut prompt_tokens,
                &mut completion_tokens,
                &mut total_tokens,
                &mut finish_reason,
                &mut model_id,
                &mut on_text_delta,
            );
        }
    })?;

    let tool_calls_json: String = tool_calls
        .iter()
        .map(|tc| {
            format!(
                "{{\"id\":\"{}\",\"name\":\"{}\",\"arguments\":\"{}\"}}",
                json_escape(&tc.id),
                json_escape(&tc.name),
                json_escape(&tc.arguments),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let final_json = format!(
        "{{\"content\":\"{}\",\"model\":\"{}\",\"usage\":{{\"prompt_tokens\":{},\"completion_tokens\":{},\"total_tokens\":{}}},\"finish_reason\":\"{}\",\"tool_calls\":[{}]}}",
        json_escape(&content),
        json_escape(&model_id),
        prompt_tokens,
        completion_tokens,
        if total_tokens != 0 { total_tokens } else { prompt_tokens + completion_tokens },
        json_escape(&finish_reason),
        tool_calls_json,
    );
    Ok(final_json)
}

// ── SSE event handler ──

#[derive(Default, Clone)]
struct ToolCallAcc {
    id: String,
    name: String,
    arguments: String,
}

fn handle_sse_data(
    payload: &str,
    content: &mut String,
    tool_calls: &mut Vec<ToolCallAcc>,
    prompt_tokens: &mut i64,
    completion_tokens: &mut i64,
    total_tokens: &mut i64,
    finish_reason: &mut String,
    model_id: &mut String,
    on_text_delta: &mut impl FnMut(String),
) {
    let parsed = match almide_rt_json_parse(payload) {
        Ok(v) => v,
        Err(_) => return,
    };
    if let Some(m) = get_string(&parsed, "model") {
        if !m.is_empty() {
            *model_id = m;
        }
    }
    if let Some(usage) = get_field(&parsed, "usage") {
        if let Some(p) = get_int(&usage, "prompt_tokens") {
            *prompt_tokens = p;
        }
        if let Some(c) = get_int(&usage, "completion_tokens") {
            *completion_tokens = c;
        }
        if let Some(t) = get_int(&usage, "total_tokens") {
            *total_tokens = t;
        }
    }
    let choices = match get_field(&parsed, "choices") {
        Some(Value::Array(a)) => a,
        _ => return,
    };
    let first = match choices.first() {
        Some(c) => c,
        None => return,
    };
    if let Some(fr) = get_string(first, "finish_reason") {
        if !fr.is_empty() {
            *finish_reason = fr;
        }
    }
    let delta = match get_field(first, "delta") {
        Some(d) => d,
        None => return,
    };
    if let Some(text) = get_string(&delta, "content") {
        if !text.is_empty() {
            on_text_delta(text.clone());
            content.push_str(&text);
        }
    }
    if let Some(Value::Array(tcs)) = get_field(&delta, "tool_calls") {
        for tc in tcs {
            let idx = get_int(&tc, "index").unwrap_or(0) as usize;
            while tool_calls.len() <= idx {
                tool_calls.push(ToolCallAcc::default());
            }
            let acc = &mut tool_calls[idx];
            if let Some(id) = get_string(&tc, "id") {
                if !id.is_empty() {
                    acc.id = id;
                }
            }
            if let Some(func) = get_field(&tc, "function") {
                if let Some(name) = get_string(&func, "name") {
                    if !name.is_empty() {
                        acc.name = name;
                    }
                }
                if let Some(args) = get_string(&func, "arguments") {
                    acc.arguments.push_str(&args);
                }
            }
        }
    }
}

// ── Tiny Value helpers (mirrors json.rs's getters) ──

fn get_field(v: &Value, key: &str) -> Option<Value> {
    if let Value::Object(pairs) = v {
        for (k, val) in pairs {
            if k == key {
                return Some(val.clone());
            }
        }
    }
    None
}

fn get_string(v: &Value, key: &str) -> Option<String> {
    if let Some(Value::Str(s)) = get_field(v, key) {
        Some(s)
    } else {
        None
    }
}

fn get_int(v: &Value, key: &str) -> Option<i64> {
    match get_field(v, key) {
        Some(Value::Int(i)) => Some(i),
        Some(Value::Float(f)) => Some(f as i64),
        _ => None,
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

// ── Anthropic Messages API streaming ─────────────────────────────────
//
// Anthropic's SSE wire shape differs from OpenAI's: every SSE event
// carries both `event:` and `data:` lines, and content lives under
// content_block_start / content_block_delta / message_delta. Tool use
// arrives as a content block of type `tool_use`, with arguments
// streamed via input_json_delta (partial JSON fragments concatenated).
//
// Returns the same LLMResponse-shaped JSON the OpenAI helper produces,
// so almai callers can treat both transports uniformly.

pub fn almide_sse_anthropic_messages(
    api_key: &str,
    body_json: &str,
    mut on_text_delta: impl FnMut(String),
) -> Result<String, String> {
    let url = "https://api.anthropic.com/v1/messages".to_string();
    let mut headers: HashMap<String, String> = HashMap::new();
    headers.insert("x-api-key".to_string(), api_key.to_string());
    headers.insert("anthropic-version".to_string(), "2023-06-01".to_string());
    headers.insert("Content-Type".to_string(), "application/json".to_string());
    headers.insert("Accept".to_string(), "text/event-stream".to_string());

    let mut sse_buffer = String::new();
    let mut content = String::new();
    // Tool blocks accumulate by `index` across content_block_start / delta.
    let mut tool_calls: Vec<ToolCallAcc> = Vec::new();
    let mut prompt_tokens: i64 = 0;
    let mut completion_tokens: i64 = 0;
    let mut finish_reason = String::new();
    let mut model_id = String::new();

    almide_http_request_stream("POST", &url, body_json, &headers, |chunk: String| {
        sse_buffer.push_str(&chunk);
        while let Some(idx) = sse_buffer.find("\n\n") {
            let event_block: String = sse_buffer.drain(..idx + 2).collect();
            // Parse the event block into (event_name, data_payload).
            let mut event_name = String::new();
            let mut data_payload = String::new();
            for line in event_block.lines() {
                if let Some(rest) = line.strip_prefix("event:") {
                    event_name = rest.trim().to_string();
                } else if let Some(rest) = line.strip_prefix("data:") {
                    let trimmed = rest.trim_start();
                    if !data_payload.is_empty() { data_payload.push('\n'); }
                    data_payload.push_str(trimmed);
                }
            }
            if data_payload.is_empty() { continue; }
            let parsed = match almide_rt_json_parse(&data_payload) {
                Ok(v) => v,
                Err(_) => continue,
            };
            handle_anthropic_event(
                &event_name,
                &parsed,
                &mut content,
                &mut tool_calls,
                &mut prompt_tokens,
                &mut completion_tokens,
                &mut finish_reason,
                &mut model_id,
                &mut on_text_delta,
            );
        }
    })?;

    let tool_calls_json: String = tool_calls
        .iter()
        .map(|tc| {
            format!(
                "{{\"id\":\"{}\",\"name\":\"{}\",\"arguments\":\"{}\"}}",
                json_escape(&tc.id),
                json_escape(&tc.name),
                json_escape(&tc.arguments),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let total = prompt_tokens + completion_tokens;
    let final_json = format!(
        "{{\"content\":\"{}\",\"model\":\"{}\",\"usage\":{{\"prompt_tokens\":{},\"completion_tokens\":{},\"total_tokens\":{}}},\"finish_reason\":\"{}\",\"tool_calls\":[{}]}}",
        json_escape(&content),
        json_escape(&model_id),
        prompt_tokens,
        completion_tokens,
        total,
        json_escape(&finish_reason),
        tool_calls_json,
    );
    Ok(final_json)
}

fn handle_anthropic_event(
    event_name: &str,
    parsed: &Value,
    content: &mut String,
    tool_calls: &mut Vec<ToolCallAcc>,
    prompt_tokens: &mut i64,
    completion_tokens: &mut i64,
    finish_reason: &mut String,
    model_id: &mut String,
    on_text_delta: &mut impl FnMut(String),
) {
    match event_name {
        "message_start" => {
            // {"type":"message_start","message":{"id","model","usage":{"input_tokens","output_tokens"}}}
            if let Some(msg) = get_field(parsed, "message") {
                if let Some(m) = get_string(&msg, "model") {
                    if !m.is_empty() { *model_id = m; }
                }
                if let Some(usage) = get_field(&msg, "usage") {
                    if let Some(p) = get_int(&usage, "input_tokens") { *prompt_tokens = p; }
                    if let Some(c) = get_int(&usage, "output_tokens") { *completion_tokens = c; }
                }
            }
        }
        "content_block_start" => {
            // {"index":N,"content_block":{"type":"text"|"tool_use","id?","name?","input?":{}}}
            let index = get_int(parsed, "index").unwrap_or(0) as usize;
            if let Some(block) = get_field(parsed, "content_block") {
                let btype = get_string(&block, "type").unwrap_or_default();
                if btype == "tool_use" {
                    while tool_calls.len() <= index {
                        tool_calls.push(ToolCallAcc::default());
                    }
                    let acc = &mut tool_calls[index];
                    if let Some(id) = get_string(&block, "id") {
                        if !id.is_empty() { acc.id = id; }
                    }
                    if let Some(name) = get_string(&block, "name") {
                        if !name.is_empty() { acc.name = name; }
                    }
                    // Anthropic occasionally inlines a complete `input` object on start.
                    if let Some(_input) = get_field(&block, "input") {
                        // Leave args as "" — input_json_delta will populate.
                    }
                }
            }
        }
        "content_block_delta" => {
            // {"index":N,"delta":{"type":"text_delta"|"input_json_delta","text"|"partial_json"}}
            let index = get_int(parsed, "index").unwrap_or(0) as usize;
            if let Some(delta) = get_field(parsed, "delta") {
                let dtype = get_string(&delta, "type").unwrap_or_default();
                if dtype == "text_delta" {
                    if let Some(text) = get_string(&delta, "text") {
                        if !text.is_empty() {
                            on_text_delta(text.clone());
                            content.push_str(&text);
                        }
                    }
                } else if dtype == "input_json_delta" {
                    if let Some(partial) = get_string(&delta, "partial_json") {
                        while tool_calls.len() <= index {
                            tool_calls.push(ToolCallAcc::default());
                        }
                        tool_calls[index].arguments.push_str(&partial);
                    }
                }
            }
        }
        "message_delta" => {
            // {"delta":{"stop_reason":"end_turn"|"tool_use"|...},"usage":{"output_tokens":N}}
            if let Some(delta) = get_field(parsed, "delta") {
                if let Some(reason) = get_string(&delta, "stop_reason") {
                    if !reason.is_empty() { *finish_reason = reason; }
                }
            }
            if let Some(usage) = get_field(parsed, "usage") {
                if let Some(c) = get_int(&usage, "output_tokens") {
                    *completion_tokens = c;
                }
            }
        }
        "error" => {
            // SSE error event — surface via text_delta so the user sees something.
            if let Some(err) = get_field(parsed, "error") {
                let msg = get_string(&err, "message").unwrap_or_default();
                if !msg.is_empty() {
                    on_text_delta(format!("[anthropic stream error] {}", msg));
                }
            }
        }
        _ => {}
    }
}

