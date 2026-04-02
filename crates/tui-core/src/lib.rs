use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::prelude::*;
use ratatui::widgets::*;

// --- Types ---

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Normal,
    Insert,
}

/// Actions shared across all vim-modal TUI apps.
/// App-specific actions (Select, CopyAndQuit, DeleteSelected, etc.)
/// are handled by the caller when `handle_key` returns `None`.
#[derive(Debug, PartialEq)]
pub enum NavAction {
    Quit,
    MoveUp,
    MoveDown,
    MoveToTop,
    MoveToBottom,
    HalfPageUp,
    HalfPageDown,
    NextMatch,
    PrevMatch,
    ShowHelp,
    EnterInsert,
    ExitInsert,
    TypeChar(char),
    Backspace,
    ClearSearch,
    Noop,
}

// --- Key handling ---

/// Dispatch a key event to the shared vim-modal handler.
///
/// Returns `Some(NavAction)` for shared bindings, `None` for keys the app
/// should handle itself (e.g. Enter, app-specific combos like `dd`).
///
/// `pending` tracks multi-key combos (e.g. `gg`). Pass `&mut app.pending_key`.
/// `extra_pending_keys` lists single-char keys that start app-specific combos
/// (e.g. `&['d']` for rippy's `dd` delete). These will set pending and return `None`.
pub fn handle_key(
    key: KeyEvent,
    mode: Mode,
    pending: &mut Option<char>,
    extra_pending_keys: &[char],
) -> Option<NavAction> {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Some(NavAction::Quit);
    }

    match mode {
        Mode::Normal => handle_normal_key(key, pending, extra_pending_keys),
        Mode::Insert => handle_insert_key(key),
    }
}

fn handle_normal_key(
    key: KeyEvent,
    pending: &mut Option<char>,
    extra_pending_keys: &[char],
) -> Option<NavAction> {
    // Resolve pending multi-key combos
    if let Some(first) = pending.take() {
        return match (first, key.code) {
            ('g', KeyCode::Char('g')) => Some(NavAction::MoveToTop),
            _ => {
                // Unknown combo — might be app-specific (e.g. 'd','d')
                // Return None so the app can handle it
                if extra_pending_keys.contains(&first) {
                    // Re-encode as a signal: set pending back so the caller can inspect
                    // Actually, we need a different approach. Let's just return None
                    // and let the caller handle pending combos entirely for their keys.
                    None
                } else {
                    Some(NavAction::Noop)
                }
            }
        };
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('u') => Some(NavAction::HalfPageUp),
            KeyCode::Char('d') => Some(NavAction::HalfPageDown),
            _ => Some(NavAction::Noop),
        };
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => Some(NavAction::Quit),
        KeyCode::Char('j') | KeyCode::Down => Some(NavAction::MoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(NavAction::MoveUp),
        KeyCode::Char('G') => Some(NavAction::MoveToBottom),
        KeyCode::Char('g') => { *pending = Some('g'); Some(NavAction::Noop) }
        KeyCode::Char('n') => Some(NavAction::NextMatch),
        KeyCode::Char('N') => Some(NavAction::PrevMatch),
        KeyCode::Char('?') => Some(NavAction::ShowHelp),
        KeyCode::Char('/') | KeyCode::Char('i') => Some(NavAction::EnterInsert),
        KeyCode::Enter => None, // App-specific
        KeyCode::Char(c) if extra_pending_keys.contains(&c) => {
            *pending = Some(c);
            None // App will handle the combo
        }
        _ => Some(NavAction::Noop),
    }
}

fn handle_insert_key(key: KeyEvent) -> Option<NavAction> {
    match key.code {
        KeyCode::Esc => Some(NavAction::ExitInsert),
        KeyCode::Enter => None, // App-specific
        KeyCode::Backspace => Some(NavAction::Backspace),
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(NavAction::ClearSearch),
        KeyCode::Up => Some(NavAction::MoveUp),
        KeyCode::Down => Some(NavAction::MoveDown),
        KeyCode::Char(c) => Some(NavAction::TypeChar(c)),
        _ => Some(NavAction::Noop),
    }
}

// --- Navigation ---

