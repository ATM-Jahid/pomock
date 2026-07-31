use std::time::Duration;

use ratatui::{
    Terminal,
    backend::TestBackend,
    style::{Color, Modifier, Style},
    text::Line,
};

use crate::{
    app::{Action, ClickTarget, Direction, ScrollTarget, SettingsMoveDirection},
    config::{KeyAction, ThemeColor, ThemeConfig, ThemeRole},
    settings::SettingField,
    timer::SessionKind,
};

use super::*;

use super::footer::{
    FooterMetrics, confirmation_prompt, footer_text_for_focus, help_item_width, text_height,
    viable_help_height,
};
use super::layout::{C_H_SUG, ClockFace, WorkspaceMode, clock_geometry};
use super::settings::{
    setting_row, settings_field_row, settings_footer, settings_group_start, settings_offset,
    settings_parts, settings_scroll_anchor, settings_visual_row, theme_role_label,
};
use super::task_lists::{task_label, task_row_at};
use super::theme::session_button_style;
use crate::{
    app::{ConfirmationOperation, TimerChange},
    config::ConfigKey,
};

fn add_task(app: &mut App, description: &str) {
    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let _ = app.dispatch(Action::BeginAdd);
    for character in description.chars() {
        let _ = app.dispatch(Action::PushInput(character));
    }
    let _ = app.dispatch(Action::SubmitEdit);
    let _ = app.dispatch(Action::NavigateFocus(Direction::Up));
}

fn app_layout(area: Rect, app: &App) -> FrameGeometry {
    let workspace_width = inner_width(area);
    let footer = stable_footer_metrics(app.input_keys(), workspace_width);

    resolve(LayoutRequest {
        area,
        footer_heights: footer.heights,
        footer_cutoff: footer.cutoff,
        focus: app.ui_focus(),
        last_task_focus: app.last_task_focus(),
        duration: app.timer().remaining(),
    })
}

#[test]
fn clock_legend_describes_cycle_session_control() {
    let app = App::new();
    let keys = KeysConfig::default();

    let help = footer_text(&app, &keys);

    assert!(help.contains("[H/J/K/L] box nav"));
    assert!(help.contains("[c] cycle session"));
    assert!(help.contains("[s] settings"));
    assert!(!help.contains("F2"));
    assert!(!help.contains("[n] next"));
}

#[test]
fn task_legend_shows_only_the_first_default_list_keys() {
    let mut app = App::new();
    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));

    let help = footer_text(&app, &KeysConfig::default());

    assert!(help.contains("[j/k] list nav"));
    assert!(!help.contains('↓'));
    assert!(!help.contains('↑'));
    assert!(help.contains("[u/d] move list item"));
}

#[test]
fn help_wraps_to_the_available_width_without_losing_controls() {
    let app = App::new();
    let help = footer_text(&app, &KeysConfig::default());

    let wrapped = wrap_help(&help, 28);

    assert!(wrapped.lines().all(|line| Line::from(line).width() <= 28));
    assert_eq!(
        wrapped.split_whitespace().collect::<Vec<_>>(),
        help.split_whitespace().collect::<Vec<_>>()
    );
    assert!(text_height(&wrapped) > 1);
}

#[test]
fn help_wraps_between_complete_key_action_items() {
    let app = App::new();
    let help = footer_text(&app, &KeysConfig::default());

    let wrapped = wrap_help(&help, 18);

    assert!(wrapped.lines().any(|line| line.contains("[q] quit")));
    assert!(wrapped.lines().any(|line| line.contains("[s] settings")));
    assert!(!wrapped.lines().any(|line| line.ends_with("[q]")));
}

#[test]
fn focus_help_variants_share_the_maximum_height_at_the_cutoff() {
    let app = App::new();
    let cutoff = stable_footer_metrics(app.input_keys(), u16::MAX).cutoff;
    let heights = stable_footer_metrics(app.input_keys(), cutoff).heights;
    let reserve = heights.reserve().unwrap();

    assert_eq!(
        reserve,
        heights
            .clock
            .unwrap()
            .max(heights.todo.unwrap())
            .max(heights.done.unwrap())
    );

    let area = Rect::new(
        0,
        0,
        cutoff.saturating_add(2),
        C_H_SUG
            .saturating_mul(2)
            .saturating_add(reserve)
            .saturating_add(2),
    );
    assert_eq!(app_layout(area, &app).footer().height, reserve);
}

#[test]
fn complete_help_item_viability_has_an_explicit_width_boundary() {
    let app = App::new();
    let text = footer_text_for_focus(&app, app.input_keys(), UiFocus::Clock);
    let item_width = help_item_width(&text);

    assert!(item_width > 0);
    assert_eq!(
        viable_help_height(&text, item_width.saturating_sub(1)),
        None
    );
    assert!(viable_help_height(&text, item_width).is_some());
}

#[test]
fn help_item_width_measures_the_complete_key_action_unit() {
    let text = "[x] a long action  [q] quit";

    assert_eq!(
        help_item_width(text),
        u16::try_from(Line::from("[x] a long action").width()).unwrap()
    );
    assert_eq!(viable_help_height(text, 8), None);
}

