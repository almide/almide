// Streaming wrappers for OpenAI-compatible Chat Completions SSE.
//
// These intrinsics own the SSE parsing + delta accumulation so that the
// almide-level caller (e.g. `almai.providers.openai.call_streaming`)
// only has to hand off a `(text_delta) -> Unit` callback for live
// rendering and consume the final LLMResponse-shaped JSON.
//
// `AlmideValue` and `almide_http_stream_dispatch` resolve via flat inlining
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

pub fn almide_rt_sse_openai_chat(
    base_url: &str,
    api_key: &str,
    body_json: &str,
    on_text_delta: std::rc::Rc<dyn Fn(String)>,
) -> Result<String, String> {
    sse_openai_chat_run(base_url, api_key, body_json, None, on_text_delta)
}

/// `openai_streaming_call_with_limits` (#2631): the same helper streaming
/// through the call handle, so `total_ms` / `idle_ms` bound it.
pub fn almide_rt_http___openai_streaming_limited(
    base_url: &str,
    api_key: &str,
    body_json: &str,
    total_ms: i64,
    idle_ms: i64,
    on_text_delta: std::rc::Rc<dyn Fn(String)>,
) -> Result<String, String> {
    sse_openai_chat_run(base_url, api_key, body_json, Some((total_ms, idle_ms)), on_text_delta)
}

fn sse_openai_chat_run(
    base_url: &str,
    api_key: &str,
    body_json: &str,
    limits: Option<(i64, i64)>,
    on_text_delta: std::rc::Rc<dyn Fn(String)>,
) -> Result<String, String> {
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let mut headers: AlmideMap<String, String> = AlmideMap::new();
    headers.insert("Authorization".to_string(), format!("Bearer {}", api_key));
    headers.insert("Content-Type".to_string(), "application/json".to_string());
    headers.insert("Accept".to_string(), "text/event-stream".to_string());

    let mut sse_buffer = String::new();
    let mut acc = AlmideSseAcc::default();
    let mut done = false;

    almide_http_stream_dispatch("POST", &url, body_json, &headers, limits, |chunk: String| {
        if done {
            return;
        }
        sse_buffer.push_str(&chunk);
        while let Some(idx) = sse_buffer.find("\n\n") {
            let event_block: String = sse_buffer.drain(..idx + 2).collect();
            // Each block can have multiple `data:` lines; merge into one payload.
            let (_, data_payload) = sse_event_parts(&event_block);
            if data_payload.is_empty() {
                continue;
            }
            if data_payload.trim() == "[DONE]" {
                // The stream is over: nothing after it is applied, in this
                // chunk or a later one.
                done = true;
                break;
            }
            handle_sse_data(&data_payload, &mut acc, &on_text_delta);
        }
    })?;
    Ok(acc.final_json())
}

// ── SSE event handler ──

#[derive(Default, Clone)]
struct AlmideToolCallAcc {
    id: String,
    name: String,
    arguments: String,
}

/// What one streamed reply has accumulated so far, in either wire shape.
#[derive(Default)]
struct AlmideSseAcc {
    content: String,
    tool_calls: Vec<AlmideToolCallAcc>,
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
    finish_reason: String,
    model_id: String,
}

impl AlmideSseAcc {
    /// The tool call at `index`, growing the list to reach it.
    fn tool(&mut self, index: usize) -> &mut AlmideToolCallAcc {
        if self.tool_calls.len() <= index {
            self.tool_calls.resize_with(index + 1, AlmideToolCallAcc::default);
        }
        &mut self.tool_calls[index]
    }

    /// A text delta: shown live through the callback and kept.
    fn text(&mut self, text: Option<String>, on_text_delta: &std::rc::Rc<dyn Fn(String)>) {
        if let Some(text) = text.filter(|t| !t.is_empty()) {
            on_text_delta(text.clone());
            self.content.push_str(&text);
        }
    }

    /// The LLMResponse-shaped JSON both transports return. A stream that
    /// reported no total (Anthropic never does) totals the two counts.
    fn final_json(&self) -> String {
        let tool_calls_json: String = self
            .tool_calls
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
        let total = if self.total_tokens != 0 { self.total_tokens } else { self.prompt_tokens + self.completion_tokens };
        format!(
            "{{\"content\":\"{}\",\"model\":\"{}\",\"usage\":{{\"prompt_tokens\":{},\"completion_tokens\":{},\"total_tokens\":{}}},\"finish_reason\":\"{}\",\"tool_calls\":[{}]}}",
            json_escape(&self.content),
            json_escape(&self.model_id),
            self.prompt_tokens,
            self.completion_tokens,
            total,
            json_escape(&self.finish_reason),
            tool_calls_json,
        )
    }
}

