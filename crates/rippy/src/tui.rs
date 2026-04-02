use crate::clipboard;
use crate::db::{ClipEntry, Store};
use crate::watcher::Watcher;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::io::stdout;
use std::path::Path;
use std::time::Instant;
use tui_core::{Mode, NavAction};

// --- Actions (Elm-style message type) ---

enum Action {
    Nav(NavAction),
    CopyAndQuit,
    DeleteSelected,
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
        }
    }

    fn refresh(&mut self) {
        let prev_id = self.selected_entry().map(|e| e.id);
        self.entries = self.store.all().unwrap_or_default();
        self.filtered = tui_core::compute_filtered(&self.entries, &self.query, |e| e.content.clone());
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
        self.filtered = tui_core::compute_filtered(&self.entries, &self.query, |e| e.content.clone());
        self.clamp_selection();
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

    match tui_core::handle_key(key, mode, pending, &['d']) {
        Some(nav) => Action::Nav(nav),
        None => Action::CopyAndQuit, // Enter is CopyAndQuit for rippy
    }
}

fn apply_action(app: &mut App, action: Action) {
    match action {
        Action::Nav(nav) => {
            match nav {
                NavAction::Noop => {}
                NavAction::Quit => app.should_quit = true,
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
                | NavAction::HalfPageDown) => {
                    app.selected = tui_core::apply_navigation(
                        nav_action,
                        app.selected,
                        app.filtered.len(),
                        app.list_height,
                    );
                }
            }
        }
        Action::CopyAndQuit => {
            if let Some(entry) = app.selected_entry() {
                clipboard::set_clipboard(&entry.content);
                app.copied_id = Some(entry.id);
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
    }
}

// --- Main loop ---

pub fn run(db_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let watcher = Watcher::spawn(db_path);

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
                let action = handle_key(key, app.mode, &mut app.pending_key);
                apply_action(app, action);
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

    f.render_widget(tui_core::render_search_bar("rippy", &app.query, app.mode, "Type to search\u{2026}"), chunks[0]);

    let list_height = chunks[1].height as usize;
    app.list_height = list_height;
    tui_core::adjust_scroll(app.selected, &mut app.scroll_offset, list_height);
    f.render_widget(
        render_clip_list(&app.entries, &app.filtered, app.selected, app.scroll_offset, list_height, app.copied_id),
        chunks[1],
    );

    f.render_widget(
        render_status_bar(app.filtered.len(), app.entries.len(), app.copied_id, app.mode),
        chunks[2],
    );
}

fn render_clip_list<'a>(
    entries: &'a [ClipEntry],
    filtered: &[usize],
    selected: usize,
    scroll_offset: usize,
    list_height: usize,
    copied_id: Option<i64>,
) -> List<'a> {
    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(list_height)
        .map(|(i, &entry_idx)| render_list_item(&entries[entry_idx], i == selected, copied_id))
        .collect();

    List::new(items).block(Block::default().borders(Borders::NONE))
}

fn render_list_item(entry: &ClipEntry, is_selected: bool, copied_id: Option<i64>) -> ListItem<'_> {
    let preview: String = entry.content.lines().next().unwrap_or("").chars().take(200).collect();
    let time = entry.timestamp.format("%m/%d %H:%M");

    let style = match (is_selected, Some(entry.id) == copied_id) {
        (true, _) => Style::default().bg(Color::DarkGray).fg(Color::White),
        (_, true) => Style::default().fg(Color::Green),
        _ => Style::default(),
    };

    let time_color = if is_selected { Color::Cyan } else { Color::DarkGray };

    ListItem::new(Line::from(vec![
        Span::styled(format!(" {time} "), style.patch(Style::default().fg(time_color))),
        Span::styled(format!("\u{2502} {preview}"), style),
    ]))
}

fn render_status_bar(count: usize, total: usize, copied_id: Option<i64>, mode: Mode) -> Paragraph<'static> {
    let (text, style) = if copied_id.is_some() {
        (" Copied! ".to_string(), Style::default().bg(Color::Green).fg(Color::Black))
    } else {
        let help = match mode {
            Mode::Normal => format!(" {count}/{total} \u{2502} j/k move \u{2502} Enter copy \u{2502} dd delete \u{2502} / search \u{2502} q quit"),
            Mode::Insert => format!(" {count}/{total} \u{2502} type to filter \u{2502} Enter copy \u{2502} Esc normal mode"),
        };
        (help, Style::default().bg(Color::DarkGray).fg(Color::White))
    };

    Paragraph::new(text).style(style)
}
