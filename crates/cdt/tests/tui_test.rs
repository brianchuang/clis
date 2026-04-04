use cdt::scanner::Workspace;
use cdt::tui::{apply_action, compute_filtered, handle_key, Action, App, Mode};
use crossterm::event::{KeyCode, KeyModifiers};
use std::path::PathBuf;
use tui_core::NavAction;

fn make_key(code: KeyCode) -> crossterm::event::KeyEvent {
    tui_core::make_test_key(code)
}

fn make_key_with_mods(code: KeyCode, modifiers: KeyModifiers) -> crossterm::event::KeyEvent {
    tui_core::make_test_key_with_mods(code, modifiers)
}

fn test_workspaces() -> Vec<Workspace> {
    vec![
        Workspace {
            project: "black-pearl".into(),
            name: "memphis".into(),
            path: PathBuf::from("/ws/black-pearl/memphis"),
            merged: None,
            branch: None,
            last_commit: None,
            dirty: false,
            pr: None,
        },
        Workspace {
            project: "black-pearl".into(),
            name: "tokyo".into(),
            path: PathBuf::from("/ws/black-pearl/tokyo"),
            merged: None,
            branch: None,
            last_commit: None,
            dirty: false,
            pr: None,
        },
        Workspace {
            project: "black-pearl".into(),
            name: "warsaw".into(),
            path: PathBuf::from("/ws/black-pearl/warsaw"),
            merged: None,
            branch: None,
            last_commit: None,
            dirty: false,
            pr: None,
        },
        Workspace {
            project: "my-app".into(),
            name: "london".into(),
            path: PathBuf::from("/ws/my-app/london"),
            merged: None,
            branch: None,
            last_commit: None,
            dirty: false,
            pr: None,
        },
        Workspace {
            project: "my-app".into(),
            name: "paris".into(),
            path: PathBuf::from("/ws/my-app/paris"),
            merged: None,
            branch: None,
            last_commit: None,
            dirty: false,
            pr: None,
        },
    ]
}

fn test_app() -> App {
    let mut app = App::new(test_workspaces());
    app.list_height = 20; // simulate a tall terminal
    app
}

// --- App initialization ---

