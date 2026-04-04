use crate::clipboard;
use crate::config::Config;
use crate::db::{ClipEntry, Store};
use crate::highlight;
use crate::tag;
use crate::watcher::Watcher;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::collections::HashSet;
use std::io::stdout;
use std::path::Path;
use std::time::Instant;
use tui_core::{Mode, NavAction};

// --- Actions (Elm-style message type) ---

enum Action {
    Nav(NavAction),
    CopyAndQuit,
    DeleteSelected,
    TogglePinSelected,
    TogglePreview,
    ToggleMultiSelect,
    ScrollPreviewDown,
    ScrollPreviewUp,
}

// --- State ---

struct App {
    store: Store,
    entries: Vec<ClipEntry>,
    filtered: Vec<usize>,
    query: String,
    selected: usize,
    scroll_offset: usize,
    should_quit: bool,
    copied_id: Option<i64>,
    mode: Mode,
    pending_key: Option<char>,
    list_height: usize,
    show_help: bool,
    show_preview: bool,
    preview_scroll: usize,
    multi_selected: HashSet<i64>,
}

impl App {
    fn new(store: Store) -> Self {
        let entries = store.all().unwrap_or_default();
        let filtered = tui_core::compute_filtered(&entries, "", |e| e.content.clone());
        App {
            store,
            entries,
            filtered,
            query: String::new(),
            selected: 0,
            scroll_offset: 0,
            should_quit: false,
            copied_id: None,
            mode: Mode::Normal,
            pending_key: None,
            list_height: 0,
            show_help: false,
            show_preview: true,
            preview_scroll: 0,
            multi_selected: HashSet::new(),
        }
    }

    fn refresh(&mut self) {
        let prev_id = self.selected_entry().map(|e| e.id);
        self.entries = self.store.all().unwrap_or_default();
        self.filtered =
            tui_core::compute_filtered(&self.entries, &self.query, |e| e.content.clone());
        // Restore selection to the same entry by ID, falling back to clamp
        if let Some(id) = prev_id {
            if let Some(pos) = self.filtered.iter().position(|&i| self.entries[i].id == id) {
                self.selected = pos;
            } else {
                self.clamp_selection();
            }
        } else {
            self.clamp_selection();
        }
    }

    fn refilter(&mut self) {
        self.filtered =
            tui_core::compute_filtered(&self.entries, &self.query, |e| e.content.clone());
        self.clamp_selection();
        self.multi_selected.clear();
    }

    fn reset_selection(&mut self) {
        self.selected = 0;
        self.scroll_offset = 0;
    }

    fn clamp_selection(&mut self) {
        if self.filtered.is_empty() {
            self.selected = 0;
            self.scroll_offset = 0;
        } else {
            self.selected = self.selected.min(self.filtered.len() - 1);
        }
    }

    fn selected_entry(&self) -> Option<&ClipEntry> {
        self.filtered.get(self.selected).map(|&i| &self.entries[i])
    }
}

// --- Key handling ---

fn handle_key(key: crossterm::event::KeyEvent, mode: Mode, pending: &mut Option<char>) -> Action {
    // Handle app-specific pending combo: dd -> DeleteSelected
    if let Some('d') = *pending {
        pending.take();
        if key.code == KeyCode::Char('d') {
            return Action::DeleteSelected;
        }
        // Invalid combo after 'd', treat as noop
        return Action::Nav(NavAction::Noop);
    }

    // Normal mode: s toggles pin, p toggles preview, Ctrl+j/k scrolls preview
    if mode == Mode::Normal {
        if let KeyCode::Char('s') = key.code {
            if key.modifiers.is_empty() {
                return Action::TogglePinSelected;
            }
        }
        if let KeyCode::Char('p') = key.code {
            if key.modifiers.is_empty() {
                return Action::TogglePreview;
            }
        }
        if key.code == KeyCode::Char(' ') && key.modifiers.is_empty() {
            return Action::ToggleMultiSelect;
        }
        if key.modifiers == crossterm::event::KeyModifiers::CONTROL {
            match key.code {
                KeyCode::Char('j') => return Action::ScrollPreviewDown,
                KeyCode::Char('k') => return Action::ScrollPreviewUp,
                _ => {}
            }
        }
    }

    match tui_core::handle_key(key, mode, pending, &['d']) {
        Some(nav) => Action::Nav(nav),
        None => Action::CopyAndQuit, // Enter is CopyAndQuit for rippy
    }
}