/// Overwrite `slot` with `value` when it is present and non-empty.
fn sse_set_nonempty(slot: &mut String, value: Option<String>) {
    if let Some(v) = value.filter(|v| !v.is_empty()) {
        *slot = v;
    }
}

/// An SSE event block's `event:` name and its `data:` lines joined by `\n`.
fn sse_event_parts(event_block: &str) -> (String, String) {
    let mut event_name = String::new();
    let mut data_payload = String::new();
    for line in event_block.lines() {
        if let Some(rest) = line.strip_prefix("event:") {
            event_name = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("data:") {
            if !data_payload.is_empty() { data_payload.push('\n'); }
            data_payload.push_str(rest.trim_start());
        }
    }
    (event_name, data_payload)
}

/// One OpenAI Chat Completions `data:` payload.
fn handle_sse_data(payload: &str, acc: &mut AlmideSseAcc, on_text_delta: &std::rc::Rc<dyn Fn(String)>) {
    let Ok(parsed) = almide_rt_json_parse(payload) else { return };
    sse_set_nonempty(&mut acc.model_id, get_string(&parsed, "model"));
    if let Some(usage) = get_field(&parsed, "usage") {
        if let Some(p) = get_int(&usage, "prompt_tokens") { acc.prompt_tokens = p; }
        if let Some(c) = get_int(&usage, "completion_tokens") { acc.completion_tokens = c; }
        if let Some(t) = get_int(&usage, "total_tokens") { acc.total_tokens = t; }
    }
    let Some(AlmideValue::Array(choices)) = get_field(&parsed, "choices") else { return };
    let Some(first) = choices.first() else { return };
    sse_set_nonempty(&mut acc.finish_reason, get_string(first, "finish_reason"));
    let Some(delta) = get_field(first, "delta") else { return };
    acc.text(get_string(&delta, "content"), on_text_delta);
    if let Some(AlmideValue::Array(tcs)) = get_field(&delta, "tool_calls") {
        for tc in tcs {
            openai_tool_call_delta(acc, &tc);
        }
    }
}

/// One entry of an OpenAI delta's `tool_calls`: id and name arrive once,
/// the arguments in fragments.
fn openai_tool_call_delta(acc: &mut AlmideSseAcc, tc: &AlmideValue) {
    let call = acc.tool(get_int(tc, "index").unwrap_or(0) as usize);
    sse_set_nonempty(&mut call.id, get_string(tc, "id"));
    if let Some(func) = get_field(tc, "function") {
        sse_set_nonempty(&mut call.name, get_string(&func, "name"));
        if let Some(args) = get_string(&func, "arguments") {
            call.arguments.push_str(&args);
        }
    }
}

// ── Tiny AlmideValue helpers (mirrors json.rs's getters) ──

fn get_field(v: &AlmideValue, key: &str) -> Option<AlmideValue> {
    if let AlmideValue::Object(pairs) = v {
        for (k, val) in pairs {
            if k.as_ref() == key {
                return Some(val.clone());
            }
        }
    }
    None
}

fn get_string(v: &AlmideValue, key: &str) -> Option<String> {
    if let Some(AlmideValue::Str(s)) = get_field(v, key) {
        Some(s)
    } else {
        None
    }
}

fn get_int(v: &AlmideValue, key: &str) -> Option<i64> {
    match get_field(v, key) {
        Some(AlmideValue::Int(i)) => Some(i),
        Some(AlmideValue::Float(f)) => Some(f as i64),
        _ => None,
    }
}

fn json_escape(s: &str) -> String {
    // The runtime's one JSON escaping rule (value.rs, #2802).
    let mut out = String::with_capacity(s.len());
    almide_rt_value_json_escape_into(&mut out, s);
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

pub fn almide_rt_sse_anthropic_messages(
    api_key: &str,
    body_json: &str,
    on_text_delta: std::rc::Rc<dyn Fn(String)>,
) -> Result<String, String> {
    sse_anthropic_messages_run(api_key, body_json, None, on_text_delta)
}

/// `anthropic_streaming_call_with_limits` (#2631): the same helper streaming
/// through the call handle, so `total_ms` / `idle_ms` bound it.
pub fn almide_rt_http___anthropic_streaming_limited(
    api_key: &str,
    body_json: &str,
    total_ms: i64,
    idle_ms: i64,
    on_text_delta: std::rc::Rc<dyn Fn(String)>,
) -> Result<String, String> {
    sse_anthropic_messages_run(api_key, body_json, Some((total_ms, idle_ms)), on_text_delta)
}

fn sse_anthropic_messages_run(
    api_key: &str,
    body_json: &str,
    limits: Option<(i64, i64)>,
    on_text_delta: std::rc::Rc<dyn Fn(String)>,
) -> Result<String, String> {
    let url = "https://api.anthropic.com/v1/messages".to_string();
    let mut headers: AlmideMap<String, String> = AlmideMap::new();
    headers.insert("x-api-key".to_string(), api_key.to_string());
    headers.insert("anthropic-version".to_string(), "2023-06-01".to_string());
    headers.insert("Content-Type".to_string(), "application/json".to_string());
    headers.insert("Accept".to_string(), "text/event-stream".to_string());

    let mut sse_buffer = String::new();
    // Tool blocks accumulate by `index` across content_block_start / delta.
    let mut acc = AlmideSseAcc::default();

    almide_http_stream_dispatch("POST", &url, body_json, &headers, limits, |chunk: String| {
        sse_buffer.push_str(&chunk);
        while let Some(idx) = sse_buffer.find("\n\n") {
            let event_block: String = sse_buffer.drain(..idx + 2).collect();
            let (event_name, data_payload) = sse_event_parts(&event_block);
            if data_payload.is_empty() { continue; }
            let Ok(parsed) = almide_rt_json_parse(&data_payload) else { continue };
            handle_anthropic_event(&event_name, &parsed, &mut acc, &on_text_delta);
        }
    })?;
    Ok(acc.final_json())
}

/// One Anthropic Messages event, by its `event:` name.
fn handle_anthropic_event(event_name: &str, parsed: &AlmideValue, acc: &mut AlmideSseAcc, on_text_delta: &std::rc::Rc<dyn Fn(String)>) {
    match event_name {
        // {"type":"message_start","message":{"id","model","usage":{"input_tokens","output_tokens"}}}
        "message_start" => anthropic_message_start(parsed, acc),
        // {"index":N,"content_block":{"type":"text"|"tool_use","id?","name?","input?":{}}}
        "content_block_start" => anthropic_block_start(parsed, acc),
        // {"index":N,"delta":{"type":"text_delta"|"input_json_delta","text"|"partial_json"}}
        "content_block_delta" => anthropic_block_delta(parsed, acc, on_text_delta),
        // {"delta":{"stop_reason":"end_turn"|"tool_use"|...},"usage":{"output_tokens":N}}
        "message_delta" => {
            if let Some(delta) = get_field(parsed, "delta") {
                sse_set_nonempty(&mut acc.finish_reason, get_string(&delta, "stop_reason"));
            }
            if let Some(c) = get_field(parsed, "usage").and_then(|u| get_int(&u, "output_tokens")) {
                acc.completion_tokens = c;
            }
        }
        // SSE error event — surface via text_delta so the user sees something.
        "error" => {
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

fn anthropic_message_start(parsed: &AlmideValue, acc: &mut AlmideSseAcc) {
    let Some(msg) = get_field(parsed, "message") else { return };
    sse_set_nonempty(&mut acc.model_id, get_string(&msg, "model"));
    if let Some(usage) = get_field(&msg, "usage") {
        if let Some(p) = get_int(&usage, "input_tokens") { acc.prompt_tokens = p; }
        if let Some(c) = get_int(&usage, "output_tokens") { acc.completion_tokens = c; }
    }
}

/// A `tool_use` block opens its call with id and name; its arguments
/// arrive through input_json_delta (an inlined `input` is not read).
fn anthropic_block_start(parsed: &AlmideValue, acc: &mut AlmideSseAcc) {
    let index = get_int(parsed, "index").unwrap_or(0) as usize;
    let Some(block) = get_field(parsed, "content_block") else { return };
    if get_string(&block, "type").unwrap_or_default() != "tool_use" { return; }
    let call = acc.tool(index);
    sse_set_nonempty(&mut call.id, get_string(&block, "id"));
    sse_set_nonempty(&mut call.name, get_string(&block, "name"));
}

fn anthropic_block_delta(parsed: &AlmideValue, acc: &mut AlmideSseAcc, on_text_delta: &std::rc::Rc<dyn Fn(String)>) {
    let index = get_int(parsed, "index").unwrap_or(0) as usize;
    let Some(delta) = get_field(parsed, "delta") else { return };
    match get_string(&delta, "type").unwrap_or_default().as_str() {
        "text_delta" => acc.text(get_string(&delta, "text"), on_text_delta),
        "input_json_delta" => {
            if let Some(partial) = get_string(&delta, "partial_json") {
                acc.tool(index).arguments.push_str(&partial);
            }
        }
        _ => {}
    }
}
