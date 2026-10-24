//! ADR-0023 step 7: the embedded lane runs an HTTP program's WASI 0.3
//! component. The stock artifact `to_p3` writes is instantiated here with
//! `wasmtime_wasi::p3` (cli, clocks, filesystem, random, sockets) and
//! `wasmtime_wasi_http::p3` (`wasi:http/types` + `client`) linked — the same
//! imports `wasmtime run -S http` satisfies — so the embedded lane and a stock
//! runtime run ONE artifact, and no op is served by this host for it
//! (decision 1: the op numbers stay between the emitted code and the shim,
//! inside the artifact).
//!
//! The transport is the `send_request` hook, and the hook is `almide-rt-core`
//! — the client native runs (§2.2, refinement 3): wasmtime-wasi-http is built
//! without `default-send-request`, so there is no hyper client and no rustls
//! here. A classified rt-core failure is handed to the guest as the matching
//! `wasi:http` error case, which the shim renders with the URL the program
//! passed (C-328's one renderer); an unclassified one rides as
//! `internal-error(some(text))`, which the shim prints verbatim.
//!
//! The guest runs on a host thread sized for the embedded stack budget, as
//! the core-module runner's does (`host::spawn_guest_thread`).

use std::convert::Infallible;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::p3::bindings::Command;
use wasmtime_wasi::{FsPerms, I32Exit, WasiCtx, WasiCtxView, WasiView};
use wasmtime_wasi_http::{Error as HttpError, RequestOptions, WasiBody, WasiHttpCtx, WasiHttpCtxView, WasiHttpHooks, WasiHttpView};

use almide_rt_core::http_client_core as client;

use crate::host::{spawn_guest_thread, EMBEDDED_WASM_STACK};

/// The store state: the WASI 0.3 context, the wasi:http context and the
/// rt-core transport.
struct State {
    wasi: WasiCtx,
    http: WasiHttpCtx,
    table: ResourceTable,
    transport: RtCoreTransport,
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
    }
}

impl WasiHttpView for State {
    fn http(&mut self) -> WasiHttpCtxView<'_> {
        WasiHttpCtxView { ctx: &mut self.http, table: &mut self.table, hooks: &mut self.transport }
    }
}

/// The wasi:http transport of the embedded lane: native's client.
struct RtCoreTransport;

type SendResult = Result<(http::Response<WasiBody>, Box<dyn Future<Output = Result<(), HttpError>> + Send>), HttpError>;

impl WasiHttpHooks for RtCoreTransport {
    /// rt-core writes its own `Host` (from the URL), as native does.
    fn set_host_header(&mut self) -> bool {
        false
    }

    fn send_request(
        &mut self,
        request: http::Request<WasiBody>,
        _options: Option<RequestOptions>,
        _fut: Box<dyn Future<Output = Result<(), HttpError>> + Send>,
    ) -> Box<dyn Future<Output = SendResult> + Send> {
        Box::new(async move {
            let exchange = Exchange::collect(request).await?;
            let answered = tokio::task::spawn_blocking(move || exchange.send())
                .await
                .map_err(|e| HttpError::InternalError(Some(format!("http transport thread: {e}"))))?;
            let response = response_of(answered?)?;
            Ok((response, Box::new(async { Ok(()) }) as Box<dyn Future<Output = Result<(), HttpError>> + Send>))
        })
    }
}

/// One request in rt-core's terms: method, URL, text body, header pairs.
struct Exchange {
    method: String,
    url: String,
    body: String,
    headers: Vec<(String, String)>,
}

impl Exchange {
    async fn collect(request: http::Request<WasiBody>) -> Result<Self, HttpError> {
        let (parts, body) = request.into_parts();
        let body = body.collect().await?.to_bytes();
        // The shim frames the body with `content-length` for the wasi:http
        // host; rt-core writes its own framing, exactly as native does.
        let headers = parts
            .headers
            .iter()
            .filter(|(k, _)| *k != http::header::CONTENT_LENGTH)
            .map(|(k, v)| (k.as_str().to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned()))
            .collect();
        Ok(Exchange {
            method: parts.method.as_str().to_string(),
            url: parts.uri.to_string(),
            body: String::from_utf8_lossy(&body).into_owned(),
            headers,
        })
    }

    fn send(self) -> Result<client::HttpRawResponse, HttpError> {
        client::request_response_bytes(&self.method, &self.url, &self.body, &self.headers).map_err(|text| classify(&text))
    }
}

