use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    widgets::{Block, Borders, Paragraph},
};

use self::layout::{LayoutRequest, resolve};
use crate::{
    app::{App, UiFocus},
    config::KeysConfig,
    display::format_state,
};

mod clock;
mod footer;
mod hit_test;
mod layout;
mod settings;
mod task_lists;
mod theme;

use clock::{draw_clock, session_label};
use footer::{footer_text, stable_footer_metrics, wrap_help};
pub use hit_test::{action_target_visible, click_target, scroll_target};
pub use layout::FrameGeometry;
use settings::draw_settings;
use task_lists::draw_task_lists;
pub use theme::Theme;
use theme::focused_block;

/// Renders the complete application UI and synchronizes list scroll offsets.
pub fn draw(frame: &mut Frame, app: &mut App, theme: Theme, keys: &KeysConfig) -> FrameGeometry {
    let theme = app
        .settings()
        .map_or(theme, |settings| Theme::from(settings.config().theme()));
    let area = frame.area();
    let footer_text = footer_text(app, keys);
    let workspace_width = inner_width(area);
    let footer = stable_footer_metrics(keys, workspace_width);
    let footer_text = wrap_help(&footer_text, workspace_width);
    let layout = resolve(LayoutRequest {
        area,
        footer_heights: footer.heights,
        footer_cutoff: footer.cutoff,
        focus: app.ui_focus(),
        last_task_focus: app.last_task_focus(),
        duration: app.timer().remaining(),
    });

    let outer_block = Block::default().title("pomock").borders(Borders::ALL);
    frame.render_widget(outer_block, area);

    let remaining_duration = app.timer().remaining();

    let state_text = app.pending_autostart().map_or_else(
        || format_state(app.timer().state()).to_string(),
        |(session, seconds)| format!("Next: {} (autostart in {seconds}s)", session_label(session)),
    );
    let footer = Paragraph::new(footer_text).alignment(Alignment::Center);

    if let Some(clock_layout) = layout.clock() {
        let clock_area = clock_layout.area;
        let clock_block = focused_block("Clock", app.ui_focus() == UiFocus::Clock, theme);
        frame.render_widget(clock_block, clock_area);
        draw_clock(
            frame,
            clock_layout,
            app,
            theme,
            &state_text,
            remaining_duration,
        );
    }
    draw_task_lists(frame, &layout, app, theme);
    if layout.footer().width > 0 && layout.footer().height > 0 {
        frame.render_widget(footer, layout.footer());
    }

    if app.is_settings_open() {
        draw_settings(frame, app, theme);
    }

    layout
}

fn inner_width(area: Rect) -> u16 {
    Block::default().borders(Borders::ALL).inner(area).width
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
