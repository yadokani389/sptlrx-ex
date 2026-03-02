use crate::cli::PipeOverflow;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Alignment {
    Center,
}

pub fn wrap_to_width(text: &str, max_width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }

    let safe_width = max_width.max(1);
    let mut output = Vec::new();

    for paragraph in text.split('\n') {
        let graphemes = paragraph.graphemes(true).collect::<Vec<_>>();
        if graphemes.is_empty() {
            output.push(String::new());
            continue;
        }

        let mut line = String::new();
        let mut line_width = 0;

        for grapheme in graphemes {
            let grapheme_width = UnicodeWidthStr::width(grapheme);
            let next_width = line_width + grapheme_width;

            if !line.is_empty() && next_width > safe_width {
                output.push(line);
                line = grapheme.to_string();
                line_width = grapheme_width;
                continue;
            }

            line.push_str(grapheme);
            line_width = next_width;
        }

        output.push(line);
    }

    output
}

pub fn align_line(text: &str, width: usize, alignment: Alignment) -> String {
    let safe_width = width.max(1);
    let text_width = UnicodeWidthStr::width(text);
    if text_width >= safe_width {
        return text.to_string();
    }

    let free = safe_width - text_width;
    match alignment {
        Alignment::Center => {
            let left = free / 2;
            let right = free - left;
            format!("{}{}{}", " ".repeat(left), text, " ".repeat(right))
        }
    }
}

pub fn truncate_to_width(text: &str, max_width: usize, mode: PipeOverflow) -> String {
    if text.is_empty() {
        return String::new();
    }

    let safe_width = max_width.max(1);

    if mode == PipeOverflow::Word {
        return truncate_word_boundary(text, safe_width);
    }

    let wrapped = wrap_to_width(text, safe_width);
    if wrapped.len() <= 1 {
        return wrapped.first().cloned().unwrap_or_default();
    }

    if mode == PipeOverflow::Ellipsis {
        let ellipsis = "...";
        let ellipsis_width = UnicodeWidthStr::width(ellipsis);
        let base_width = safe_width.saturating_sub(ellipsis_width).max(1);
        let clipped = wrap_to_width(&wrapped[0], base_width)
            .first()
            .cloned()
            .unwrap_or_default();
        return format!("{clipped}{ellipsis}");
    }

    wrapped.first().cloned().unwrap_or_default()
}

fn truncate_word_boundary(text: &str, max_width: usize) -> String {
    let first_line = text.split('\n').next().unwrap_or_default();
    if first_line.is_empty() {
        return String::new();
    }

    if UnicodeWidthStr::width(first_line) <= max_width {
        return first_line.to_string();
    }

    let mut result = String::new();
    let mut result_width = 0;

    for word in first_line.split_whitespace() {
        let word_width = UnicodeWidthStr::width(word);

        if result.is_empty() {
            if word_width > max_width {
                return wrap_to_width(first_line, max_width)
                    .first()
                    .cloned()
                    .unwrap_or_default();
            }
            result.push_str(word);
            result_width = word_width;
            continue;
        }

        let next_width = result_width + 1 + word_width;
        if next_width > max_width {
            break;
        }

        result.push(' ');
        result.push_str(word);
        result_width = next_width;
    }

    if result.is_empty() {
        return wrap_to_width(first_line, max_width)
            .first()
            .cloned()
            .unwrap_or_default();
    }

    result
}

#[cfg(test)]
mod tests {
    use super::{Alignment, align_line, truncate_to_width, wrap_to_width};
    use crate::cli::PipeOverflow;

    #[test]
    fn wrap_to_width_splits_long_ascii_text() {
        assert_eq!(wrap_to_width("abcdef", 3), vec!["abc", "def"]);
    }

    #[test]
    fn align_line_centers_text() {
        assert_eq!(align_line("abc", 7, Alignment::Center), "  abc  ");
    }

    #[test]
    fn truncate_to_width_with_ellipsis_keeps_width() {
        assert_eq!(
            truncate_to_width("abcdefgh", 5, PipeOverflow::Ellipsis),
            "ab..."
        );
    }

    #[test]
    fn truncate_to_width_none_mode_clips_first_wrapped_line() {
        assert_eq!(
            truncate_to_width("abcdefgh", 5, PipeOverflow::None),
            "abcde"
        );
    }

    #[test]
    fn truncate_to_width_word_mode_keeps_word_boundary() {
        assert_eq!(
            truncate_to_width("hello world again", 10, PipeOverflow::Word),
            "hello"
        );
    }

    #[test]
    fn truncate_to_width_none_mode_can_split_word() {
        assert_eq!(
            truncate_to_width("hello world", 7, PipeOverflow::None),
            "hello w"
        );
    }

    #[test]
    fn truncate_to_width_word_mode_falls_back_on_long_word() {
        assert_eq!(
            truncate_to_width("supercalifragilistic", 5, PipeOverflow::Word),
            "super"
        );
    }
}
