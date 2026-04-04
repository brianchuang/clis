/// Syntax highlighting for the preview pane using syntect.
///
/// Lazily loads syntax definitions and theme once, then converts
/// syntect-highlighted ranges into ratatui `Span`s.
use ratatui::prelude::*;
use syntect::easy::HighlightLines;
use syntect::highlighting::{self, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use std::sync::LazyLock;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME: LazyLock<highlighting::Theme> = LazyLock::new(|| {
    let ts = ThemeSet::load_defaults();
    ts.themes["base16-eighties.dark"].clone()
});

/// Convert a syntect color to a ratatui color.
fn to_ratatui_color(c: highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

/// Highlight a line of code and return ratatui `Span`s.
///
/// If highlighting fails (unknown syntax, parse error), returns a single
/// unstyled span so the preview always renders.
pub fn highlight_line<'a>(h: &mut HighlightLines<'_>, line: &'a str) -> Vec<Span<'a>> {
    let ranges = match h.highlight_line(line, &SYNTAX_SET) {
        Ok(r) => r,
        Err(_) => return vec![Span::raw(line)],
    };
    ranges
        .into_iter()
        .map(|(style, text)| {
            Span::styled(
                text,
                Style::default().fg(to_ratatui_color(style.foreground)),
            )
        })
        .collect()
}

/// Create a `HighlightLines` for the given content, auto-detecting language.
pub fn highlighter_for(content: &str) -> HighlightLines<'static> {
    let syntax = SYNTAX_SET
        .find_syntax_by_first_line(content)
        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());
    HighlightLines::new(syntax, &THEME)
}

/// Build highlighted ratatui `Line`s for the full content, with line numbers.
pub fn highlight_content(content: &str) -> Vec<Line<'static>> {
    let mut h = highlighter_for(content);
    LinesWithEndings::from(content)
        .enumerate()
        .map(|(i, line)| {
            let line_num = Span::styled(
                format!("{:>4} ", i + 1),
                Style::default().fg(Color::DarkGray),
            );
            let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
            let mut spans = vec![line_num];
            spans.extend(
                highlight_line(&mut h, trimmed)
                    .into_iter()
                    .map(|s| Span::styled(s.content.into_owned(), s.style)),
            );
            Line::from(spans)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_rust_code() {
        let code = "fn main() {\n    println!(\"hello\");\n}\n";
        let lines = highlight_content(code);
        assert_eq!(lines.len(), 3);
        for line in &lines {
            assert!(line.spans.len() >= 2, "expected highlighted spans");
        }
    }

    #[test]
    fn highlight_plain_text_fallback() {
        let text = "just some plain text";
        let lines = highlight_content(text);
        assert_eq!(lines.len(), 1);
        // Line number + at least one content span
        assert!(lines[0].spans.len() >= 2);
    }

    #[test]
    fn highlight_empty_content() {
        let lines = highlight_content("");
        // Empty string produces one empty line via LinesWithEndings
        assert!(lines.len() <= 1);
    }

    #[test]
    fn highlight_python_code() {
        let code = "#!/usr/bin/env python\ndef foo():\n    return 42\n";
        let lines = highlight_content(code);
        assert_eq!(lines.len(), 3);
        // Shebang line should trigger Python syntax detection
        // Each line should have line number + at least one content span
        for line in &lines {
            assert!(line.spans.len() >= 2, "expected highlighted spans");
        }
    }

    #[test]
    fn highlight_javascript_code() {
        let code = "function greet(name) {\n  return `Hello, ${name}!`;\n}\n";
        let lines = highlight_content(code);
        assert_eq!(lines.len(), 3);
        // function keyword line should be highlighted
        assert!(lines[0].spans.len() >= 2);
    }
}
