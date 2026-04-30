use std::io::{IsTerminal, Write, stdout};

use anyhow::Result;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    execute,
    style::{Attribute, Print, SetAttribute},
    terminal::{self, Clear, ClearType},
};

use crate::model::LyricState;

use super::shared::{Alignment, align_line, wrap_to_width};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum LineStyle {
    Normal,
    Bold,
    Dim,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StyledLine {
    text: String,
    style: LineStyle,
}

pub struct FullRenderer {
    debug: bool,
    started: bool,
    last_frame_signature: String,
    sticky_track_key: String,
    sticky_index: isize,
}

impl FullRenderer {
    pub fn new(debug: bool) -> Self {
        Self {
            debug,
            started: false,
            last_frame_signature: String::new(),
            sticky_track_key: String::new(),
            sticky_index: -1,
        }
    }

    pub fn start(&mut self) -> Result<()> {
        if self.started {
            return Ok(());
        }

        self.started = true;
        let mut out = stdout();
        if out.is_terminal() {
            execute!(out, Hide, MoveTo(0, 0), Clear(ClearType::All))?;
            out.flush()?;
        }

        Ok(())
    }

    pub fn render(&mut self, state: &LyricState) -> Result<()> {
        let (width, height) = terminal_size();
        let frame = render_lyrics_frame(
            state,
            width,
            height,
            &mut self.sticky_track_key,
            &mut self.sticky_index,
        );

        if let Some(frame) = frame {
            self.draw(&frame)?;
            return Ok(());
        }

        if self.debug && !state.status.is_empty() {
            let frame = render_hint_frame(width, height, &state.status);
            self.draw(&frame)?;
            return Ok(());
        }

        let frame = blank_frame(width, height);
        self.draw(&frame)
    }

    pub fn stop(&mut self) -> Result<()> {
        if !self.started {
            return Ok(());
        }

        self.started = false;
        let mut out = stdout();
        if out.is_terminal() {
            execute!(out, SetAttribute(Attribute::Reset), Show)?;
            out.flush()?;
        }

        Ok(())
    }

    fn draw(&mut self, frame: &[StyledLine]) -> Result<()> {
        let mut out = stdout();
        if !out.is_terminal() {
            return Ok(());
        }

        let signature = frame_signature(frame);
        if signature == self.last_frame_signature {
            return Ok(());
        }
        self.last_frame_signature = signature;

        execute!(out, MoveTo(0, 0), Clear(ClearType::All))?;

        for (index, line) in frame.iter().enumerate() {
            if index > 0 {
                execute!(out, Print("\n"))?;
            }

            let style = match line.style {
                LineStyle::Normal => Attribute::Reset,
                LineStyle::Bold => Attribute::Bold,
                LineStyle::Dim => Attribute::Dim,
            };

            execute!(
                out,
                SetAttribute(style),
                Print(&line.text),
                SetAttribute(Attribute::Reset)
            )?;
        }

        out.flush()?;
        Ok(())
    }
}

fn terminal_size() -> (usize, usize) {
    match terminal::size() {
        Ok((width, height)) => (usize::from(width).max(1), usize::from(height).max(1)),
        Err(_) => (80, 24),
    }
}

fn blank_frame(width: usize, height: usize) -> Vec<StyledLine> {
    let row = " ".repeat(width.max(1));
    (0..height.max(1))
        .map(|_| StyledLine {
            text: row.clone(),
            style: LineStyle::Normal,
        })
        .collect()
}

fn render_lyric_line(text: &str, width: usize, style: LineStyle) -> Vec<StyledLine> {
    wrap_to_width(text, width)
        .into_iter()
        .map(|line| StyledLine {
            text: align_line(&line, width, Alignment::Center),
            style,
        })
        .collect()
}

fn render_hint_frame(width: usize, height: usize, hint: &str) -> Vec<StyledLine> {
    let mut frame = blank_frame(width, height);
    if hint.is_empty() {
        return frame;
    }

    let rendered = render_lyric_line(hint, width, LineStyle::Dim);
    let top = height.saturating_sub(rendered.len()) / 2;

    for (offset, line) in rendered.into_iter().enumerate() {
        let index = top + offset;
        if index < frame.len() {
            frame[index] = line;
        }
    }

    frame
}

fn render_single_centered_line(text: &str, width: usize, height: usize) -> Option<Vec<StyledLine>> {
    if text.is_empty() {
        return None;
    }

    let mut frame = blank_frame(width, height);
    let rendered = render_lyric_line(text, width, LineStyle::Bold);
    let top = height.saturating_sub(rendered.len()) / 2;

    for (offset, line) in rendered.into_iter().enumerate() {
        let index = top + offset;
        if index < frame.len() {
            frame[index] = line;
        }
    }

    Some(frame)
}

fn resolve_current_index(state: &LyricState, lines: &[String]) -> isize {
    if lines.is_empty() {
        return -1;
    }

    if let Some(current_line) = &state.current_line {
        if current_line.index >= 0 {
            let index = current_line.index as usize;
            if index < lines.len() {
                return current_line.index as isize;
            }
        }

        if !current_line.text.is_empty()
            && let Some(index) = lines.iter().position(|line| line == &current_line.text)
        {
            return index as isize;
        }
    }

    -1
}

fn resolve_track_key(state: &LyricState, lines: &[String]) -> String {
    let first = lines.first().cloned().unwrap_or_default();
    let last = lines.last().cloned().unwrap_or_default();
    format!(
        "{}::{}::{}::{}::{}",
        state.title,
        state.artists.join("|"),
        lines.len(),
        first,
        last
    )
}

fn should_draw_blank(state: &LyricState) -> bool {
    state.status == "lyrics_panel_closed" || state.status == "lyrics_not_available"
}

fn resolve_display_index(state: &LyricState, lines: &[String], sticky_index: &mut isize) -> isize {
    if lines.is_empty() {
        *sticky_index = -1;
        return -1;
    }

    let current_index = resolve_current_index(state, lines);
    if current_index >= 0 {
        *sticky_index = current_index;
        return current_index;
    }

    if *sticky_index >= 0 {
        let sticky = *sticky_index as usize;
        if sticky < lines.len() {
            return *sticky_index;
        }
    }

    -1
}

fn fill_before(
    frame: &mut [StyledLine],
    lines: &[String],
    current_index: usize,
    row_limit: usize,
    width: usize,
) {
    if current_index == 0 {
        return;
    }

    let mut cursor = row_limit as isize - 1;
    for index in (0..current_index).rev() {
        if cursor < 0 {
            break;
        }

        let rendered = render_lyric_line(&lines[index], width, LineStyle::Bold);
        for line in rendered.into_iter().rev() {
            if cursor < 0 {
                break;
            }
            frame[cursor as usize] = line;
            cursor -= 1;
        }
    }
}

fn fill_after(
    frame: &mut [StyledLine],
    lines: &[String],
    current_index: usize,
    start_row: usize,
    width: usize,
) {
    let mut cursor = start_row;
    for line_text in lines.iter().skip(current_index + 1) {
        if cursor >= frame.len() {
            break;
        }

        let rendered = render_lyric_line(line_text, width, LineStyle::Dim);
        for line in rendered {
            if cursor >= frame.len() {
                break;
            }
            frame[cursor] = line;
            cursor += 1;
        }
    }
}

fn render_lyrics_frame_with_index(
    lines: &[String],
    current_index: usize,
    width: usize,
    height: usize,
) -> Option<Vec<StyledLine>> {
    if lines.is_empty() || current_index >= lines.len() {
        return None;
    }

    let mut frame = blank_frame(width, height);
    let rendered_current = render_lyric_line(&lines[current_index], width, LineStyle::Bold);
    let current_height = rendered_current.len();

    let before_rows = height.saturating_sub(current_height) / 2;
    let current_top = before_rows;
    let after_start = (current_top + current_height).min(frame.len());

    fill_before(&mut frame, lines, current_index, before_rows, width);
    for (offset, line) in rendered_current.into_iter().enumerate() {
        let row = current_top + offset;
        if row < frame.len() {
            frame[row] = line;
        }
    }
    fill_after(&mut frame, lines, current_index, after_start, width);

    Some(frame)
}

fn render_lyrics_frame(
    state: &LyricState,
    width: usize,
    height: usize,
    sticky_track_key: &mut String,
    sticky_index: &mut isize,
) -> Option<Vec<StyledLine>> {
    let lines = &state.lines;
    let track_key = resolve_track_key(state, lines);
    if *sticky_track_key != track_key {
        *sticky_track_key = track_key;
        *sticky_index = -1;
    }

    if should_draw_blank(state) {
        *sticky_index = -1;
        return None;
    }

    if lines.is_empty() {
        *sticky_index = -1;
        return render_single_centered_line(
            state
                .current_line
                .as_ref()
                .map(|line| line.text.as_str())
                .unwrap_or_default(),
            width,
            height,
        );
    }

    let index = resolve_display_index(state, lines, sticky_index);
    if index >= 0 {
        return render_lyrics_frame_with_index(lines, index as usize, width, height);
    }

    render_single_centered_line(
        state
            .current_line
            .as_ref()
            .map(|line| line.text.as_str())
            .unwrap_or_default(),
        width,
        height,
    )
}

fn frame_signature(frame: &[StyledLine]) -> String {
    let mut signature = String::new();
    for line in frame {
        let style = match line.style {
            LineStyle::Normal => 'n',
            LineStyle::Bold => 'b',
            LineStyle::Dim => 'd',
        };
        signature.push(style);
        signature.push('|');
        signature.push_str(&line.text);
        signature.push('\n');
    }
    signature
}