fn apply_action(app: &mut App, action: Action) {
    let prev_selected = app.selected;
    match action {
        Action::Nav(nav) => match nav {
            NavAction::Noop => {}
            NavAction::Quit => app.should_quit = true,
            NavAction::ShowHelp => app.show_help = !app.show_help,
            NavAction::EnterInsert => app.mode = Mode::Insert,
            NavAction::ExitInsert => {
                app.mode = Mode::Normal;
                app.pending_key = None;
            }
            NavAction::TypeChar(c) => {
                app.query.push(c);
                app.refilter();
                app.reset_selection();
            }
            NavAction::Backspace => {
                app.query.pop();
                app.refilter();
                app.reset_selection();
            }
            NavAction::ClearSearch => {
                app.query.clear();
                app.refilter();
                app.reset_selection();
            }
            ref nav_action @ (NavAction::MoveUp
            | NavAction::MoveDown
            | NavAction::MoveToTop
            | NavAction::MoveToBottom
            | NavAction::HalfPageUp
            | NavAction::HalfPageDown
            | NavAction::NextMatch
            | NavAction::PrevMatch) => {
                app.selected = tui_core::apply_navigation(
                    nav_action,
                    app.selected,
                    app.filtered.len(),
                    app.list_height,
                );
            }
        },
        Action::CopyAndQuit => {
            if app.multi_selected.is_empty() {
                // Single copy: just the current entry
                if let Some(entry) = app.selected_entry() {
                    clipboard::set_clipboard(&entry.content);
                    app.copied_id = Some(entry.id);
                }
            } else {
                // Batch copy: concatenate selected entries in list order
                let combined: Vec<&str> = app
                    .filtered
                    .iter()
                    .map(|&i| &app.entries[i])
                    .filter(|e| app.multi_selected.contains(&e.id))
                    .map(|e| e.content.as_str())
                    .collect();
                clipboard::set_clipboard(&combined.join("\n"));
            }
            app.should_quit = true;
        }
        Action::DeleteSelected => {
            if let Some(entry) = app.selected_entry() {
                let id = entry.id;
                app.store.delete(id).ok();
                app.refresh();
            }
        }
        Action::TogglePinSelected => {
            if let Some(entry) = app.selected_entry() {
                let id = entry.id;
                app.store.toggle_pin(id).ok();
                app.refresh();
            }
        }
        Action::ToggleMultiSelect => {
            if let Some(entry) = app.selected_entry() {
                let id = entry.id;
                if !app.multi_selected.remove(&id) {
                    app.multi_selected.insert(id);
                }
            }
        }
        Action::TogglePreview => {
            app.show_preview = !app.show_preview;
            app.preview_scroll = 0;
        }
        Action::ScrollPreviewDown => {
            app.preview_scroll = app.preview_scroll.saturating_add(3);
        }
        Action::ScrollPreviewUp => {
            app.preview_scroll = app.preview_scroll.saturating_sub(3);
        }
    }
    // Reset preview scroll when selection changes
    if app.selected != prev_selected {
        app.preview_scroll = 0;
    }
}

// --- Main loop ---