#[test]
fn app_starts_in_normal_mode() {
    let app = test_app();
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn app_starts_with_all_workspaces_visible() {
    let app = test_app();
    assert_eq!(app.filtered.len(), 5);
    assert_eq!(app.selected, 0);
}

#[test]
fn app_starts_with_empty_query() {
    let app = test_app();
    assert!(app.query.is_empty());
}

// --- Navigation ---

#[test]
fn move_down_increments_selected() {
    let mut app = test_app();
    app.mode = Mode::Normal;
    apply_action(&mut app, Action::Nav(NavAction::MoveDown));
    assert_eq!(app.selected, 1);
}

#[test]
fn move_down_stops_at_bottom() {
    let mut app = test_app();
    app.mode = Mode::Normal;
    app.selected = 4; // last item
    apply_action(&mut app, Action::Nav(NavAction::MoveDown));
    assert_eq!(app.selected, 4);
}

#[test]
fn move_up_decrements_selected() {
    let mut app = test_app();
    app.selected = 2;
    apply_action(&mut app, Action::Nav(NavAction::MoveUp));
    assert_eq!(app.selected, 1);
}

#[test]
fn move_up_stops_at_top() {
    let mut app = test_app();
    app.selected = 0;
    apply_action(&mut app, Action::Nav(NavAction::MoveUp));
    assert_eq!(app.selected, 0);
}

#[test]
fn move_to_top() {
    let mut app = test_app();
    app.selected = 3;
    apply_action(&mut app, Action::Nav(NavAction::MoveToTop));
    assert_eq!(app.selected, 0);
}

#[test]
fn move_to_bottom() {
    let mut app = test_app();
    apply_action(&mut app, Action::Nav(NavAction::MoveToBottom));
    assert_eq!(app.selected, 4);
}

#[test]
fn half_page_down() {
    let mut app = test_app();
    app.list_height = 4;
    app.selected = 0;
    apply_action(&mut app, Action::Nav(NavAction::HalfPageDown));
    assert_eq!(app.selected, 2); // half of 4
}

#[test]
fn half_page_up() {
    let mut app = test_app();
    app.list_height = 4;
    app.selected = 4;
    apply_action(&mut app, Action::Nav(NavAction::HalfPageUp));
    assert_eq!(app.selected, 2);
}

#[test]
fn half_page_down_clamps_to_end() {
    let mut app = test_app();
    app.list_height = 20;
    app.selected = 3;
    apply_action(&mut app, Action::Nav(NavAction::HalfPageDown));
    assert_eq!(app.selected, 4); // clamped to last
}

// --- Selection ---

#[test]
fn select_sets_chosen_path_and_quits() {
    let mut app = test_app();
    app.selected = 1; // tokyo
    apply_action(&mut app, Action::Select);
    assert!(app.should_quit);
    assert_eq!(app.chosen, Some(PathBuf::from("/ws/black-pearl/tokyo")));
}

#[test]
fn quit_sets_no_chosen() {
    let mut app = test_app();
    apply_action(&mut app, Action::Nav(NavAction::Quit));
    assert!(app.should_quit);
    assert!(app.chosen.is_none());
}

#[test]
fn selected_workspace_returns_correct_entry() {
    let app = test_app();
    let ws = app.selected_workspace().unwrap();
    assert_eq!(ws.name, "memphis");
}

// --- Fuzzy filtering ---

#[test]
fn empty_query_returns_all() {
    let ws = test_workspaces();
    let filtered = compute_filtered(&ws, "");
    assert_eq!(filtered.len(), 5);
    assert_eq!(filtered, vec![0, 1, 2, 3, 4]);
}

#[test]
fn filter_by_workspace_name() {
    let ws = test_workspaces();
    let filtered = compute_filtered(&ws, "tokyo");
    assert!(!filtered.is_empty());
    assert_eq!(ws[filtered[0]].name, "tokyo");
}

#[test]
fn filter_by_project_name() {
    let ws = test_workspaces();
    let filtered = compute_filtered(&ws, "my-app");
    assert!(filtered.len() >= 2);
    for &i in &filtered {
        assert_eq!(ws[i].project, "my-app");
    }
}

#[test]
fn filter_by_combined_label() {
    let ws = test_workspaces();
    let filtered = compute_filtered(&ws, "pearl/mem");
    assert!(!filtered.is_empty());
    assert_eq!(ws[filtered[0]].label(), "black-pearl/memphis");
}

#[test]
fn filter_no_match_returns_empty() {
    let ws = test_workspaces();
    let filtered = compute_filtered(&ws, "zzzznotfound");
    assert!(filtered.is_empty());
}

#[test]
fn typing_updates_filter() {
    let mut app = test_app();
    apply_action(&mut app, Action::Nav(NavAction::TypeChar('t')));
    apply_action(&mut app, Action::Nav(NavAction::TypeChar('o')));
    apply_action(&mut app, Action::Nav(NavAction::TypeChar('k')));
    assert_eq!(app.query, "tok");
    assert!(!app.filtered.is_empty());
    let ws = app.selected_workspace().unwrap();
    assert_eq!(ws.name, "tokyo");
}

#[test]
fn backspace_removes_last_char() {
    let mut app = test_app();
    apply_action(&mut app, Action::Nav(NavAction::TypeChar('t')));
    apply_action(&mut app, Action::Nav(NavAction::TypeChar('o')));
    apply_action(&mut app, Action::Nav(NavAction::Backspace));
    assert_eq!(app.query, "t");
}

#[test]
fn backspace_on_empty_query_is_noop() {
    let mut app = test_app();
    apply_action(&mut app, Action::Nav(NavAction::Backspace));
    assert!(app.query.is_empty());
    assert_eq!(app.filtered.len(), 5);
}

#[test]
fn clear_search_resets_query_and_filter() {
    let mut app = test_app();
    apply_action(&mut app, Action::Nav(NavAction::TypeChar('x')));
    apply_action(&mut app, Action::Nav(NavAction::TypeChar('x')));
    apply_action(&mut app, Action::Nav(NavAction::ClearSearch));
    assert!(app.query.is_empty());
    assert_eq!(app.filtered.len(), 5);
    assert_eq!(app.selected, 0);
}

#[test]
fn typing_resets_selection_to_top() {
    let mut app = test_app();
    app.selected = 3;
    apply_action(&mut app, Action::Nav(NavAction::TypeChar('p')));
    assert_eq!(app.selected, 0);
}

// --- Mode switching ---

#[test]
fn enter_insert_switches_mode() {
    let mut app = test_app();
    app.mode = Mode::Normal;
    apply_action(&mut app, Action::Nav(NavAction::EnterInsert));
    assert_eq!(app.mode, Mode::Insert);
}

#[test]
fn exit_insert_switches_to_normal() {
    let mut app = test_app();
    app.mode = Mode::Insert;
    apply_action(&mut app, Action::Nav(NavAction::ExitInsert));
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn exit_insert_clears_pending_key() {
    let mut app = test_app();
    app.pending_key = Some('g');
    apply_action(&mut app, Action::Nav(NavAction::ExitInsert));
    assert!(app.pending_key.is_none());
}

// --- Key handling: Normal mode ---

#[test]
fn normal_q_quits() {
    let mut pending = None;
    let action = handle_key(make_key(KeyCode::Char('q')), Mode::Normal, &mut pending);
    assert_eq!(action, Action::Nav(NavAction::Quit));
}

#[test]
fn normal_esc_quits() {
    let mut pending = None;
    let action = handle_key(make_key(KeyCode::Esc), Mode::Normal, &mut pending);
    assert_eq!(action, Action::Nav(NavAction::Quit));
}

#[test]
fn normal_j_moves_down() {
    let mut pending = None;
    let action = handle_key(make_key(KeyCode::Char('j')), Mode::Normal, &mut pending);
    assert_eq!(action, Action::Nav(NavAction::MoveDown));
}

#[test]
fn normal_k_moves_up() {
    let mut pending = None;
    let action = handle_key(make_key(KeyCode::Char('k')), Mode::Normal, &mut pending);
    assert_eq!(action, Action::Nav(NavAction::MoveUp));
}

#[test]
fn normal_enter_selects() {
    let mut pending = None;
    let action = handle_key(make_key(KeyCode::Enter), Mode::Normal, &mut pending);
    assert_eq!(action, Action::Select);
}

#[test]
fn normal_slash_enters_insert() {
    let mut pending = None;
    let action = handle_key(make_key(KeyCode::Char('/')), Mode::Normal, &mut pending);
    assert_eq!(action, Action::Nav(NavAction::EnterInsert));
}

#[test]
fn normal_big_g_moves_to_bottom() {
    let mut pending = None;
    let action = handle_key(make_key(KeyCode::Char('G')), Mode::Normal, &mut pending);
    assert_eq!(action, Action::Nav(NavAction::MoveToBottom));
}

#[test]
fn normal_gg_moves_to_top() {
    let mut pending = None;
    // First g sets pending
    let action = handle_key(make_key(KeyCode::Char('g')), Mode::Normal, &mut pending);
    assert_eq!(action, Action::Nav(NavAction::Noop));
    assert_eq!(pending, Some('g'));
    // Second g triggers MoveToTop
    let action = handle_key(make_key(KeyCode::Char('g')), Mode::Normal, &mut pending);
    assert_eq!(action, Action::Nav(NavAction::MoveToTop));
    assert!(pending.is_none());
}

#[test]
fn normal_g_then_other_key_is_noop() {
    let mut pending = None;
    handle_key(make_key(KeyCode::Char('g')), Mode::Normal, &mut pending);
    let action = handle_key(make_key(KeyCode::Char('x')), Mode::Normal, &mut pending);
    assert_eq!(action, Action::Nav(NavAction::Noop));
    assert!(pending.is_none());
}

#[test]
fn normal_ctrl_d_half_page_down() {
    let mut pending = None;
    let action = handle_key(
        make_key_with_mods(KeyCode::Char('d'), KeyModifiers::CONTROL),
        Mode::Normal,
        &mut pending,
    );
    assert_eq!(action, Action::Nav(NavAction::HalfPageDown));
}

#[test]
fn normal_ctrl_u_half_page_up() {
    let mut pending = None;
    let action = handle_key(
        make_key_with_mods(KeyCode::Char('u'), KeyModifiers::CONTROL),
        Mode::Normal,
        &mut pending,
    );
    assert_eq!(action, Action::Nav(NavAction::HalfPageUp));
}

#[test]
fn ctrl_c_always_quits() {
    let mut pending = None;

    // From Normal
    let action = handle_key(
        make_key_with_mods(KeyCode::Char('c'), KeyModifiers::CONTROL),
        Mode::Normal,
        &mut pending,
    );
    assert_eq!(action, Action::Nav(NavAction::Quit));

    // From Insert
    let action = handle_key(
        make_key_with_mods(KeyCode::Char('c'), KeyModifiers::CONTROL),
        Mode::Insert,
        &mut pending,
    );
    assert_eq!(action, Action::Nav(NavAction::Quit));
}

// --- Key handling: Insert mode ---

#[test]
fn insert_esc_exits() {
    let mut pending = None;
    let action = handle_key(make_key(KeyCode::Esc), Mode::Insert, &mut pending);
    assert_eq!(action, Action::Nav(NavAction::ExitInsert));
}

#[test]
fn insert_enter_selects() {
    let mut pending = None;
    let action = handle_key(make_key(KeyCode::Enter), Mode::Insert, &mut pending);
    assert_eq!(action, Action::Select);
}

#[test]
fn insert_char_types() {
    let mut pending = None;
    let action = handle_key(make_key(KeyCode::Char('a')), Mode::Insert, &mut pending);
    assert_eq!(action, Action::Nav(NavAction::TypeChar('a')));
}

#[test]
fn insert_backspace() {
    let mut pending = None;
    let action = handle_key(make_key(KeyCode::Backspace), Mode::Insert, &mut pending);
    assert_eq!(action, Action::Nav(NavAction::Backspace));
}

#[test]
fn insert_ctrl_u_clears() {
    let mut pending = None;
    let action = handle_key(
        make_key_with_mods(KeyCode::Char('u'), KeyModifiers::CONTROL),
        Mode::Insert,
        &mut pending,
    );
    assert_eq!(action, Action::Nav(NavAction::ClearSearch));
}

#[test]
fn insert_arrows_navigate() {
    let mut pending = None;
    assert_eq!(
        handle_key(make_key(KeyCode::Up), Mode::Insert, &mut pending),
        Action::Nav(NavAction::MoveUp)
    );
    assert_eq!(
        handle_key(make_key(KeyCode::Down), Mode::Insert, &mut pending),
        Action::Nav(NavAction::MoveDown)
    );
}

// --- Edge cases ---

#[test]
fn select_with_empty_filter_still_quits() {
    let mut app = test_app();
    // Force filter to empty
    app.query = "zzzzzz".into();
    app.refilter();
    assert!(app.filtered.is_empty());

    apply_action(&mut app, Action::Select);
    assert!(app.should_quit);
    assert!(app.chosen.is_none()); // nothing to select
}

#[test]
fn navigation_with_empty_filter_is_safe() {
    let mut app = test_app();
    app.query = "zzzzzz".into();
    app.refilter();

    // None of these should panic
    apply_action(&mut app, Action::Nav(NavAction::MoveUp));
    apply_action(&mut app, Action::Nav(NavAction::MoveDown));
    apply_action(&mut app, Action::Nav(NavAction::MoveToTop));
    apply_action(&mut app, Action::Nav(NavAction::MoveToBottom));
    apply_action(&mut app, Action::Nav(NavAction::HalfPageUp));
    apply_action(&mut app, Action::Nav(NavAction::HalfPageDown));
    assert_eq!(app.selected, 0);
}

#[test]
fn clamp_selection_after_filter_narrows() {
    let mut app = test_app();
    app.selected = 4; // last of 5
                      // Type something that matches fewer items
    apply_action(&mut app, Action::Nav(NavAction::TypeChar('t')));
    apply_action(&mut app, Action::Nav(NavAction::TypeChar('o')));
    apply_action(&mut app, Action::Nav(NavAction::TypeChar('k')));
    // selected should be reset to 0, not left at 4
    assert_eq!(app.selected, 0);
    assert!(app.filtered.len() < 5);
}
