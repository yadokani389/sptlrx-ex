mod full;
mod pipe;
mod shared;

use anyhow::Result;

use crate::cli::{CliArgs, RenderMode};
use crate::model::LyricState;

pub enum Renderer {
    Full(full::FullRenderer),
    Pipe(pipe::PipeRenderer),
}

impl Renderer {
    pub fn new(config: &CliArgs) -> Self {
        match config.mode {
            RenderMode::Pipe => Self::Pipe(pipe::PipeRenderer::new(
                config.pipe_length(),
                config.pipe_overflow,
            )),
            RenderMode::Full => Self::Full(full::FullRenderer::new(config.debug_enabled())),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Full(_) => "full",
            Self::Pipe(_) => "pipe",
        }
    }

    pub fn start(&mut self) -> Result<()> {
        match self {
            Self::Full(renderer) => renderer.start(),
            Self::Pipe(renderer) => renderer.start(),
        }
    }

    pub fn render(&mut self, state: &LyricState) -> Result<()> {
        match self {
            Self::Full(renderer) => renderer.render(state),
            Self::Pipe(renderer) => renderer.render(state),
        }
    }

    pub fn stop(&mut self) -> Result<()> {
        match self {
            Self::Full(renderer) => renderer.stop(),
            Self::Pipe(renderer) => renderer.stop(),
        }
    }
}