/// rt-core's failure text as the wasi:http case of its class (C-328), so the
/// shim renders it with the URL the program passed; a text no class heads is
/// carried whole.
fn classify(text: &str) -> HttpError {
    use client::HttpErrorClass as C;
    let case = |class: C| match class {
        C::Dns => HttpError::DnsError { rcode: None, info_code: None },
        C::Connect => HttpError::ConnectionRefused,
        C::Timeout => HttpError::ConnectionTimeout,
        C::Tls => HttpError::TlsCertificateError,
        C::Protocol => HttpError::HttpProtocolError,
    };
    [C::Dns, C::Connect, C::Timeout, C::Tls, C::Protocol]
        .into_iter()
        .find(|class| {
            let (head, tail) = client::http_error_parts(*class);
            text.starts_with(head) && text.ends_with(tail)
        })
        .map_or_else(|| HttpError::InternalError(Some(text.to_string())), case)
}

/// rt-core's `(status, fields, body)` as the response the guest reads.
fn response_of((status, fields, body): client::HttpRawResponse) -> Result<http::Response<WasiBody>, HttpError> {
    let status = u16::try_from(status).ok().and_then(|s| http::StatusCode::from_u16(s).ok()).ok_or(HttpError::HttpProtocolError)?;
    let mut builder = http::Response::builder().status(status);
    for (k, v) in &fields {
        if let (Ok(name), Ok(value)) = (http::HeaderName::from_bytes(k.as_bytes()), http::HeaderValue::from_bytes(v.as_bytes())) {
            builder = builder.header(name, value);
        }
    }
    let body = Full::new(Bytes::from(body)).map_err(|never: Infallible| match never {}).boxed_unsync();
    builder.body(body).map_err(|e| HttpError::InternalError(Some(e.to_string())))
}

/// Run a WASI 0.3 command component to completion with the run's real stdio,
/// environment and program arguments, and the directories `[permissions] fs`
/// grants preopened (the cwd and the temp dir without the key, #4022; the
/// guest resolves relative paths against `ALMIDE_CWD`, the launcher's cwd,
/// as native's child does). Answers the exit code.
pub fn run_component(bytes: &[u8], args: &[String]) -> anyhow::Result<i32> {
    std::thread::scope(|s| {
        spawn_guest_thread(s, || run_component_here(bytes, args)).join().unwrap_or_else(|p| std::panic::resume_unwind(p))
    })
}

fn run_component_here(bytes: &[u8], args: &[String]) -> anyhow::Result<i32> {
    let mut cfg = Config::new();
    cfg.wasm_component_model_async(true);
    cfg.max_wasm_stack(EMBEDDED_WASM_STACK);
    cfg.async_stack_size(EMBEDDED_WASM_STACK + 1024 * 1024);
    let engine = Engine::new(&cfg)?;
    let component = Component::new(&engine, bytes)?;
    let mut linker = Linker::<State>::new(&engine);
    wasmtime_wasi::p3::add_to_linker(&mut linker)?;
    wasmtime_wasi_http::p3::add_to_linker(&mut linker)?;
    let argv: Vec<&str> = std::iter::once("almide").chain(args.iter().map(String::as_str)).collect();
    let mut wasi = WasiCtx::builder();
    wasi.inherit_stdio().inherit_env().args(&argv);
    // #4022: the granted roots of `[permissions] fs`, or the cwd and the temp
    // dir without the key — never the whole host filesystem at `/`. Each sits
    // at its own host path, so an absolute path names the same file as native.
    for dir in crate::host_fs_scope::preopens() {
        if std::path::Path::new(&dir).is_dir() {
            wasi.preopened_dir(&dir, &dir, FsPerms::ReadWrite)?;
        }
    }
    // The #874 cwd pin native's child gets too: an inherited PWD can be stale.
    if let Some(cwd) = std::env::current_dir().ok().and_then(|d| d.to_str().map(str::to_string)) {
        wasi.env("ALMIDE_CWD", &cwd);
    }
    let state = State { wasi: wasi.build(), http: WasiHttpCtx::new(), table: ResourceTable::new(), transport: RtCoreTransport };
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(async {
        let mut store = Store::new(&engine, state);
        let command = Command::instantiate_async(&mut store, &component, &linker).await?;
        let ran = store.run_concurrent(async move |store| command.wasi_cli_run().call_run(store).await).await;
        exit_code(ran.and_then(|r| r))
    })
}

/// `run`'s answer as an exit code: ok is 0, err is 1, and `wasi:cli/exit`
/// is the code it carries; any other trap is the run's error.
fn exit_code(ran: wasmtime::Result<Result<(), ()>>) -> anyhow::Result<i32> {
    match ran {
        Ok(Ok(())) => Ok(0),
        Ok(Err(())) => Ok(1),
        Err(e) => match e.downcast_ref::<I32Exit>() {
            Some(exit) => Ok(exit.0),
            None => Err(e.into()),
        },
    }
}