#[test]
fn responsive_layout_selects_each_space_class() {
    let app = App::new();
    for (area, expected) in [
        (Rect::new(0, 0, 80, 24), WorkspaceMode::Full),
        (Rect::new(0, 0, 80, 23), WorkspaceMode::Short),
        (Rect::new(0, 0, 40, 26), WorkspaceMode::Narrow),
        (Rect::new(0, 0, 40, 25), WorkspaceMode::Mono),
        (Rect::new(0, 0, 20, 9), WorkspaceMode::Mono),
    ] {
        assert_eq!(app_layout(area, &app).mode(), expected, "area: {area:?}");
    }
}

#[test]
fn responsive_mode_is_stable_when_focus_changes_the_help_height() {
    for (area, expected) in [
        (Rect::new(0, 0, 80, 24), WorkspaceMode::Full),
        (Rect::new(0, 0, 80, 23), WorkspaceMode::Short),
        (Rect::new(0, 0, 40, 26), WorkspaceMode::Narrow),
        (Rect::new(0, 0, 40, 25), WorkspaceMode::Mono),
        (Rect::new(0, 0, 20, 9), WorkspaceMode::Mono),
    ] {
        let mut app = App::new();
        assert_eq!(app_layout(area, &app).mode(), expected);

        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        assert_eq!(app_layout(area, &app).mode(), expected);

        let _ = app.dispatch(Action::NavigateFocus(Direction::Right));
        assert_eq!(app_layout(area, &app).mode(), expected);

        let _ = app.dispatch(Action::NavigateFocus(Direction::Up));
        assert_eq!(app_layout(area, &app).mode(), expected);
    }
}

#[test]
fn focus_changes_preserve_footer_and_workspace_rectangles_across_help_heights() {
    let area = Rect::new(0, 0, 16, 34);
    let mut app = App::new();
    let clock_focus = app_layout(area, &app);

    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let todo_focus = app_layout(area, &app);
    let _ = app.dispatch(Action::NavigateFocus(Direction::Right));
    let done_focus = app_layout(area, &app);

    assert_eq!(clock_focus.mode(), WorkspaceMode::Narrow);
    assert_eq!(clock_focus.footer(), todo_focus.footer());
    assert_eq!(todo_focus.footer(), done_focus.footer());
    assert_eq!(
        clock_focus.clock().unwrap().area,
        todo_focus.clock().unwrap().area
    );
    assert_eq!(
        todo_focus.clock().unwrap().area,
        done_focus.clock().unwrap().area
    );
    assert_eq!(clock_focus.todo().unwrap(), todo_focus.todo().unwrap());
    assert_eq!(todo_focus.todo().unwrap(), done_focus.done().unwrap());
}

#[test]
fn add_task_preserves_footer_and_workspace_rectangles() {
    let area = Rect::new(0, 0, 80, 30);
    let mut app = App::new();
    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let normal = app_layout(area, &app);

    let _ = app.dispatch(Action::BeginAdd);
    let adding = app_layout(area, &app);

    assert_eq!(normal.footer(), adding.footer());
    assert_eq!(normal.clock().unwrap().area, adding.clock().unwrap().area);
    assert_eq!(normal.todo(), adding.todo());
    assert_eq!(normal.done(), adding.done());
}

#[test]
fn confirmation_preserves_footer_and_workspace_rectangles() {
    let area = Rect::new(0, 0, 80, 30);
    let mut app = App::new();
    let normal = app_layout(area, &app);

    let _ = app.dispatch(Action::PrimaryAction);
    let _ = app.tick(Duration::from_secs(10));
    let _ = app.dispatch(Action::CycleSession);
    let confirming = app_layout(area, &app);

    assert!(app.is_confirmation_open());
    assert_eq!(normal.footer(), confirming.footer());
    assert_eq!(
        normal.clock().unwrap().area,
        confirming.clock().unwrap().area
    );
    assert_eq!(normal.todo(), confirming.todo());
    assert_eq!(normal.done(), confirming.done());
}

#[test]
fn shorter_current_help_leaves_the_rest_of_the_stable_footer_blank() {
    let mut app = App::new();
    let theme = Theme::from(&ThemeConfig::default());
    let keys = KeysConfig::default();
    let metrics = stable_footer_metrics(&keys, u16::MAX);
    let width = metrics.cutoff;
    let heights = stable_footer_metrics(&keys, width).heights;
    let reserve = heights.reserve().unwrap();
    let current_height = heights.clock.unwrap();

    assert!(current_height < reserve);

    let area = Rect::new(
        0,
        0,
        width.saturating_add(2),
        C_H_SUG
            .saturating_mul(2)
            .saturating_add(reserve)
            .saturating_add(2),
    );
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();

    let layout = terminal
        .draw(|frame| {
            draw(frame, &mut app, theme, &keys);
        })
        .unwrap();
    let footer = app_layout(layout.area, &app).footer();

    assert_eq!(footer.height, reserve);
    for y in footer.y.saturating_add(current_height)..footer.bottom() {
        for x in footer.x..footer.right() {
            assert_eq!(terminal.backend().buffer()[(x, y)].symbol(), " ");
        }
    }
}

