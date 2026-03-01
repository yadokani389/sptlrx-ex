use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Request, State},
    http::{HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::any,
};
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use crate::{
    cli::CliArgs,
    model::{LyricState, PayloadError},
    render::Renderer,
};

const MAX_BODY_BYTES: usize = 512 * 1024;

#[derive(Clone)]
struct AppState {
    inner: Arc<Mutex<BridgeState>>,
    mode: &'static str,
}

struct BridgeState {
    latest_state: LyricState,
    renderer: Renderer,
}

#[derive(Serialize)]
struct HealthResponse<'a> {
    ok: bool,
    service: &'a str,
    mode: &'a str,
}

#[derive(Serialize)]
struct StateResponse<'a> {
    ok: bool,
    mode: &'a str,
    state: LyricState,
}

#[derive(Serialize)]
struct ErrorResponse<'a> {
    ok: bool,
    error: &'a str,
}

pub async fn run(args: CliArgs) -> Result<()> {
    let listener = bind_listener(&args.host, args.port)
        .await
        .with_context(|| format!("failed to bind {}:{}", args.host, args.port))?;
    run_with_listener(args, listener).await
}

pub async fn bind_listener(host: &str, port: u16) -> std::io::Result<TcpListener> {
    let bind = format!("{host}:{port}");
    tokio::net::TcpListener::bind(bind).await
}

pub async fn run_with_listener(args: CliArgs, listener: TcpListener) -> Result<()> {
    let mut renderer = Renderer::new(&args);
    let mode = renderer.name();
    renderer.start().context("failed to start renderer")?;

    let state = AppState {
        inner: Arc::new(Mutex::new(BridgeState {
            latest_state: LyricState::waiting(),
            renderer,
        })),
        mode,
    };

    let app = Router::new()
        .route("/lyrics", any(handle_lyrics))
        .route("/health", any(handle_health))
        .route("/state", any(handle_state))
        .fallback(any(handle_not_found))
        .with_state(state.clone());

    eprintln!("[relay] Listening on http://{}:{}", args.host, args.port);
    eprintln!(
        "[relay] Health endpoint: http://{}:{}/health",
        args.host, args.port
    );
    eprintln!(
        "[relay] State endpoint: http://{}:{}/state",
        args.host, args.port
    );
    eprintln!("[relay] Mode: {mode}");
    if mode == "pipe" {
        eprintln!("[relay] Pipe output is written to stdout");
    }

    let serve_result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await;

    stop_renderer(&state).await;

    serve_result.context("server terminated with an error")
}

async fn stop_renderer(state: &AppState) {
    let mut guard = state.inner.lock().await;
    if let Err(error) = guard.renderer.stop() {
        eprintln!("[relay] failed to stop renderer: {error}");
    }
}

async fn handle_lyrics(State(state): State<AppState>, request: Request) -> Response {
    match *request.method() {
        Method::OPTIONS => no_content_response(StatusCode::NO_CONTENT),
        Method::POST => handle_lyrics_post(state, request).await,
        _ => not_found_response(),
    }
}

async fn handle_health(State(state): State<AppState>, request: Request) -> Response {
    if *request.method() != Method::GET {
        return not_found_response();
    }

    json_response(
        StatusCode::OK,
        &HealthResponse {
            ok: true,
            service: "sptlrx-ex",
            mode: state.mode,
        },
    )
}

async fn handle_state(State(state): State<AppState>, request: Request) -> Response {
    if *request.method() != Method::GET {
        return not_found_response();
    }

    let latest_state = {
        let guard = state.inner.lock().await;
        guard.latest_state.clone()
    };

    json_response(
        StatusCode::OK,
        &StateResponse {
            ok: true,
            mode: state.mode,
            state: latest_state,
        },
    )
}

async fn handle_not_found(State(_state): State<AppState>, _request: Request) -> Response {
    not_found_response()
}

async fn handle_lyrics_post(state: AppState, request: Request) -> Response {
    let body = match to_bytes(request.into_body(), MAX_BODY_BYTES + 1).await {
        Ok(body) => body,
        Err(_) => return error_response(StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large"),
    };

    if body.len() > MAX_BODY_BYTES {
        return error_response(StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large");
    }

    let parsed = match LyricState::from_json_bytes(&body) {
        Ok(parsed) => parsed,
        Err(PayloadError::InvalidJson) => {
            return error_response(StatusCode::BAD_REQUEST, "invalid_json");
        }
        Err(PayloadError::InvalidPayload) => {
            return error_response(StatusCode::BAD_REQUEST, "invalid_payload");
        }
    };

    let mut guard = state.inner.lock().await;
    guard.latest_state = parsed.clone();
    if let Err(error) = guard.renderer.render(&parsed) {
        eprintln!("[relay] renderer error: {error}");
    }

    no_content_response(StatusCode::NO_CONTENT)
}

fn no_content_response(status: StatusCode) -> Response {
    with_cors(status.into_response())
}

fn not_found_response() -> Response {
    error_response(StatusCode::NOT_FOUND, "not_found")
}

fn error_response(status: StatusCode, code: &'static str) -> Response {
    json_response(
        status,
        &ErrorResponse {
            ok: false,
            error: code,
        },
    )
}

fn json_response<T>(status: StatusCode, payload: &T) -> Response
where
    T: Serialize,
{
    let response = (status, Json(payload)).into_response();
    with_cors(response)
}

fn with_cors(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type"),
    );
    headers.insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static("600"),
    );
    response
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut stream) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            let _ = stream.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
      _ = ctrl_c => {},
      _ = terminate => {},
    }
}
