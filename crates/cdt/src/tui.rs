use crate::scanner::{self, Workspace};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::io::{stderr, Stderr};
use std::path::{Path, PathBuf};

pub use tui_core::Mode;

// --- Actions ---

#[derive(Debug, PartialEq)]
pub enum Action {
    Nav(tui_core::NavAction),
    Select,
}

// --- State ---

pub struct App {
    pub workspaces: Vec<Workspace>,
    pub filtered: Vec<usize>,
    pub query: String,
    pub selected: usize,
    pub scroll_offset: usize,
    pub should_quit: bool,
    pub chosen: Option<PathBuf>,
    pub mode: Mode,
    pub pending_key: Option<char>,
    pub list_height: usize,
    pub show_help: bool,
}

impl App {
    pub fn new(workspaces: Vec<Workspace>) -> Self {
        let filtered = tui_core::compute_filtered(&workspaces, "", |ws| ws.label());
        App {
            workspaces,
            filtered,
            query: String::new(),
            selected: 0,
            scroll_offset: 0,
            should_quit: false,
            chosen: None,
            mode: Mode::Normal,
            pending_key: None,
            list_height: 0,
            show_help: false,
        }
    }

    pub fn refilter(&mut self) {
        self.filtered = tui_core::compute_filtered(&self.workspaces, &self.query, |ws| ws.label());
        self.clamp_selection();
    }

    fn clamp_selection(&mut self) {
        if self.filtered.is_empty() {
            self.selected = 0;
            self.scroll_offset = 0;
        } else {
            self.selected = self.selected.min(self.filtered.len() - 1);
        }
    }

    pub fn selected_workspace(&self) -> Option<&Workspace> {
        self.filtered
            .get(self.selected)
            .map(|&i| &self.workspaces[i])
    }
}

// --- Key handling ---

pub fn handle_key(
    key: crossterm::event::KeyEvent,
    mode: Mode,
    pending: &mut Option<char>,
) -> Action {
    match tui_core::handle_key(key, mode, pending, &[]) {
        Some(nav) => Action::Nav(nav),
        None => Action::Select, // Enter is the only app-specific key for cdt
    }
}

pub fn apply_action(app: &mut App, action: Action) {
    match action {
        Action::Nav(nav) => match nav {
            tui_core::NavAction::Noop => {}
            tui_core::NavAction::Quit => app.should_quit = true,
            tui_core::NavAction::ShowHelp => app.show_help = !app.show_help,
            tui_core::NavAction::EnterInsert => app.mode = Mode::Insert,
            tui_core::NavAction::ExitInsert => {
                app.mode = Mode::Normal;
                app.pending_key = None;
            }
            tui_core::NavAction::TypeChar(c) => {
                app.query.push(c);
                app.refilter();
                app.selected = 0;
            }
            tui_core::NavAction::Backspace => {
                app.query.pop();
                app.refilter();
                app.selected = 0;
            }
            tui_core::NavAction::ClearSearch => {
                app.query.clear();
                app.refilter();
                app.selected = 0;
            }
            ref nav_action @ (tui_core::NavAction::MoveUp
            | tui_core::NavAction::MoveDown
            | tui_core::NavAction::MoveToTop
            | tui_core::NavAction::MoveToBottom
            | tui_core::NavAction::HalfPageUp
            | tui_core::NavAction::HalfPageDown
            | tui_core::NavAction::NextMatch
            | tui_core::NavAction::PrevMatch) => {
                app.selected = tui_core::apply_navigation(
                    nav_action,
                    app.selected,
                    app.filtered.len(),
                    app.list_height,
                );
            }
        },
        Action::Select => {
            if let Some(ws) = app.selected_workspace() {
                app.chosen = Some(ws.path.clone());
            }
            app.should_quit = true;
        }
    }
}

pub fn compute_filtered(workspaces: &[Workspace], query: &str) -> Vec<usize> {
    tui_core::compute_filtered(workspaces, query, |ws| ws.label())
}

// --- Main loop ---
// We render to stderr so stdout stays clean for the selected path.

