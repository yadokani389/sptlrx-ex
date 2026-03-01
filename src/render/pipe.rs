use std::io::{Write, stdout};

use anyhow::Result;
use crossterm::{execute, style::Print};

use crate::cli::PipeOverflow;
use crate::model::LyricState;

use super::shared::truncate_to_width;

pub struct PipeRenderer {
    pipe_length: usize,
    overflow: PipeOverflow,
    last_printed: Option<String>,
}

impl PipeRenderer {
    pub fn new(pipe_length: usize, overflow: PipeOverflow) -> Self {
        Self {
            pipe_length,
            overflow,
            last_printed: None,
        }
    }

    pub fn start(&mut self) -> Result<()> {
        Ok(())
    }

    pub fn render(&mut self, state: &LyricState) -> Result<()> {
        let mut line = state
            .current_line
            .as_ref()
            .map(|line| line.text.clone())
            .unwrap_or_default();

        if self.pipe_length > 0 && !line.is_empty() {
            line = truncate_to_width(&line, self.pipe_length, self.overflow);
        }

        if self.last_printed.as_deref() == Some(line.as_str()) {
            return Ok(());
        }

        self.last_printed = Some(line.clone());

        let mut out = stdout();
        execute!(out, Print(line), Print("\n"))?;
        out.flush()?;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        Ok(())
    }
}
