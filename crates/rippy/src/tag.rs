/// Auto-detected content tags for clipboard entries.
///
/// Detection is heuristic and intentionally simple — better to show a tag
/// that's occasionally wrong than to add a heavyweight parser.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentTag {
    Url,
    Path,
    Code,
    Text,
}

impl ContentTag {
    pub fn label(self) -> &'static str {
        match self {
            Self::Url => "url",
            Self::Path => "path",
            Self::Code => "code",
            Self::Text => "text",
        }
    }
}

/// Classify clipboard content into a content tag.
///
/// Priority: url > path > code > text.
pub fn detect(content: &str) -> ContentTag {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return ContentTag::Text;
    }

    if looks_like_url(trimmed) {
        return ContentTag::Url;
    }
    if looks_like_path(trimmed) {
        return ContentTag::Path;
    }
    if looks_like_code(trimmed) {
        return ContentTag::Code;
    }
    ContentTag::Text
}

fn looks_like_url(s: &str) -> bool {
    let first_line = s.lines().next().unwrap_or("");
    let lower = first_line.trim().to_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("ftp://")
        || lower.starts_with("ssh://")
}

fn looks_like_path(s: &str) -> bool {
    // Only consider single-line content as paths
    if s.lines().count() > 1 {
        return false;
    }
    let trimmed = s.trim();
    // Unix absolute or home-relative paths
    if trimmed.starts_with('/') || trimmed.starts_with("~/") || trimmed.starts_with("./") {
        return true;
    }
    // Contains path separators with a file extension (e.g. "src/main.rs")
    if trimmed.contains('/') && has_file_extension(trimmed) && !trimmed.contains(' ') {
        return true;
    }
    false
}

fn has_file_extension(s: &str) -> bool {
    // Check the last path component for a dot-extension
    let last = s.rsplit('/').next().unwrap_or(s);
    if let Some(dot_pos) = last.rfind('.') {
        let ext = &last[dot_pos + 1..];
        !ext.is_empty() && ext.len() <= 10 && ext.chars().all(|c| c.is_ascii_alphanumeric())
    } else {
        false
    }
}

fn looks_like_code(s: &str) -> bool {
    let lines: Vec<&str> = s.lines().collect();

    // Multi-line with indentation is a strong code signal
    if lines.len() >= 2 {
        let indented = lines.iter().filter(|l| !l.is_empty() && (l.starts_with("  ") || l.starts_with('\t'))).count();
        if indented >= 2 {
            return true;
        }
    }

    // Common code patterns — strong markers (keywords that are almost always code)
    let strong_markers = [
        "fn ", "def ", "func ", "function ", "class ", "struct ", "enum ",
        "import ", "#include", "require(",
    ];
    if strong_markers.iter().any(|m| s.contains(m)) {
        return true;
    }

    // Weaker markers need a matching ending to confirm
    let weak_markers = [
        "from ", "use ", "if (", "if(", "for (", "for(", "while (", "while(",
        "return ", "const ", "let ", "var ", "pub ",
        "=> ", "-> ", "||", "&&",
    ];

    let end_markers = [";", "{", "}", "};", "),", "])", "});", ":"];

    let has_marker = weak_markers.iter().any(|m| s.contains(m));
    let has_end = end_markers.iter().any(|m| {
        lines.iter().any(|l| l.trim().ends_with(m))
    });

    has_marker && has_end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        assert_eq!(detect("https://example.com/foo"), ContentTag::Url);
        assert_eq!(detect("http://localhost:3000"), ContentTag::Url);
        assert_eq!(detect("  https://trimmed.com  "), ContentTag::Url);
        assert_eq!(detect("ftp://files.example.com/data"), ContentTag::Url);
        assert_eq!(detect("ssh://git@github.com:repo"), ContentTag::Url);
    }

    #[test]
    fn paths() {
        assert_eq!(detect("/usr/local/bin/rippy"), ContentTag::Path);
        assert_eq!(detect("~/Documents/notes.md"), ContentTag::Path);
        assert_eq!(detect("./src/main.rs"), ContentTag::Path);
        assert_eq!(detect("src/components/App.tsx"), ContentTag::Path);
    }

    #[test]
    fn paths_not_sentences() {
        // Sentences with slashes shouldn't be paths
        assert_ne!(detect("this is a sentence and/or something"), ContentTag::Path);
    }

    #[test]
    fn code_snippets() {
        assert_eq!(detect("fn main() {\n    println!(\"hello\");\n}"), ContentTag::Code);
        assert_eq!(detect("const x = 42;\nlet y = x + 1;"), ContentTag::Code);
        assert_eq!(detect("import React from 'react';\nfunction App() {\n  return null;\n}"), ContentTag::Code);
        assert_eq!(detect("def foo():\n    return 42"), ContentTag::Code);
    }

    #[test]
    fn plain_text() {
        assert_eq!(detect("hello world"), ContentTag::Text);
        assert_eq!(detect("just some notes about the meeting"), ContentTag::Text);
        assert_eq!(detect(""), ContentTag::Text);
        assert_eq!(detect("   "), ContentTag::Text);
    }

    #[test]
    fn multiline_url_is_url() {
        // URL on first line — still classified as URL
        assert_eq!(detect("https://example.com\nsome description"), ContentTag::Url);
    }

    #[test]
    fn multiline_is_not_path() {
        assert_ne!(detect("/usr/bin/foo\n/usr/bin/bar"), ContentTag::Path);
    }
}