pub fn run(
    root: &Path,
    no_cache: bool,
    time: bool,
) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    use crate::cache;
    use std::time::Instant;

    let t0 = Instant::now();

    // Try cache first for instant startup
    let workspaces = if !no_cache {
        if let Some(cached) = cache::load(root) {
            if time {
                eprintln!("[cdt] TUI cache hit — loaded in {:.1?}", t0.elapsed());
            }
            cached
        } else {
            let ws = scanner::scan(root)?;
            if time {
                eprintln!(
                    "[cdt] TUI fresh scan — {} workspaces in {:.1?}",
                    ws.len(),
                    t0.elapsed()
                );
            }
            cache::save(root, &ws);
            ws
        }
    } else {
        let ws = scanner::scan(root)?;
        if time {
            eprintln!(
                "[cdt] TUI scan (no-cache) — {} workspaces in {:.1?}",
                ws.len(),
                t0.elapsed()
            );
        }
        ws
    };

    if workspaces.is_empty() {
        return Err(format!("No workspaces found in {}", root.display()).into());
    }

    let mut app = App::new(workspaces);

    enable_raw_mode()?;
    stderr().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stderr()))?;

    let result = event_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    stderr().execute(LeaveAlternateScreen)?;

    result.map(|_| app.chosen)
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stderr>>,
    app: &mut App,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        terminal.draw(|f| render(f, app))?;

        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
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

// --- Rendering ---

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
        tui_core::render_search_bar(
            "cdt",
            &app.query,
            app.mode,
            "Type to filter workspaces\u{2026}",
        ),
        chunks[0],
    );

    let list_height = chunks[1].height as usize;
    app.list_height = list_height;
    tui_core::adjust_scroll(app.selected, &mut app.scroll_offset, list_height);
    f.render_widget(
        render_workspace_list(
            &app.workspaces,
            &app.filtered,
            app.selected,
            app.scroll_offset,
            list_height,
        ),
        chunks[1],
    );

    f.render_widget(
        render_status_bar(
            app.filtered.len(),
            app.workspaces.len(),
            app.mode,
            app.selected_workspace(),
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
            ("/", "Search"),
            ("Enter", "Select workspace"),
            ("Esc / q", "Quit"),
            ("?", "Toggle this help"),
        ];
        let (widget, area) = tui_core::render_help_overlay("cdt", bindings, f.area());
        f.render_widget(Clear, area);
        f.render_widget(widget, area);
    }
}

fn render_workspace_list<'a>(
    workspaces: &'a [Workspace],
    filtered: &[usize],
    selected: usize,
    scroll_offset: usize,
    list_height: usize,
) -> List<'a> {
    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(list_height)
        .map(|(i, &ws_idx)| render_list_item(&workspaces[ws_idx], i == selected))
        .collect();

    List::new(items).block(Block::default().borders(Borders::NONE))
}

fn render_list_item(ws: &Workspace, is_selected: bool) -> ListItem<'static> {
    let d = ws.display_columns();

    let style = if is_selected {
        Style::default().bg(Color::DarkGray).fg(Color::White)
    } else {
        Style::default()
    };

    let indicator_color = match ws.merged {
        Some(true) => Color::Green,
        Some(false) => Color::Yellow,
        None => Color::DarkGray,
    };
    let indicator = match ws.merged {
        Some(true) => "\u{2713}",
        Some(false) => "\u{25cf}",
        None => " ",
    };
    let project_color = if is_selected {
        Color::Cyan
    } else {
        Color::Blue
    };
    let name_color = if is_selected {
        Color::White
    } else {
        Color::Green
    };
    let branch_color = if is_selected {
        Color::White
    } else {
        Color::Magenta
    };
    let age_color = if is_selected {
        Color::Gray
    } else {
        Color::DarkGray
    };
    let dirty_color = Color::Red;

    let mut spans = vec![
        Span::styled(
            format!(" {indicator} "),
            style.patch(Style::default().fg(indicator_color)),
        ),
        Span::styled(
            format!("{:<16}", d.project),
            style.patch(Style::default().fg(project_color)),
        ),
        Span::styled(
            format!("{:<16}", d.name),
            style.patch(Style::default().fg(name_color)),
        ),
        Span::styled(
            format!("{:<24}", d.branch),
            style.patch(Style::default().fg(branch_color)),
        ),
        Span::styled(
            format!("{:>8}", d.age),
            style.patch(Style::default().fg(age_color)),
        ),
    ];
    if d.dirty {
        spans.push(Span::styled(
            " \u{2717}",
            style.patch(Style::default().fg(dirty_color)),
        ));
    }

    ListItem::new(Line::from(spans))
}

fn render_status_bar(
    count: usize,
    total: usize,
    mode: Mode,
    selected: Option<&Workspace>,
) -> Paragraph<'static> {
    let path_hint = selected
        .map(|ws| ws.path.display().to_string())
        .unwrap_or_default();

    let help = match mode {
        Mode::Normal => format!(" {count}/{total} \u{2502} j/k move \u{2502} Enter select \u{2502} / search \u{2502} ? help \u{2502} q quit  {path_hint}"),
        Mode::Insert => format!(" {count}/{total} \u{2502} type to filter \u{2502} Enter select \u{2502} Esc normal  {path_hint}"),
    };

    Paragraph::new(help).style(Style::default().bg(Color::DarkGray).fg(Color::White))
}