/// Apply a navigation action, returning the new `selected` index.
pub fn apply_navigation(
    action: &NavAction,
    selected: usize,
    filtered_len: usize,
    list_height: usize,
) -> usize {
    match action {
        NavAction::MoveUp => selected.saturating_sub(1),
        NavAction::MoveDown => {
            if selected + 1 < filtered_len {
                selected + 1
            } else {
                selected
            }
        }
        NavAction::MoveToTop => 0,
        NavAction::MoveToBottom => {
            if filtered_len > 0 {
                filtered_len - 1
            } else {
                selected
            }
        }
        NavAction::HalfPageUp => {
            let half = list_height / 2;
            selected.saturating_sub(half.max(1))
        }
        NavAction::HalfPageDown => {
            let half = list_height / 2;
            if filtered_len > 0 {
                (selected + half.max(1)).min(filtered_len - 1)
            } else {
                selected
            }
        }
        NavAction::NextMatch => {
            if filtered_len > 0 {
                (selected + 1) % filtered_len
            } else {
                selected
            }
        }
        NavAction::PrevMatch => {
            if filtered_len > 0 {
                if selected == 0 { filtered_len - 1 } else { selected - 1 }
            } else {
                selected
            }
        }
        _ => selected,
    }
}

// --- Fuzzy filtering ---

/// Compute filtered indices sorted by fuzzy match score.
/// `text_fn` extracts the searchable string from each item.
pub fn compute_filtered<T, F>(items: &[T], query: &str, text_fn: F) -> Vec<usize>
where
    F: Fn(&T) -> String,
{
    if query.is_empty() {
        return (0..items.len()).collect();
    }

    let matcher = SkimMatcherV2::default();
    let mut scored: Vec<(usize, i64)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| {
            matcher
                .fuzzy_match(&text_fn(item), query)
                .map(|score| (i, score))
        })
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1));
    scored.into_iter().map(|(i, _)| i).collect()
}

// --- Scrolling ---

/// Adjust scroll offset to keep the selected item visible.
pub fn adjust_scroll(selected: usize, scroll_offset: &mut usize, list_height: usize) {
    if selected < *scroll_offset {
        *scroll_offset = selected;
    }
    if selected >= *scroll_offset + list_height {
        *scroll_offset = selected - list_height + 1;
    }
}

// --- Rendering ---

/// Render the search bar widget shared across all vim-modal TUI apps.
pub fn render_search_bar(app_name: &str, query: &str, mode: Mode, placeholder: &str) -> Paragraph<'static> {
    let border_color = match mode {
        Mode::Insert => Color::Green,
        Mode::Normal => Color::Cyan,
    };

    let (text, style) = match mode {
        Mode::Insert if query.is_empty() => {
            (format!(" {placeholder}"), Style::default().fg(Color::DarkGray))
        }
        Mode::Insert => {
            (format!(" {query}\u{2588}"), Style::default().fg(Color::White))
        }
        Mode::Normal if query.is_empty() => {
            (" Press / to search".to_string(), Style::default().fg(Color::DarkGray))
        }
        Mode::Normal => {
            (format!(" {query}"), Style::default().fg(Color::White))
        }
    };

    let mode_label = match mode {
        Mode::Normal => format!(" {app_name} [NORMAL] "),
        Mode::Insert => format!(" {app_name} [INSERT] "),
    };

    Paragraph::new(text)
        .style(style)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(mode_label),
        )
}

// --- Help overlay ---