#[test]
fn wide_height_progression_is_full_then_short_then_footerless() {
    let app = App::new();
    let full = app_layout(Rect::new(0, 0, 50, 26), &app);
    let short_with_help = app_layout(Rect::new(0, 0, 50, 25), &app);
    let short_without_help = app_layout(Rect::new(0, 0, 50, 15), &app);
    let below_clock_minimum = app_layout(Rect::new(0, 0, 50, 11), &app);

    assert_eq!(
        (full.mode(), full.footer().height),
        (WorkspaceMode::Full, 4)
    );
    assert_eq!(
        (short_with_help.mode(), short_with_help.footer().height),
        (WorkspaceMode::Short, 4)
    );
    assert_eq!(short_without_help.mode(), WorkspaceMode::Short);
    assert_eq!(short_without_help.footer().height, 0);
    assert_eq!(below_clock_minimum.mode(), WorkspaceMode::Short);
    assert_eq!(below_clock_minimum.footer().height, 0);
    assert_eq!(below_clock_minimum.clock().unwrap().face, ClockFace::Text);
}

#[test]
fn short_layout_switches_between_clock_and_the_task_split() {
    let area = Rect::new(0, 0, 80, 10);
    let mut app = App::new();

    let clock = app_layout(area, &app);
    assert_eq!(clock.mode(), WorkspaceMode::Short);
    assert!(clock.clock().is_some());
    assert!(clock.todo().is_none());
    assert!(clock.done().is_none());

    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let tasks = app_layout(area, &app);
    assert_eq!(tasks.mode(), WorkspaceMode::Short);
    assert!(tasks.clock().is_none());
    assert!(tasks.todo().is_some());
    assert!(tasks.done().is_some());
}

#[test]
fn hit_testing_uses_the_layout_of_the_last_rendered_frame() {
    let area = Rect::new(0, 0, 80, 10);
    let mut app = App::new();
    let rendered = app_layout(area, &app);
    let clock = rendered.clock().unwrap().area;

    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    assert!(app_layout(area, &app).clock().is_none());
    assert_eq!(
        click_target(&rendered, (clock.x + 1, clock.y + 1), &app),
        ClickTarget::Clock
    );
}

#[test]
fn narrow_layout_retains_the_last_task_panel_while_clock_is_focused() {
    let area = Rect::new(0, 0, 40, 26);
    let mut app = App::new();
    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let _ = app.dispatch(Action::NavigateFocus(Direction::Right));

    let done = app_layout(area, &app);
    assert_eq!(done.mode(), WorkspaceMode::Narrow);
    assert!(done.todo().is_none());
    assert!(done.done().is_some());

    let _ = app.dispatch(Action::NavigateFocus(Direction::Up));
    let clock = app_layout(area, &app);
    assert!(clock.clock().is_some());
    assert!(clock.todo().is_none());
    assert!(clock.done().is_some());

    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let todo = app_layout(area, &app);
    assert_eq!(app.ui_focus(), UiFocus::Todo);
    assert!(todo.todo().is_some());
    assert!(todo.done().is_none());
}

#[test]
fn mono_layout_shows_only_the_focused_panel() {
    let area = Rect::new(0, 0, 40, 10);
    let mut app = App::new();

    let clock = app_layout(area, &app);
    assert_eq!(clock.mode(), WorkspaceMode::Mono);
    assert!(clock.clock().is_some());
    assert!(clock.todo().is_none());

    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let todo = app_layout(area, &app);
    assert!(todo.clock().is_none());
    assert!(todo.todo().is_some());
    assert!(todo.done().is_none());

    let _ = app.dispatch(Action::NavigateFocus(Direction::Right));
    let done = app_layout(area, &app);
    assert!(done.todo().is_none());
    assert!(done.done().is_some());
}

#[test]
fn smallest_mono_clock_exposes_only_its_boxed_hit_target() {
    let app = App::new();
    let area = Rect::new(0, 0, 20, 9);
    let layout = app_layout(area, &app);

    assert_eq!(layout.mode(), WorkspaceMode::Mono);
    assert!(layout.clock().is_some());
    assert_eq!(click_target(&layout, (10, 5), &app), ClickTarget::Clock);
    assert_eq!(scroll_target(&layout, (10, 5), &app), None);
}

#[test]
fn short_text_clock_shows_complete_help_when_it_fits() {
    let mut app = App::new();
    let area = Rect::new(0, 0, 80, 14);
    let help = wrap_help(&footer_text(&app, app.input_keys()), inner_width(area));
    let layout = app_layout(area, &app);

    assert_eq!(layout.mode(), WorkspaceMode::Short);
    assert_eq!(layout.footer().height, text_height(&help));
    assert!(layout.footer().height > 0);
    assert!(layout.clock().unwrap().remaining.height > 0);

    let theme = Theme::from(&ThemeConfig::default());
    let keys = KeysConfig::default();
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal
        .draw(|frame| {
            draw(frame, &mut app, theme, &keys);
        })
        .unwrap();
    let rendered = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(rendered.contains("box nav"));
}

