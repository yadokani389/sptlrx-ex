use clap::{ArgAction, Parser, ValueEnum};
use std::time::Duration;

pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_PORT: u16 = 17373;
const DEFAULT_POLL_MS: u64 = 100;

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum RunRole {
    Auto,
    Bridge,
    Client,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum RenderMode {
    Full,
    Pipe,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum PipeOverflow {
    Word,
    None,
    Ellipsis,
}

#[derive(Parser, Debug, Clone)]
#[command(name = "sptlrx-ex")]
#[command(about = "sptlrx-ex local relay")]
pub struct CliArgs {
    #[arg(long, value_enum, default_value_t = RunRole::Auto)]
    pub role: RunRole,

    #[arg(long, value_enum, default_value_t = RenderMode::Full)]
    pub mode: RenderMode,

    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "no_debug")]
    pub debug: bool,

    #[arg(long = "no-debug", action = ArgAction::SetTrue, conflicts_with = "debug")]
    pub no_debug: bool,

    #[arg(long = "length", default_value_t = 0)]
    pub pipe_length: i64,

    #[arg(long = "overflow", value_enum, default_value_t = PipeOverflow::Word)]
    pub pipe_overflow: PipeOverflow,

    #[arg(long, default_value = DEFAULT_HOST)]
    pub host: String,

    #[arg(long, default_value_t = DEFAULT_PORT)]
    pub port: u16,

    #[arg(long)]
    pub upstream: Option<String>,

    #[arg(long = "poll-ms", default_value_t = DEFAULT_POLL_MS)]
    pub poll_ms: u64,
}

impl CliArgs {
    pub fn debug_enabled(&self) -> bool {
        self.debug && !self.no_debug
    }

    pub fn pipe_length(&self) -> usize {
        if self.pipe_length <= 0 {
            return 0;
        }
        self.pipe_length as usize
    }

    pub fn upstream_base_url(&self) -> String {
        if let Some(value) = &self.upstream {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return trimmed.trim_end_matches('/').to_string();
            }
        }

        format!("http://{}:{}", self.host, self.port)
    }

    pub fn poll_interval(&self) -> Duration {
        Duration::from_millis(self.poll_ms.max(50))
    }
}