/// Render a centered help overlay listing keybindings.
/// `app_name` is shown in the title. `bindings` is a list of (key, description) pairs.
pub fn render_help_overlay(app_name: &str, bindings: &[(&str, &str)], area: Rect) -> (Paragraph<'static>, Rect) {
    let max_key_width = bindings.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    let max_desc_width = bindings.iter().map(|(_, d)| d.len()).max().unwrap_or(0);
    let inner_width = max_key_width + max_desc_width + 5; // padding + separator
    let inner_height = bindings.len() as u16 + 2; // +2 for top/bottom padding

    let popup_width = (inner_width as u16 + 4).min(area.width.saturating_sub(4));
    let popup_height = (inner_height + 2).min(area.height.saturating_sub(2)); // +2 for borders
    let popup_x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = area.y + (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    let lines: Vec<Line<'static>> = std::iter::once(Line::from(""))
        .chain(bindings.iter().map(|(key, desc)| {
            Line::from(vec![
                Span::styled(
                    format!("  {key:>width$}", width = max_key_width),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(desc.to_string(), Style::default().fg(Color::White)),
            ])
        }))
        .chain(std::iter::once(Line::from(Span::styled(
            "  Press any key to close",
            Style::default().fg(Color::DarkGray),
        ))))
        .collect();

    let widget = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(format!(" {app_name} — Keybindings "))
            .style(Style::default().bg(Color::Black)),
    );

    (widget, popup_area)
}

// --- Test helpers ---

/// Create a KeyEvent for testing. Convenience for `KeyEventKind::Press` with no modifiers.
pub fn make_test_key(code: KeyCode) -> KeyEvent {
    use crossterm::event::{KeyEventKind, KeyEventState};
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

/// Create a KeyEvent with modifiers for testing.
pub fn make_test_key_with_mods(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    use crossterm::event::{KeyEventKind, KeyEventState};
    KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode;

    // --- handle_key tests ---

    #[test]
    fn ctrl_c_always_quits() {
        let mut pending = None;
        assert_eq!(
            handle_key(make_test_key_with_mods(KeyCode::Char('c'), KeyModifiers::CONTROL), Mode::Normal, &mut pending, &[]),
            Some(NavAction::Quit)
        );
        assert_eq!(
            handle_key(make_test_key_with_mods(KeyCode::Char('c'), KeyModifiers::CONTROL), Mode::Insert, &mut pending, &[]),
            Some(NavAction::Quit)
        );
    }

    #[test]
    fn normal_q_quits() {
        let mut pending = None;
        assert_eq!(handle_key(make_test_key(KeyCode::Char('q')), Mode::Normal, &mut pending, &[]), Some(NavAction::Quit));
    }

    #[test]
    fn normal_esc_quits() {
        let mut pending = None;
        assert_eq!(handle_key(make_test_key(KeyCode::Esc), Mode::Normal, &mut pending, &[]), Some(NavAction::Quit));
    }

    #[test]
    fn normal_j_moves_down() {
        let mut pending = None;
        assert_eq!(handle_key(make_test_key(KeyCode::Char('j')), Mode::Normal, &mut pending, &[]), Some(NavAction::MoveDown));
    }

    #[test]
    fn normal_k_moves_up() {
        let mut pending = None;
        assert_eq!(handle_key(make_test_key(KeyCode::Char('k')), Mode::Normal, &mut pending, &[]), Some(NavAction::MoveUp));
    }

    #[test]
    fn normal_big_g_moves_to_bottom() {
        let mut pending = None;
        assert_eq!(handle_key(make_test_key(KeyCode::Char('G')), Mode::Normal, &mut pending, &[]), Some(NavAction::MoveToBottom));
    }

    #[test]
    fn normal_gg_moves_to_top() {
        let mut pending = None;
        let action = handle_key(make_test_key(KeyCode::Char('g')), Mode::Normal, &mut pending, &[]);
        assert_eq!(action, Some(NavAction::Noop));
        assert_eq!(pending, Some('g'));
        let action = handle_key(make_test_key(KeyCode::Char('g')), Mode::Normal, &mut pending, &[]);
        assert_eq!(action, Some(NavAction::MoveToTop));
        assert!(pending.is_none());
    }

    #[test]
    fn normal_g_then_other_is_noop() {
        let mut pending = None;
        handle_key(make_test_key(KeyCode::Char('g')), Mode::Normal, &mut pending, &[]);
        let action = handle_key(make_test_key(KeyCode::Char('x')), Mode::Normal, &mut pending, &[]);
        assert_eq!(action, Some(NavAction::Noop));
    }

    #[test]
    fn normal_n_next_match() {
        let mut pending = None;
        assert_eq!(handle_key(make_test_key(KeyCode::Char('n')), Mode::Normal, &mut pending, &[]), Some(NavAction::NextMatch));
    }

    #[test]
    fn normal_shift_n_prev_match() {
        let mut pending = None;
        assert_eq!(handle_key(make_test_key(KeyCode::Char('N')), Mode::Normal, &mut pending, &[]), Some(NavAction::PrevMatch));
    }

    #[test]
    fn normal_question_mark_shows_help() {
        let mut pending = None;
        assert_eq!(handle_key(make_test_key(KeyCode::Char('?')), Mode::Normal, &mut pending, &[]), Some(NavAction::ShowHelp));
    }

    #[test]
    fn normal_slash_enters_insert() {
        let mut pending = None;
        assert_eq!(handle_key(make_test_key(KeyCode::Char('/')), Mode::Normal, &mut pending, &[]), Some(NavAction::EnterInsert));
    }

    #[test]
    fn normal_ctrl_d_half_page_down() {
        let mut pending = None;
        assert_eq!(
            handle_key(make_test_key_with_mods(KeyCode::Char('d'), KeyModifiers::CONTROL), Mode::Normal, &mut pending, &[]),
            Some(NavAction::HalfPageDown)
        );
    }

    #[test]
    fn normal_ctrl_u_half_page_up() {
        let mut pending = None;
        assert_eq!(
            handle_key(make_test_key_with_mods(KeyCode::Char('u'), KeyModifiers::CONTROL), Mode::Normal, &mut pending, &[]),
            Some(NavAction::HalfPageUp)
        );
    }

    #[test]
    fn normal_enter_returns_none() {
        let mut pending = None;
        assert_eq!(handle_key(make_test_key(KeyCode::Enter), Mode::Normal, &mut pending, &[]), None);
    }

    #[test]
    fn insert_esc_exits() {
        let mut pending = None;
        assert_eq!(handle_key(make_test_key(KeyCode::Esc), Mode::Insert, &mut pending, &[]), Some(NavAction::ExitInsert));
    }

    #[test]
    fn insert_enter_returns_none() {
        let mut pending = None;
        assert_eq!(handle_key(make_test_key(KeyCode::Enter), Mode::Insert, &mut pending, &[]), None);
    }

    #[test]
    fn insert_char_types() {
        let mut pending = None;
        assert_eq!(handle_key(make_test_key(KeyCode::Char('a')), Mode::Insert, &mut pending, &[]), Some(NavAction::TypeChar('a')));
    }

    #[test]
    fn insert_backspace() {
        let mut pending = None;
        assert_eq!(handle_key(make_test_key(KeyCode::Backspace), Mode::Insert, &mut pending, &[]), Some(NavAction::Backspace));
    }

    #[test]
    fn insert_ctrl_u_clears() {
        let mut pending = None;
        assert_eq!(
            handle_key(make_test_key_with_mods(KeyCode::Char('u'), KeyModifiers::CONTROL), Mode::Insert, &mut pending, &[]),
            Some(NavAction::ClearSearch)
        );
    }

    #[test]
    fn insert_arrows_navigate() {
        let mut pending = None;
        assert_eq!(handle_key(make_test_key(KeyCode::Up), Mode::Insert, &mut pending, &[]), Some(NavAction::MoveUp));
        assert_eq!(handle_key(make_test_key(KeyCode::Down), Mode::Insert, &mut pending, &[]), Some(NavAction::MoveDown));
    }

    #[test]
    fn extra_pending_key_returns_none() {
        let mut pending = None;
        let action = handle_key(make_test_key(KeyCode::Char('d')), Mode::Normal, &mut pending, &['d']);
        assert_eq!(action, None);
        assert_eq!(pending, Some('d'));
    }

    #[test]
    fn extra_pending_combo_returns_none() {
        let mut pending = Some('d');
        let action = handle_key(make_test_key(KeyCode::Char('d')), Mode::Normal, &mut pending, &['d']);
        // 'd' is not 'g', so the combo ('d','d') falls through to the extra_pending branch
        assert_eq!(action, None);
    }

    // --- apply_navigation tests ---

    #[test]
    fn nav_move_up() {
        assert_eq!(apply_navigation(&NavAction::MoveUp, 2, 5, 10), 1);
    }

    #[test]
    fn nav_move_up_at_top() {
        assert_eq!(apply_navigation(&NavAction::MoveUp, 0, 5, 10), 0);
    }

    #[test]
    fn nav_move_down() {
        assert_eq!(apply_navigation(&NavAction::MoveDown, 1, 5, 10), 2);
    }

    #[test]
    fn nav_move_down_at_bottom() {
        assert_eq!(apply_navigation(&NavAction::MoveDown, 4, 5, 10), 4);
    }

    #[test]
    fn nav_move_to_top() {
        assert_eq!(apply_navigation(&NavAction::MoveToTop, 3, 5, 10), 0);
    }

    #[test]
    fn nav_move_to_bottom() {
        assert_eq!(apply_navigation(&NavAction::MoveToBottom, 0, 5, 10), 4);
    }

    #[test]
    fn nav_half_page_down() {
        assert_eq!(apply_navigation(&NavAction::HalfPageDown, 0, 5, 4), 2);
    }

    #[test]
    fn nav_half_page_up() {
        assert_eq!(apply_navigation(&NavAction::HalfPageUp, 4, 5, 4), 2);
    }

    #[test]
    fn nav_half_page_down_clamps() {
        assert_eq!(apply_navigation(&NavAction::HalfPageDown, 3, 5, 20), 4);
    }

    #[test]
    fn nav_next_match_wraps() {
        assert_eq!(apply_navigation(&NavAction::NextMatch, 4, 5, 10), 0);
    }

    #[test]
    fn nav_next_match_advances() {
        assert_eq!(apply_navigation(&NavAction::NextMatch, 2, 5, 10), 3);
    }

    #[test]
    fn nav_prev_match_wraps() {
        assert_eq!(apply_navigation(&NavAction::PrevMatch, 0, 5, 10), 4);
    }

    #[test]
    fn nav_prev_match_retreats() {
        assert_eq!(apply_navigation(&NavAction::PrevMatch, 3, 5, 10), 2);
    }

    #[test]
    fn nav_next_match_empty() {
        assert_eq!(apply_navigation(&NavAction::NextMatch, 0, 0, 10), 0);
    }

    #[test]
    fn nav_prev_match_empty() {
        assert_eq!(apply_navigation(&NavAction::PrevMatch, 0, 0, 10), 0);
    }

    #[test]
    fn nav_empty_list() {
        assert_eq!(apply_navigation(&NavAction::MoveDown, 0, 0, 10), 0);
        assert_eq!(apply_navigation(&NavAction::MoveToBottom, 0, 0, 10), 0);
        assert_eq!(apply_navigation(&NavAction::HalfPageDown, 0, 0, 10), 0);
    }

    // --- compute_filtered tests ---

    #[test]
    fn empty_query_returns_all() {
        let items = vec!["foo", "bar", "baz"];
        let filtered = compute_filtered(&items, "", |s| s.to_string());
        assert_eq!(filtered, vec![0, 1, 2]);
    }

    #[test]
    fn filter_matches() {
        let items = vec!["apple", "banana", "apricot"];
        let filtered = compute_filtered(&items, "ap", |s| s.to_string());
        assert!(!filtered.is_empty());
        assert!(filtered.contains(&0)); // apple
        assert!(filtered.contains(&2)); // apricot
    }

    #[test]
    fn filter_no_match() {
        let items = vec!["apple", "banana"];
        let filtered = compute_filtered(&items, "zzzzz", |s| s.to_string());
        assert!(filtered.is_empty());
    }

    // --- adjust_scroll tests ---

    #[test]
    fn scroll_follows_selection_down() {
        let mut offset = 0;
        adjust_scroll(5, &mut offset, 3);
        assert_eq!(offset, 3); // 5 - 3 + 1
    }

    #[test]
    fn scroll_follows_selection_up() {
        let mut offset = 5;
        adjust_scroll(2, &mut offset, 3);
        assert_eq!(offset, 2);
    }

    #[test]
    fn scroll_stays_when_visible() {
        let mut offset = 2;
        adjust_scroll(3, &mut offset, 5);
        assert_eq!(offset, 2); // 3 is within [2, 7)
    }
}