#[test]
fn one_row_mono_clock_omits_help_instead_of_displacing_the_timer() {
    let app = App::new();
    let area = Rect::new(0, 0, 30, 5);
    let layout = app_layout(area, &app);

    assert_eq!(layout.mode(), WorkspaceMode::Mono);
    assert_eq!(layout.footer().height, 0);
    assert_eq!(layout.clock().unwrap().remaining.height, 1);
}

#[test]
fn mono_text_clock_omits_help_instead_of_cutting_it_down() {
    let app = App::new();
    let area = Rect::new(0, 0, 20, 10);
    let help = wrap_help(&footer_text(&app, app.input_keys()), inner_width(area));
    let layout = app_layout(area, &app);

    assert_eq!(layout.mode(), WorkspaceMode::Mono);
    assert_eq!(layout.clock().unwrap().face, ClockFace::Text);
    assert!(text_height(&help) > 2);
    assert_eq!(layout.footer().height, 0);
}

#[test]
fn smallest_mono_clock_renders_text_duration_instead_of_big_glyphs() {
    let mut app = App::new();
    let area = Rect::new(0, 0, 20, 9);
    let theme = Theme::from(&ThemeConfig::default());
    let keys = KeysConfig::default();
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();

    terminal
        .draw(|frame| {
            draw(frame, &mut app, theme, &keys);
        })
        .unwrap();

    let rendered = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(rendered.contains("25:00"));
    assert!(rendered.contains("Clock"));
    assert!(!rendered.contains('█'));

    let clock = app_layout(area, &app).clock().unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(clock.area.x, clock.area.y)].symbol(), "┌");
    let duration_x = clock.remaining.x + (clock.remaining.width - 5) / 2;
    assert_eq!(buffer[(duration_x, clock.remaining.y)].fg, theme.focus);
}

#[test]
fn full_clock_falls_back_to_text_without_removing_other_rows() {
    let app = App::new();
    let layout = app_layout(Rect::new(0, 0, 9, 10), &app);
    let clock = layout.clock().unwrap();

    assert_eq!(layout.mode(), WorkspaceMode::Mono);
    assert_eq!(clock.face, ClockFace::Text);
    assert_eq!(clock.state.height, 1);
    assert_eq!(clock.remaining.height, 1);
    assert_eq!(clock.completed_sessions.height, 1);
    assert!(clock.session_controls.iter().all(|area| area.height == 1));
}

#[test]
fn mono_actions_are_available_only_for_the_rendered_panel() {
    let mut app = App::new();
    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let mono = app_layout(Rect::new(0, 0, 20, 9), &app);

    for action in [
        Action::BeginAdd,
        Action::EditSelected,
        Action::DeleteSelected,
        Action::PrimaryAction,
        Action::MoveSelection(Direction::Down),
        Action::MoveSelectedTask(Direction::Up),
    ] {
        assert!(action_target_visible(&mono, UiFocus::Todo, &action));
    }
    assert!(action_target_visible(
        &mono,
        UiFocus::Todo,
        &Action::NavigateFocus(Direction::Up)
    ));
    assert!(!action_target_visible(
        &mono,
        UiFocus::Clock,
        &Action::PrimaryAction
    ));
}

#[test]
fn clock_content_is_centered_with_equal_internal_gaps() {
    let layout = clock_geometry(Rect::new(0, 0, 80, 18), Duration::from_secs(25 * 60));

    assert_eq!(layout.remaining.height, 5);
    assert_eq!(layout.remaining.y, layout.state.y + layout.state.height + 1);
    assert_eq!(
        layout.completed_sessions.y,
        layout.remaining.y + layout.remaining.height + 1
    );

    let top_padding = layout.state.y - 1;
    let bottom_padding = layout.session_controls[0].y
        - (layout.completed_sessions.y + layout.completed_sessions.height);
    assert_eq!(top_padding, bottom_padding);
}

#[test]
fn mono_clock_removes_internal_gaps_before_squeezing_content() {
    let layout = clock_geometry(Rect::new(0, 0, 80, 10), Duration::from_secs(25 * 60));

    assert_eq!(layout.remaining.height, 5);
    assert_eq!(layout.remaining.y, layout.state.y + layout.state.height);
    assert_eq!(
        layout.completed_sessions.y,
        layout.remaining.y + layout.remaining.height
    );
    assert_eq!(layout.session_controls[0].height, 1);
    assert_eq!(
        layout.session_controls[0].y,
        layout.completed_sessions.y + layout.completed_sessions.height
    );
}

#[test]
fn roomy_clock_scales_glyphs_to_available_width_and_height() {
    let layout = clock_geometry(Rect::new(0, 0, 80, 19), Duration::from_secs(25 * 60));

    assert_eq!(layout.face, ClockFace::Glyphs { scale: 2 });
    assert_eq!(layout.remaining.height, 10);
}

#[test]
fn clock_scaling_accounts_for_additional_minute_glyphs() {
    let layout = clock_geometry(Rect::new(0, 0, 80, 19), Duration::from_secs(9999 * 60 + 59));

    assert_eq!(layout.face, ClockFace::Glyphs { scale: 1 });
    assert_eq!(layout.remaining.height, 5);
}

