use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::Deserialize;

use crate::cli::CliArgs;
use crate::model::LyricState;
use crate::render::Renderer;

const REQUEST_TIMEOUT: Duration = Duration::from_millis(1500);
const ERROR_LOG_THROTTLE: Duration = Duration::from_millis(5000);
const HEALTH_PROBE_RETRY: usize = 4;
const HEALTH_PROBE_RETRY_DELAY: Duration = Duration::from_millis(150);

#[derive(Debug, Deserialize)]
struct HealthResponse {
    ok: bool,
    mode: String,
    service: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StateResponse {
    ok: bool,
    state: LyricState,
}

pub async fn run(args: CliArgs) -> Result<()> {
    let upstream = args.upstream_base_url();
    let poll_interval = args.poll_interval();

    let http = build_http_client()?;
    let mut renderer = Renderer::new(&args);
    renderer.start().context("failed to start renderer")?;

    eprintln!("[client] Upstream: {upstream}");
    eprintln!("[client] Poll interval: {}ms", poll_interval.as_millis());
    eprintln!("[client] Mode: {}", renderer.name());

    let mut interval = tokio::time::interval(poll_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);

    let mut last_error_at = Instant::now() - ERROR_LOG_THROTTLE;

    loop {
        tokio::select! {
          _ = &mut shutdown => {
            break;
          }
          _ = interval.tick() => {
            match fetch_state(&http, &upstream).await {
              Ok(state) => {
                if let Err(error) = renderer.render(&state) {
                  eprintln!("[client] renderer error: {error}");
                }
              }
              Err(error) => {
                let now = Instant::now();
                if now.duration_since(last_error_at) >= ERROR_LOG_THROTTLE {
                  eprintln!("[client] state poll failed: {error}");
                  last_error_at = now;
                }
              }
            }
          }
        }
    }

    renderer.stop().context("failed to stop renderer")
}

pub async fn looks_like_bridge(upstream: &str) -> bool {
    let Ok(http) = build_http_client() else {
        return false;
    };

    for attempt in 0..HEALTH_PROBE_RETRY {
        if probe_health_once(&http, upstream).await {
            return true;
        }

        if attempt + 1 < HEALTH_PROBE_RETRY {
            tokio::time::sleep(HEALTH_PROBE_RETRY_DELAY).await;
        }
    }

    false
}

async fn probe_health_once(http: &Client, upstream: &str) -> bool {
    let url = endpoint_url(upstream, "health");
    let Ok(response) = http.get(url).send().await else {
        return false;
    };

    if !response.status().is_success() {
        return false;
    }

    let Ok(health) = response.json::<HealthResponse>().await else {
        return false;
    };

    if !health.ok {
        return false;
    }

    if health.mode != "full" && health.mode != "pipe" {
        return false;
    }

    if let Some(service) = health.service {
        return service == "sptlrx-ex";
    }

    true
}

fn build_http_client() -> Result<Client> {
    Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .context("failed to create http client")
}

async fn fetch_state(http: &Client, upstream: &str) -> Result<LyricState> {
    let url = endpoint_url(upstream, "state");
    let response = http
        .get(url)
        .send()
        .await
        .context("request to /state failed")?;

    if !response.status().is_success() {
        bail!("/state returned non-success status: {}", response.status());
    }

    let payload = response
        .json::<StateResponse>()
        .await
        .context("failed to decode /state response")?;

    if !payload.ok {
        bail!("/state returned ok=false");
    }

    Ok(payload.state)
}

fn endpoint_url(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), path)
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