pub fn run(db_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = db_path.parent().unwrap_or(Path::new("."));
    let cfg = Config::load(data_dir);
    let watcher = Watcher::spawn(
        db_path,
        cfg.history.max_entries,
        cfg.history.auto_expire_seconds,
    );

    let store = Store::open(db_path)?;
    let mut app = App::new(store);

    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let result = event_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    watcher.stop();
    result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut last_refresh = Instant::now();

    loop {
        // Refresh entries from DB every second to pick up watcher inserts
        if last_refresh.elapsed() >= std::time::Duration::from_secs(1) {
            app.refresh();
            last_refresh = Instant::now();
        }

        terminal.draw(|f| render(f, app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                app.copied_id = None;
                if app.show_help {
                    app.show_help = false;
                } else {
                    let action = handle_key(key, app.mode, &mut app.pending_key);
                    apply_action(app, action);
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

// --- Rendering (pure view functions) ---

fn render(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(f.area());

    f.render_widget(
        tui_core::render_search_bar("rippy", &app.query, app.mode, "Type to search\u{2026}"),
        chunks[0],
    );

    let content_area = chunks[1];
    let list_area = if app.show_preview {
        let halves = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(content_area);

        render_preview(f, app, halves[1]);
        halves[0]
    } else {
        content_area
    };

    let list_height = list_area.height as usize;
    app.list_height = list_height;
    tui_core::adjust_scroll(app.selected, &mut app.scroll_offset, list_height);
    f.render_widget(
        render_clip_list(
            &app.entries,
            &app.filtered,
            app.selected,
            app.scroll_offset,
            list_height,
            app.copied_id,
            &app.multi_selected,
        ),
        list_area,
    );

    f.render_widget(
        render_status_bar(
            app.filtered.len(),
            app.entries.len(),
            app.copied_id,
            app.mode,
            app.show_preview,
            app.multi_selected.len(),
        ),
        chunks[2],
    );

    if app.show_help {
        let bindings: &[(&str, &str)] = &[
            ("j / k", "Move down / up"),
            ("n / N", "Next / previous (wrapping)"),
            ("g g", "Go to top"),
            ("G", "Go to bottom"),
            ("Ctrl-d / Ctrl-u", "Half-page down / up"),
            ("Space", "Toggle multi-select"),
            ("s", "Toggle pin (starred)"),
            ("p", "Toggle preview pane"),
            ("Ctrl-j / Ctrl-k", "Scroll preview down / up"),
            ("/", "Search"),
            ("Enter", "Copy and quit"),
            ("d d", "Delete entry"),
            ("Esc / q", "Quit"),
            ("?", "Toggle this help"),
        ];
        let (widget, area) = tui_core::render_help_overlay("rippy", bindings, f.area());
        f.render_widget(Clear, area);
        f.render_widget(widget, area);
    }
}

fn render_clip_list<'a>(
    entries: &'a [ClipEntry],
    filtered: &[usize],
    selected: usize,
    scroll_offset: usize,
    list_height: usize,
    copied_id: Option<i64>,
    multi_selected: &HashSet<i64>,
) -> List<'a> {
    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(list_height)
        .map(|(i, &entry_idx)| {
            let is_multi = multi_selected.contains(&entries[entry_idx].id);
            render_list_item(&entries[entry_idx], i == selected, copied_id, is_multi)
        })
        .collect();

    List::new(items).block(Block::default().borders(Borders::NONE))
}

fn render_list_item(
    entry: &ClipEntry,
    is_selected: bool,
    copied_id: Option<i64>,
    is_multi_selected: bool,
) -> ListItem<'_> {
    let preview: String = entry
        .content
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(200)
        .collect();
    let time = entry.timestamp.format("%m/%d %H:%M");
    let pin = if entry.pinned { "★ " } else { "  " };
    let content_tag = tag::detect(&entry.content);

    let style = match (is_selected, Some(entry.id) == copied_id, is_multi_selected) {
        (true, _, _) => Style::default().bg(Color::DarkGray).fg(Color::White),
        (_, true, _) => Style::default().fg(Color::Green),
        (_, _, true) => Style::default().fg(Color::Cyan),
        _ => Style::default(),
    };

    let check = if is_multi_selected { "● " } else { "  " };
    let check_color = if is_multi_selected {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let time_color = if is_selected {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let pin_color = if is_selected {
        Color::Yellow
    } else {
        Color::DarkGray
    };
    let tag_color = tag_color(content_tag, is_selected);

    ListItem::new(Line::from(vec![
        Span::styled(check, style.patch(Style::default().fg(check_color))),
        Span::styled(pin, style.patch(Style::default().fg(pin_color))),
        Span::styled(
            format!("{time} "),
            style.patch(Style::default().fg(time_color)),
        ),
        Span::styled(
            format!("{:<4} ", content_tag.label()),
            style.patch(Style::default().fg(tag_color)),
        ),
        Span::styled(format!("\u{2502} {preview}"), style),
    ]))
}

fn tag_color(tag: tag::ContentTag, is_selected: bool) -> Color {
    if is_selected {
        match tag {
            tag::ContentTag::Url => Color::Blue,
            tag::ContentTag::Path => Color::Yellow,
            tag::ContentTag::Code => Color::Green,
            tag::ContentTag::Text => Color::White,
        }
    } else {
        match tag {
            tag::ContentTag::Url => Color::Blue,
            tag::ContentTag::Path => Color::Yellow,
            tag::ContentTag::Code => Color::Green,
            tag::ContentTag::Text => Color::DarkGray,
        }
    }
}

fn render_preview(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Preview ")
        .title_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    let inner = block.inner(area);
    f.render_widget(block, area);

    let entry = match app.selected_entry() {
        Some(e) => e,
        None => return,
    };

    let preview_height = inner.height as usize;
    let is_code = tag::detect(&entry.content) == tag::ContentTag::Code;
    let lines: Vec<Line> = if is_code {
        highlight::highlight_content(&entry.content)
    } else {
        entry
            .content
            .lines()
            .enumerate()
            .map(|(i, line)| {
                let line_num = Span::styled(
                    format!("{:>4} ", i + 1),
                    Style::default().fg(Color::DarkGray),
                );
                let text = Span::raw(line.to_string());
                Line::from(vec![line_num, text])
            })
            .collect()
    };

    // Clamp preview scroll to content bounds
    let max_scroll = lines.len().saturating_sub(preview_height);
    let scroll = app.preview_scroll.min(max_scroll);

    let paragraph = Paragraph::new(lines)
        .scroll((scroll as u16, 0))
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, inner);
}

fn render_status_bar(
    count: usize,
    total: usize,
    copied_id: Option<i64>,
    mode: Mode,
    show_preview: bool,
    multi_count: usize,
) -> Paragraph<'static> {
    let (text, style) = if copied_id.is_some() {
        (
            " Copied! ".to_string(),
            Style::default().bg(Color::Green).fg(Color::Black),
        )
    } else {
        let help = match mode {
            Mode::Normal => {
                let count_str = if multi_count > 0 {
                    format!(" {count}/{total} ({multi_count} selected)")
                } else {
                    format!(" {count}/{total}")
                };
                let mut parts: Vec<&str> = vec![
                    &count_str,
                    "j/k move",
                    "Space select",
                    "Enter copy",
                    "s pin",
                    "dd delete",
                    "p preview",
                ];
                if show_preview {
                    parts.push("C-j/k scroll");
                }
                parts.extend_from_slice(&["/ search", "? help", "q quit"]);
                parts.join(" \u{2502} ")
            }
            Mode::Insert => format!(" {count}/{total} \u{2502} type to filter \u{2502} Enter copy \u{2502} Esc normal mode"),
        };
        (help, Style::default().bg(Color::DarkGray).fg(Color::White))
    };

    Paragraph::new(text).style(style)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn test_app(contents: &[&str]) -> App {
        let store = Store::open(Path::new(":memory:")).unwrap();
        for c in contents {
            store.insert(c, None).unwrap();
        }
        App::new(store)
    }

    #[test]
    fn toggle_multi_select_adds_and_removes() {
        let mut app = test_app(&["aaa", "bbb", "ccc"]);
        assert!(app.multi_selected.is_empty());

        // Select first entry
        apply_action(&mut app, Action::ToggleMultiSelect);
        assert_eq!(app.multi_selected.len(), 1);
        let first_id = app.selected_entry().unwrap().id;
        assert!(app.multi_selected.contains(&first_id));

        // Toggle again to deselect
        apply_action(&mut app, Action::ToggleMultiSelect);
        assert!(app.multi_selected.is_empty());
    }

    #[test]
    fn multi_select_multiple_entries() {
        let mut app = test_app(&["aaa", "bbb", "ccc"]);

        // Select first
        apply_action(&mut app, Action::ToggleMultiSelect);
        // Move down and select second
        apply_action(&mut app, Action::Nav(NavAction::MoveDown));
        apply_action(&mut app, Action::ToggleMultiSelect);

        assert_eq!(app.multi_selected.len(), 2);
    }

    #[test]
    fn refilter_clears_multi_select() {
        let mut app = test_app(&["aaa", "bbb", "ccc"]);
        apply_action(&mut app, Action::ToggleMultiSelect);
        assert_eq!(app.multi_selected.len(), 1);

        // Typing a character triggers refilter, which clears selections
        apply_action(&mut app, Action::Nav(NavAction::EnterInsert));
        apply_action(&mut app, Action::Nav(NavAction::TypeChar('a')));
        assert!(app.multi_selected.is_empty());
    }

    #[test]
    fn multi_select_empty_list_is_noop() {
        let mut app = test_app(&[]);
        apply_action(&mut app, Action::ToggleMultiSelect);
        assert!(app.multi_selected.is_empty());
    }
}