#[test]
fn clock_does_not_scale_when_only_one_dimension_has_room() {
    let duration = Duration::from_secs(25 * 60);
    let wide_but_short = clock_geometry(Rect::new(0, 0, 100, 12), duration);
    let tall_but_narrow = clock_geometry(Rect::new(0, 0, 50, 24), duration);

    assert_eq!(wide_but_short.face, ClockFace::Glyphs { scale: 1 });
    assert_eq!(tall_but_narrow.face, ClockFace::Glyphs { scale: 1 });
}

#[test]
fn completed_focus_count_uses_normal_terminal_text_color() {
    let mut app = App::new();
    let keys = KeysConfig::default();
    let theme = Theme::from(&ThemeConfig::default());
    let area = Rect::new(0, 0, 80, 24);
    let completed_area = app_layout(area, &app).clock().unwrap().completed_sessions;
    let text = "Focus sessions completed: 0";
    let text_x = completed_area.x + (completed_area.width - text.len() as u16) / 2;
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();

    terminal
        .draw(|frame| {
            draw(frame, &mut app, theme, &keys);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    for x in text_x..text_x + text.len() as u16 {
        assert_eq!(buffer[(x, completed_area.y)].fg, Color::Reset);
    }
}

#[test]
fn configured_colors_map_to_their_semantic_theme_roles() {
    let config = ThemeConfig::new(
        ThemeColor::LightBlue,
        ThemeColor::Black,
        ThemeColor::LightYellow,
        ThemeColor::LightGreen,
    )
    .with_color(ThemeRole::Focus, ThemeColor::Red)
    .with_color(ThemeRole::ShortBreak, ThemeColor::Blue)
    .with_color(ThemeRole::LongBreak, ThemeColor::Magenta);

    let theme = Theme::from(&config);

    assert_eq!(theme.focused_border, Color::LightBlue);
    assert_eq!(theme.unfocused_border, Color::Black);
    assert_eq!(theme.todo_highlight, Color::LightYellow);
    assert_eq!(theme.done_highlight, Color::LightGreen);
    assert_eq!(theme.session(SessionKind::Focus), Color::Red);
    assert_eq!(theme.session(SessionKind::ShortBreak), Color::Blue);
    assert_eq!(theme.session(SessionKind::LongBreak), Color::Magenta);
}

#[test]
fn only_the_current_session_button_uses_its_session_color() {
    let config = ThemeConfig::default().with_color(ThemeRole::Focus, ThemeColor::Red);
    let theme = Theme::from(&config);

    let current = session_button_style(SessionKind::Focus, SessionKind::Focus, theme);
    let inactive = session_button_style(SessionKind::ShortBreak, SessionKind::Focus, theme);

    assert_eq!(current.fg, Some(Color::Red));
    assert!(current.add_modifier.contains(Modifier::REVERSED));
    assert_eq!(inactive, Style::default());
}

#[test]
fn rgb_colors_map_to_terminal_rgb_colors() {
    let config = ThemeConfig::default()
        .with_color(ThemeRole::FocusedBorder, ThemeColor::Rgb(0x5f, 0xd7, 0xff));

    assert_eq!(
        Theme::from(&config).focused_border,
        Color::Rgb(0x5f, 0xd7, 0xff)
    );
}

#[test]
fn normal_mode_help_uses_configured_key_labels() {
    let app = App::new();
    let keys: KeysConfig = toml::from_str(
        "focus_left = \"left\"\nclock_primary = \"backspace\"\ncycle_session = \"n\"\n",
    )
    .unwrap();

    let help = footer_text(&app, &keys);

    assert!(help.contains("[←/J/K/L] box nav"));
    assert!(help.contains("[Backspace] start/pause"));
    assert!(help.contains("[n] cycle session"));
    assert!(!help.contains("[c] cycle session"));
}

#[test]
fn task_help_uses_configured_item_movement_keys() {
    let mut app = App::new();
    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let keys: KeysConfig =
        toml::from_str("move_task_up = \"w\"\nmove_task_down = \"z\"\n").unwrap();

    let help = footer_text(&app, &keys);

    assert!(help.contains("[w/z] move list item"));
    assert!(!help.contains("[u/d] move list item"));
}

#[test]
fn normal_mode_help_uses_the_configured_settings_key() {
    let app = App::new();
    let keys: KeysConfig = toml::from_str("settings = \"t\"\n").unwrap();

    let help = footer_text(&app, &keys);

    assert!(help.contains("[t] settings"));
    assert!(!help.contains("[s] settings"));
}

#[test]
fn normal_mode_help_uses_only_the_first_key_for_each_action() {
    let app = App::new();
    let keys: KeysConfig =
        toml::from_str("clock_primary = [\"enter\", \"space\"]\ncycle_session = [\"n\", \"c\"]\n")
            .unwrap();

    let help = footer_text(&app, &keys);

    assert!(help.contains("[Enter] start/pause"));
    assert!(help.contains("[n] cycle session"));
    assert!(!help.contains("space"));
    assert!(!help.contains("[c] cycle session"));
}

#[test]
fn cycle_confirmation_describes_progress_loss() {
    let mut app = App::new();
    let _ = app.dispatch(Action::PrimaryAction);
    let _ = app.tick(Duration::from_secs(10));
    let _ = app.dispatch(Action::CycleSession);

    assert_eq!(
        footer_text(&app, &KeysConfig::default()),
        "Discard progress and cycle session?  [y/Enter] confirm  [n/Esc] cancel"
    );
}

#[test]
fn quit_confirmation_describes_progress_loss() {
    let mut app = App::new();
    let _ = app.dispatch(Action::PrimaryAction);
    let _ = app.tick(Duration::from_secs(10));
    let _ = app.dispatch(Action::Quit);

    assert_eq!(
        footer_text(&app, &KeysConfig::default()),
        "Quit and discard progress?  [y/Enter] confirm  [n/Esc] cancel"
    );
}

#[test]
fn session_change_confirmation_distinguishes_select_from_start() {
    assert_eq!(
        confirmation_prompt(ConfirmationOperation::TimerChange(
            TimerChange::SelectSession(SessionKind::ShortBreak),
        )),
        "Discard progress and change to Short break?"
    );
    assert_eq!(
        confirmation_prompt(ConfirmationOperation::TimerChange(
            TimerChange::StartSession(SessionKind::ShortBreak),
        )),
        "Discard progress, change to Short break, and start it?"
    );
}

#[test]
fn task_hit_testing_ignores_borders_empty_space_and_empty_lists() {
    let area = Rect::new(10, 5, 12, 5);

    assert_eq!(task_row_at((10, 6), area, 0, 3), None);
    assert_eq!(task_row_at((11, 5), area, 0, 3), None);
    assert_eq!(task_row_at((11, 6), area, 0, 0), None);
    assert_eq!(task_row_at((11, 9), area, 0, 3), None);
}

#[test]
fn task_hit_testing_maps_visible_rows_through_the_scroll_offset() {
    let area = Rect::new(10, 5, 12, 5);

    assert_eq!(task_row_at((11, 6), area, 4, 8), Some(4));
    assert_eq!(task_row_at((20, 8), area, 4, 8), Some(6));
}

#[test]
fn task_labels_can_show_or_hide_one_based_numbers() {
    assert_eq!(task_label(0, "First", true), "1. First");
    assert_eq!(task_label(11, "Twelfth", true), "12. Twelfth");
    assert_eq!(task_label(0, "First", false), "First");
}

#[test]
fn click_translation_distinguishes_boxes_rows_and_outside() {
    let mut app = App::new();
    add_task(&mut app, "First");
    add_task(&mut app, "Second");
    let area = Rect::new(0, 0, 80, 24);
    let layout = app_layout(area, &app);
    let clock = layout.clock().unwrap().area;
    let todo = layout.todo().unwrap();
    let done = layout.done().unwrap();

    assert_eq!(
        click_target(&layout, (clock.x, clock.y), &app),
        ClickTarget::Clock
    );
    assert_eq!(
        click_target(&layout, (todo.x + 1, todo.y + 2), &app),
        ClickTarget::TodoTask(1)
    );
    assert_eq!(
        click_target(&layout, (todo.x, todo.y), &app),
        ClickTarget::Todo
    );
    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let _ = app.dispatch(Action::PrimaryAction);
    assert_eq!(
        click_target(&layout, (done.x + 1, done.y + 1), &app),
        ClickTarget::DoneTask(0)
    );
    assert_eq!(click_target(&layout, (0, 0), &app), ClickTarget::Outside);
}

#[test]
fn click_translation_uses_list_scroll_offsets() {
    let mut app = App::new();
    for index in 0..8 {
        add_task(&mut app, &format!("Task {index}"));
    }
    app.set_offsets(4, 0);
    let area = Rect::new(0, 0, 80, 24);
    let layout = app_layout(area, &app);
    let todo = layout.todo().unwrap();

    assert_eq!(
        click_target(&layout, (todo.x + 1, todo.y + 1), &app),
        ClickTarget::TodoTask(4)
    );
}

#[test]
fn scroll_hit_testing_uses_task_boxes_and_settings_list() {
    let mut app = App::new();
    add_task(&mut app, "First");
    let area = Rect::new(0, 0, 80, 24);
    let layout = app_layout(area, &app);
    let clock = layout.clock().unwrap().area;
    let todo = layout.todo().unwrap();
    let done = layout.done().unwrap();

    assert_eq!(
        scroll_target(&layout, (todo.x, todo.y), &app),
        Some(ScrollTarget::Todo)
    );
    assert_eq!(
        scroll_target(&layout, (done.x, done.y), &app),
        Some(ScrollTarget::Done)
    );
    assert_eq!(scroll_target(&layout, (clock.x, clock.y), &app), None);

    let _ = app.dispatch(Action::OpenSettings);
    let settings = app.settings().unwrap();
    let footer = settings_footer(settings);
    let (list, footer_area) = settings_parts(area, &footer);
    assert_eq!(
        scroll_target(&layout, (list.x, list.y), &app),
        Some(ScrollTarget::Settings)
    );
    assert_eq!(
        scroll_target(&layout, (footer_area.x, footer_area.y), &app),
        None
    );
}

#[test]
fn click_translation_maps_all_visible_session_controls() {
    let app = App::new();
    let area = Rect::new(0, 0, 80, 24);
    let layout = app_layout(area, &app);
    let controls = layout.clock().unwrap().session_controls;

    for (control, session) in controls.into_iter().zip([
        SessionKind::Focus,
        SessionKind::ShortBreak,
        SessionKind::LongBreak,
    ]) {
        assert_eq!(
            click_target(&layout, (control.x, control.y), &app),
            ClickTarget::SessionControl(session)
        );
    }
}

#[test]
fn settings_hit_testing_uses_the_visible_scrolled_rows() {
    let mut app = App::new();
    let _ = app.dispatch(Action::OpenSettings);
    for _ in 0..25 {
        let _ = app.dispatch(Action::SettingsMove(SettingsMoveDirection::Down));
    }
    let area = Rect::new(0, 0, 80, 24);
    let footer = settings_footer(app.settings().unwrap());
    let (list, _) = settings_parts(area, &footer);
    let selection = SettingField::ALL.len() - 1;
    let selected_row = settings_visual_row(selection);
    let first_visible = settings_offset(selected_row, usize::from(list.height));
    let row = (first_visible..)
        .find(|row| settings_field_row(*row).is_some())
        .unwrap();
    let expected = settings_field_row(row).unwrap();
    app.set_settings_offset(first_visible);
    let layout = app_layout(area, &app);

    assert_eq!(
        click_target(
            &layout,
            (list.x, list.y + u16::try_from(row - first_visible).unwrap()),
            &app
        ),
        ClickTarget::SettingsRow(expected)
    );
    assert_eq!(click_target(&layout, (0, 0), &app), ClickTarget::Outside);
}

#[test]
fn settings_click_keeps_the_existing_viewport_when_row_is_already_visible() {
    let mut app = App::new();
    let _ = app.dispatch(Action::OpenSettings);
    let area = Rect::new(0, 0, 80, 24);
    let footer = settings_footer(app.settings().unwrap());
    let (list, _) = settings_parts(area, &footer);
    let offset = settings_visual_row(25).saturating_sub(usize::from(list.height)) + 1;
    app.set_settings_offset(offset);

    let clicked = settings_field_row(offset + 2).unwrap();
    let layout = app_layout(area, &app);
    assert_eq!(
        click_target(&layout, (list.x, list.y + 2), &app),
        ClickTarget::SettingsRow(clicked)
    );
    let _ = app.handle_click_target(ClickTarget::SettingsRow(clicked), std::time::Instant::now());
    assert_eq!(app.settings().unwrap().selection(), clicked);
    assert_eq!(app.settings().unwrap().offset(), offset);
}

#[test]
fn settings_group_first_fields_scroll_with_their_heading() {
    assert_eq!(settings_scroll_anchor(0), 0);
    let notification = SettingField::ALL
        .iter()
        .position(|field| *field == SettingField::NotificationEnabled)
        .unwrap();
    let completion_sound = SettingField::ALL
        .iter()
        .position(|field| *field == SettingField::CompletionSoundEnabled)
        .unwrap();
    assert_eq!(
        settings_scroll_anchor(notification),
        settings_visual_row(notification) - 1
    );
    assert_eq!(
        settings_scroll_anchor(completion_sound),
        settings_visual_row(completion_sound) - 1
    );
    assert_eq!(settings_scroll_anchor(1), settings_visual_row(1));
}

#[test]
fn settings_groups_have_one_heading_and_indented_option_rows() {
    let mut app = App::new();
    let _ = app.dispatch(Action::OpenSettings);
    let settings = app.settings().unwrap();

    assert_eq!(
        theme_role_label(ThemeRole::FocusedBorder),
        "  Focused border"
    );
    assert!(
        !setting_row(SettingField::Theme(ThemeRole::FocusedBorder), settings).contains("Theme /")
    );
    assert!(!setting_row(SettingField::FocusDuration, settings).contains("Timer /"));
    assert!(!setting_row(SettingField::PersistTasks, settings).contains("Tasks /"));
    assert!(!setting_row(SettingField::Key(KeyAction::FocusLeft), settings).contains("Keys /"));

    for (group_index, (_, fields)) in SettingField::GROUPS.iter().enumerate() {
        let first_field = settings_group_start(group_index);
        let heading_row = first_field + group_index;
        assert_eq!(settings_field_row(heading_row), None);
        assert_eq!(settings_field_row(heading_row + 1), Some(first_field));
        assert_eq!(SettingField::ALL[first_field], fields[0]);
    }
}

#[test]
fn settings_help_shows_fixed_navigation_and_the_active_close_keys() {
    let mut app = App::new();
    let _ = app.dispatch(Action::OpenSettings);
    while app.settings().unwrap().field() != SettingField::Key(KeyAction::Settings) {
        let _ = app.dispatch(Action::SettingsMove(SettingsMoveDirection::Down));
    }
    let _ = app.dispatch(Action::SettingsActivate);
    let _ = app.dispatch(Action::SettingsCaptureKey(ConfigKey::Character('t')));

    let footer = settings_footer(app.settings().unwrap());

    assert!(footer.contains("[←/→ or h/l] change"));
    assert!(footer.contains("[Enter/Space] edit"));
    assert!(footer.contains("[t/Esc] close"));
    assert!(!footer.contains("[s/Esc] close"));
}

#[test]
fn narrow_settings_overlay_reserves_every_wrapped_help_row() {
    let mut app = App::new();
    let _ = app.dispatch(Action::OpenSettings);
    let settings = app.settings().unwrap();
    let area = Rect::new(0, 0, 40, 18);
    let footer = settings_footer(settings);

    let (list, footer_area) = settings_parts(area, &footer);
    let wrapped = wrap_help(&footer, footer_area.width);

    assert!(footer_area.height > 2);
    assert_eq!(footer_area.height, text_height(&wrapped));
    assert!(list.height > 0);
    assert!(
        wrapped
            .lines()
            .all(|line| Line::from(line).width() <= usize::from(footer_area.width))
    );
}

#[test]
fn settings_overlay_separates_list_from_help_footer() {
    let mut app = App::new();
    let _ = app.dispatch(Action::OpenSettings);
    let settings = app.settings().unwrap();
    let area = Rect::new(0, 0, 80, 24);
    let footer = settings_footer(settings);

    let (list, footer_area) = settings_parts(area, &footer);

    assert_eq!(footer_area.y, list.bottom() + 1);
}

#[test]
fn settings_help_wraps_between_complete_actions() {
    let mut app = App::new();
    let _ = app.dispatch(Action::OpenSettings);
    let footer = settings_footer(app.settings().unwrap());

    let wrapped = wrap_help(&footer, 24);

    assert!(
        wrapped
            .lines()
            .any(|line| line.contains("[Enter/Space] edit"))
    );
    assert!(wrapped.lines().any(|line| line.contains("[s/Esc] close")));
}

#[test]
fn cutoff_metrics_follow_the_specification() {
    let app = App::new();
    let metrics = stable_footer_metrics(app.input_keys(), 80);

    assert!(metrics.item_width > 0);
    assert!(metrics.height_width >= metrics.item_width);
    assert_eq!(metrics.cutoff, metrics.item_width.max(metrics.height_width));

    let below = metrics.height_width.saturating_sub(1);
    assert!(
        stable_footer_metrics(app.input_keys(), below)
            .heights
            .reserve()
            .is_none_or(|height| height > C_H_SUG)
    );
    assert!(
        stable_footer_metrics(app.input_keys(), metrics.height_width)
            .heights
            .reserve()
            .is_some_and(|height| height <= C_H_SUG)
    );
}

#[test]
fn help_is_suppressed_below_cutoff_and_complete_at_cutoff() {
    let app = App::new();
    let metrics = stable_footer_metrics(app.input_keys(), u16::MAX);

    let below = app_layout(Rect::new(0, 0, metrics.cutoff.saturating_add(1), 40), &app);
    let at = app_layout(Rect::new(0, 0, metrics.cutoff.saturating_add(2), 40), &app);

    assert_eq!(below.footer().height, 0);
    assert!(
        at.footer().height
            >= stable_footer_metrics(app.input_keys(), metrics.cutoff)
                .heights
                .reserve()
                .unwrap()
    );
}

#[test]
fn configured_keybindings_change_stable_footer_metrics() {
    let app = App::new();
    let defaults = KeysConfig::default();
    let configured = KeysConfig::default()
        .with_binding(KeyAction::FocusLeft, ConfigKey::Backspace)
        .with_binding(KeyAction::FocusDown, ConfigKey::Backspace)
        .with_binding(KeyAction::FocusUp, ConfigKey::Backspace)
        .with_binding(KeyAction::FocusRight, ConfigKey::Backspace);

    let default_metrics = stable_footer_metrics(&defaults, u16::MAX);
    let configured_metrics = stable_footer_metrics(&configured, u16::MAX);

    assert!(configured_metrics.item_width > default_metrics.item_width);
    assert!(configured_metrics.cutoff >= configured_metrics.item_width);
    assert_ne!(configured_metrics, default_metrics);

    let area = Rect::new(
        0,
        0,
        crate::ui::layout::T_W_SUG
            .saturating_mul(2)
            .saturating_add(2),
        C_H_SUG.saturating_mul(2).saturating_add(2),
    );
    let workspace_width = inner_width(area);
    assert!(workspace_width >= default_metrics.cutoff);
    assert!(workspace_width < configured_metrics.cutoff);

    let default_footer = stable_footer_metrics(&defaults, workspace_width);
    let configured_footer = stable_footer_metrics(&configured, workspace_width);
    let request = |footer: FooterMetrics| LayoutRequest {
        area,
        footer_heights: footer.heights,
        footer_cutoff: footer.cutoff,
        focus: app.ui_focus(),
        last_task_focus: app.last_task_focus(),
        duration: app.timer().remaining(),
    };

    let with_default_keys = resolve(request(default_footer));
    let with_configured_keys = resolve(request(configured_footer));
    assert_eq!(with_default_keys.mode(), WorkspaceMode::Short);
    assert!(with_default_keys.footer().height > 0);
    assert_eq!(with_configured_keys.mode(), WorkspaceMode::Full);
    assert_eq!(with_configured_keys.footer().height, 0);
}
