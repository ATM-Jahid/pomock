use std::time::Duration;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders},
};

use crate::{
    app::UiFocus,
    display::{BIG_DURATION_HEIGHT, big_duration_width},
};

/// Suggested width of one task panel, including its border.
pub(crate) const T_W_SUG: u16 = 24;
/// Suggested height of every panel, including its border. At this height a
/// sufficiently wide clock can render its glyph timer and all primary rows.
pub(crate) const C_H_SUG: u16 = 10;
const FULL_W_SUG: u16 = T_W_SUG.saturating_mul(2);
const SPACED_CLOCK_MIN_INNER_HEIGHT: u16 = 12;
const NON_GLYPH_HEIGHT: u16 = 3;
const SCALED_GLYPH_PADDING_HEIGHT: u16 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceMode {
    Full,
    Short,
    Narrow,
    Mono,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ClockGeometry {
    pub(crate) area: Rect,
    pub(crate) state: Rect,
    pub(crate) remaining: Rect,
    pub(crate) completed_sessions: Rect,
    pub(crate) session_controls: [Rect; 3],
    pub(crate) face: ClockFace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClockFace {
    Text,
    Glyphs { scale: u16 },
}

#[derive(Debug, Clone, Copy)]
enum TaskGeometry {
    Todo(Rect),
    Done(Rect),
}

#[derive(Debug, Clone, Copy)]
enum WorkspaceGeometry {
    Full {
        clock: ClockGeometry,
        todo: Rect,
        done: Rect,
    },
    ShortClock(ClockGeometry),
    ShortTasks {
        todo: Rect,
        done: Rect,
    },
    Narrow {
        clock: ClockGeometry,
        task: TaskGeometry,
    },
    MonoClock(ClockGeometry),
    MonoTask(TaskGeometry),
}

/// Exact rectangles used to render one application frame.
#[derive(Debug, Clone, Copy)]
pub struct FrameGeometry {
    area: Rect,
    workspace: WorkspaceGeometry,
    footer: Rect,
}

impl FrameGeometry {
    pub(crate) fn area(self) -> Rect {
        self.area
    }

    #[cfg(test)]
    pub(crate) fn mode(self) -> WorkspaceMode {
        match self.workspace {
            WorkspaceGeometry::Full { .. } => WorkspaceMode::Full,
            WorkspaceGeometry::ShortClock(_) | WorkspaceGeometry::ShortTasks { .. } => {
                WorkspaceMode::Short
            }
            WorkspaceGeometry::Narrow { .. } => WorkspaceMode::Narrow,
            WorkspaceGeometry::MonoClock(_) | WorkspaceGeometry::MonoTask(_) => WorkspaceMode::Mono,
        }
    }

    pub(crate) fn clock(self) -> Option<ClockGeometry> {
        match self.workspace {
            WorkspaceGeometry::Full { clock, .. }
            | WorkspaceGeometry::ShortClock(clock)
            | WorkspaceGeometry::Narrow { clock, .. }
            | WorkspaceGeometry::MonoClock(clock) => Some(clock),
            WorkspaceGeometry::ShortTasks { .. } | WorkspaceGeometry::MonoTask(_) => None,
        }
    }

    pub(crate) fn todo(self) -> Option<Rect> {
        match self.workspace {
            WorkspaceGeometry::Full { todo, .. } | WorkspaceGeometry::ShortTasks { todo, .. } => {
                Some(todo)
            }
            WorkspaceGeometry::Narrow {
                task: TaskGeometry::Todo(area),
                ..
            }
            | WorkspaceGeometry::MonoTask(TaskGeometry::Todo(area)) => Some(area),
            WorkspaceGeometry::ShortClock(_)
            | WorkspaceGeometry::Narrow {
                task: TaskGeometry::Done(_),
                ..
            }
            | WorkspaceGeometry::MonoClock(_)
            | WorkspaceGeometry::MonoTask(TaskGeometry::Done(_)) => None,
        }
    }

    pub(crate) fn done(self) -> Option<Rect> {
        match self.workspace {
            WorkspaceGeometry::Full { done, .. } | WorkspaceGeometry::ShortTasks { done, .. } => {
                Some(done)
            }
            WorkspaceGeometry::Narrow {
                task: TaskGeometry::Done(area),
                ..
            }
            | WorkspaceGeometry::MonoTask(TaskGeometry::Done(area)) => Some(area),
            WorkspaceGeometry::ShortClock(_)
            | WorkspaceGeometry::Narrow {
                task: TaskGeometry::Todo(_),
                ..
            }
            | WorkspaceGeometry::MonoClock(_)
            | WorkspaceGeometry::MonoTask(TaskGeometry::Todo(_)) => None,
        }
    }

    pub(crate) fn footer(self) -> Rect {
        self.footer
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LayoutRequest {
    pub(crate) area: Rect,
    pub(crate) footer_heights: FooterHeights,
    pub(crate) footer_cutoff: u16,
    pub(crate) focus: UiFocus,
    pub(crate) last_task_focus: UiFocus,
    pub(crate) duration: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FooterHeights {
    pub(crate) clock: Option<u16>,
    pub(crate) todo: Option<u16>,
    pub(crate) done: Option<u16>,
}

impl FooterHeights {
    pub(crate) fn reserve(self) -> Option<u16> {
        match (self.clock, self.todo, self.done) {
            (Some(clock), Some(todo), Some(done)) => Some(clock.max(todo).max(done)),
            _ => None,
        }
    }
}

pub(crate) fn resolve(request: LayoutRequest) -> FrameGeometry {
    let inner_area = Block::default().borders(Borders::ALL).inner(request.area);
    let footer_reserve = effective_footer_height(
        inner_area.width,
        request.footer_cutoff,
        request.footer_heights,
    );
    let mode = classify(inner_area, footer_reserve);
    let footer_height = budget_footer(mode, inner_area, footer_reserve);
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(footer_height)])
        .split(inner_area);
    let workspace_area = vertical[0];
    let footer = vertical[1];
    let workspace = allocate_workspace(mode, workspace_area, request);

    FrameGeometry {
        area: request.area,
        workspace,
        footer,
    }
}

/// `Q_h(w)` for classification and allocation. Help has no layout cost below
/// its cutoff; at or above the cutoff every focus variant must be complete.
fn effective_footer_height(width: u16, cutoff: u16, heights: FooterHeights) -> u16 {
    if width < cutoff {
        0
    } else {
        heights.reserve().unwrap_or(0)
    }
}

fn classify(inner_area: Rect, footer_reserve: u16) -> WorkspaceMode {
    let h_sug = C_H_SUG
        .saturating_add(C_H_SUG)
        .saturating_add(footer_reserve);

    match (inner_area.width >= FULL_W_SUG, inner_area.height >= h_sug) {
        (true, true) => WorkspaceMode::Full,
        (true, false) => WorkspaceMode::Short,
        (false, true) => WorkspaceMode::Narrow,
        (false, false) => WorkspaceMode::Mono,
    }
}

fn budget_footer(mode: WorkspaceMode, inner_area: Rect, footer_reserve: u16) -> u16 {
    let minimum_workspace_height = match mode {
        WorkspaceMode::Full | WorkspaceMode::Narrow => C_H_SUG.saturating_mul(2),
        WorkspaceMode::Short | WorkspaceMode::Mono => C_H_SUG,
    };

    if footer_reserve > 0
        && inner_area.height >= minimum_workspace_height.saturating_add(footer_reserve)
    {
        footer_reserve
    } else {
        0
    }
}

fn allocate_workspace(
    mode: WorkspaceMode,
    area: Rect,
    request: LayoutRequest,
) -> WorkspaceGeometry {
    match mode {
        WorkspaceMode::Full => {
            let [clock, tasks] = split_clock_and_tasks(area);
            let [todo, done] = split_tasks(tasks);
            WorkspaceGeometry::Full {
                clock: clock_geometry(clock, request.duration),
                todo,
                done,
            }
        }
        WorkspaceMode::Short if request.focus == UiFocus::Clock => {
            WorkspaceGeometry::ShortClock(clock_geometry(area, request.duration))
        }
        WorkspaceMode::Short => {
            let [todo, done] = split_tasks(area);
            WorkspaceGeometry::ShortTasks { todo, done }
        }
        WorkspaceMode::Narrow => {
            let [clock, task_area] = split_clock_and_tasks(area);
            let task = if request.last_task_focus == UiFocus::Done {
                TaskGeometry::Done(task_area)
            } else {
                TaskGeometry::Todo(task_area)
            };
            WorkspaceGeometry::Narrow {
                clock: clock_geometry(clock, request.duration),
                task,
            }
        }
        WorkspaceMode::Mono => match request.focus {
            UiFocus::Clock => WorkspaceGeometry::MonoClock(clock_geometry(area, request.duration)),
            UiFocus::Todo => WorkspaceGeometry::MonoTask(TaskGeometry::Todo(area)),
            UiFocus::Done => WorkspaceGeometry::MonoTask(TaskGeometry::Done(area)),
        },
    }
}

fn split_tasks(area: Rect) -> [Rect; 2] {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    [chunks[0], chunks[1]]
}

fn split_clock_and_tasks(area: Rect) -> [Rect; 2] {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    [chunks[0], chunks[1]]
}

pub(crate) fn clock_geometry(area: Rect, duration: Duration) -> ClockGeometry {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    let scale_for_width = inner.width / big_duration_width(duration);
    let scale_for_height = if inner.height >= NON_GLYPH_HEIGHT + BIG_DURATION_HEIGHT {
        (inner.height.saturating_sub(SCALED_GLYPH_PADDING_HEIGHT) / BIG_DURATION_HEIGHT).max(1)
    } else {
        0
    };
    let glyph_scale = scale_for_width.min(scale_for_height);
    let face = if glyph_scale == 0 {
        ClockFace::Text
    } else {
        ClockFace::Glyphs { scale: glyph_scale }
    };
    let duration_height = match face {
        ClockFace::Text => u16::from(inner.height > 0),
        ClockFace::Glyphs { scale } => BIG_DURATION_HEIGHT.saturating_mul(scale),
    };
    let state_height = u16::from(inner.height > duration_height);
    let controls_height = u16::from(inner.height > duration_height + state_height);
    let completed_height =
        u16::from(inner.height > duration_height + state_height + controls_height);
    let content_gap = u16::from(inner.height >= SPACED_CLOCK_MIN_INNER_HEIGHT);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(state_height),
            Constraint::Length(content_gap),
            Constraint::Length(duration_height),
            Constraint::Length(content_gap),
            Constraint::Length(completed_height),
            Constraint::Fill(1),
            Constraint::Length(controls_height),
        ])
        .split(inner);
    let controls = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
        ])
        .split(chunks[7]);

    ClockGeometry {
        area,
        state: chunks[1],
        remaining: chunks[3],
        completed_sessions: chunks[5],
        session_controls: [controls[0], controls[1], controls[2]],
        face,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT_DURATION: Duration = Duration::from_secs(25 * 60);

    fn request_for_workspace(width: u16, height: u16) -> LayoutRequest {
        LayoutRequest {
            area: Rect::new(0, 0, width.saturating_add(2), height.saturating_add(2)),
            footer_heights: FooterHeights {
                clock: Some(2),
                todo: Some(3),
                done: Some(4),
            },
            footer_cutoff: 0,
            focus: UiFocus::Clock,
            last_task_focus: UiFocus::Todo,
            duration: DEFAULT_DURATION,
        }
    }

    #[test]
    fn every_mode_and_help_boundary_uses_fitting_side_equality() {
        const Q_H: u16 = 4;
        const W_CUT: u16 = 16;

        let widths = [
            W_CUT - 1,
            W_CUT,
            W_CUT + 1,
            FULL_W_SUG - 1,
            FULL_W_SUG,
            FULL_W_SUG + 1,
        ];
        let heights = [
            C_H_SUG - 1,
            C_H_SUG,
            C_H_SUG + Q_H - 1,
            C_H_SUG + Q_H,
            C_H_SUG.saturating_mul(2) - 1,
            C_H_SUG.saturating_mul(2),
            C_H_SUG.saturating_mul(2).saturating_add(Q_H) - 1,
            C_H_SUG.saturating_mul(2).saturating_add(Q_H),
            C_H_SUG.saturating_mul(2).saturating_add(Q_H) + 1,
        ];

        for width in widths {
            for height in heights {
                let request = LayoutRequest {
                    footer_cutoff: W_CUT,
                    ..request_for_workspace(width, height)
                };
                let geometry = resolve(request);

                let help_available = width >= W_CUT;
                let help_height = if help_available { Q_H } else { 0 };
                let tall = height >= C_H_SUG.saturating_mul(2).saturating_add(help_height);
                let wide = width >= FULL_W_SUG;

                let expected_mode = match (wide, tall) {
                    (true, true) => WorkspaceMode::Full,
                    (true, false) => WorkspaceMode::Short,
                    (false, true) => WorkspaceMode::Narrow,
                    (false, false) => WorkspaceMode::Mono,
                };

                let expected_help = if help_available && height >= C_H_SUG.saturating_add(Q_H) {
                    Q_H
                } else {
                    0
                };

                assert_eq!(
                    geometry.mode(),
                    expected_mode,
                    "workspace: {width}x{height}"
                );
                assert_eq!(
                    geometry.footer().height,
                    expected_help,
                    "workspace: {width}x{height}"
                );
            }
        }
    }

    #[test]
    fn crossing_footer_cutoff_can_change_narrow_to_mono() {
        const Q_H: u16 = 4;
        const W_CUT: u16 = 16;
        let height = C_H_SUG.saturating_mul(2);

        let below = resolve(LayoutRequest {
            footer_cutoff: W_CUT,
            ..request_for_workspace(W_CUT - 1, height)
        });
        let at = resolve(LayoutRequest {
            footer_cutoff: W_CUT,
            ..request_for_workspace(W_CUT, height)
        });

        assert_eq!(below.mode(), WorkspaceMode::Narrow);
        assert_eq!(below.footer().height, 0);

        assert_eq!(at.mode(), WorkspaceMode::Mono);
        assert_eq!(at.footer().height, Q_H);
    }

    #[test]
    fn help_height_is_zero_below_a_cutoff_larger_than_the_full_width_threshold() {
        const LARGE_CUTOFF: u16 = FULL_W_SUG + 10;
        let height = C_H_SUG.saturating_mul(2);

        let geometry = resolve(LayoutRequest {
            footer_cutoff: LARGE_CUTOFF,
            ..request_for_workspace(FULL_W_SUG, height)
        });

        assert_eq!(geometry.mode(), WorkspaceMode::Full);
        assert_eq!(geometry.footer().height, 0);
    }

    #[test]
    fn effective_footer_height_is_zero_below_cutoff_and_complete_at_it() {
        let heights = FooterHeights {
            clock: Some(2),
            todo: Some(3),
            done: Some(4),
        };

        assert_eq!(effective_footer_height(15, 16, heights), 0);
        assert_eq!(effective_footer_height(16, 16, heights), 4);
        assert_eq!(effective_footer_height(17, 16, heights), 4);
    }

    #[test]
    fn stable_footer_reserve_is_the_maximum_of_three_viable_help_heights() {
        let geometry = resolve(request_for_workspace(FULL_W_SUG, C_H_SUG + C_H_SUG + 4));
        assert_eq!(geometry.mode(), WorkspaceMode::Full);
        assert_eq!(geometry.footer().height, 4);
    }

    #[test]
    fn any_non_viable_help_variant_disables_the_footer_for_every_focus() {
        for missing in 0..3 {
            let mut heights = [Some(2), Some(3), Some(4)];
            heights[missing] = None;
            for focus in [UiFocus::Clock, UiFocus::Todo, UiFocus::Done] {
                let geometry = resolve(LayoutRequest {
                    footer_heights: FooterHeights {
                        clock: heights[0],
                        todo: heights[1],
                        done: heights[2],
                    },
                    focus,
                    ..request_for_workspace(FULL_W_SUG, C_H_SUG + C_H_SUG)
                });
                assert_eq!(geometry.mode(), WorkspaceMode::Full);
                assert_eq!(geometry.footer().height, 0);
            }
        }
    }

    #[test]
    fn help_is_allocated_wholly_or_omitted_when_content_budget_is_too_small() {
        for (height, expected) in [(C_H_SUG + 4, 4), (C_H_SUG + 3, 0), (C_H_SUG - 1, 0)] {
            let geometry = resolve(request_for_workspace(FULL_W_SUG, height));
            assert_eq!(geometry.mode(), WorkspaceMode::Short);
            assert_eq!(geometry.footer().height, expected);
        }

        let reserve_exceeds_height = resolve(LayoutRequest {
            footer_heights: FooterHeights {
                clock: Some(30),
                todo: Some(29),
                done: Some(28),
            },
            ..request_for_workspace(FULL_W_SUG, 25)
        });
        assert_eq!(reserve_exceeds_height.footer().height, 0);
    }

    #[test]
    fn decreasing_height_never_reenters_a_larger_vertical_mode() {
        for width in [FULL_W_SUG - 1, FULL_W_SUG] {
            let modes: Vec<_> = (0..=30)
                .rev()
                .map(|height| resolve(request_for_workspace(width, height)).mode())
                .collect();
            let transition = if width >= FULL_W_SUG {
                (WorkspaceMode::Short, WorkspaceMode::Full)
            } else {
                (WorkspaceMode::Mono, WorkspaceMode::Narrow)
            };
            assert!(
                !modes
                    .windows(2)
                    .any(|pair| pair == [transition.0, transition.1])
            );
        }
    }

    #[test]
    fn short_narrow_and_mono_panel_selection_is_focus_driven() {
        let short_clock = resolve(request_for_workspace(FULL_W_SUG, C_H_SUG + 4 - 1));
        assert!(short_clock.clock().is_some());
        assert!(short_clock.todo().is_none());

        let short_tasks = resolve(LayoutRequest {
            focus: UiFocus::Todo,
            ..request_for_workspace(FULL_W_SUG, C_H_SUG + 4 - 1)
        });
        assert!(short_tasks.clock().is_none());
        assert!(short_tasks.todo().is_some() && short_tasks.done().is_some());

        let narrow = resolve(LayoutRequest {
            focus: UiFocus::Clock,
            last_task_focus: UiFocus::Done,
            ..request_for_workspace(FULL_W_SUG - 1, C_H_SUG + C_H_SUG + 4)
        });
        assert!(narrow.clock().is_some() && narrow.done().is_some());
        assert!(narrow.todo().is_none());

        for focus in [UiFocus::Clock, UiFocus::Todo, UiFocus::Done] {
            let mono = resolve(LayoutRequest {
                focus,
                ..request_for_workspace(FULL_W_SUG - 1, C_H_SUG + 4 - 1)
            });
            assert_eq!(mono.clock().is_some(), focus == UiFocus::Clock);
            assert_eq!(mono.todo().is_some(), focus == UiFocus::Todo);
            assert_eq!(mono.done().is_some(), focus == UiFocus::Done);
        }
    }

    #[test]
    fn task_columns_are_equal_halves_in_full_and_short_modes() {
        for width in [FULL_W_SUG, FULL_W_SUG + 1] {
            let full = resolve(request_for_workspace(
                width,
                C_H_SUG.saturating_mul(2).saturating_add(4),
            ));
            assert_eq!(full.mode(), WorkspaceMode::Full);

            let full_todo = full.todo().unwrap();
            let full_done = full.done().unwrap();
            assert!(full_todo.width.abs_diff(full_done.width) <= 1);
            assert_eq!(full_todo.width.saturating_add(full_done.width), width);
            assert_eq!(full_todo.right(), full_done.x);

            let short = resolve(LayoutRequest {
                focus: UiFocus::Todo,
                ..request_for_workspace(width, C_H_SUG)
            });
            assert_eq!(short.mode(), WorkspaceMode::Short);

            let short_todo = short.todo().unwrap();
            let short_done = short.done().unwrap();
            assert!(short_todo.width.abs_diff(short_done.width) <= 1);
            assert_eq!(short_todo.width.saturating_add(short_done.width), width);
            assert_eq!(short_todo.right(), short_done.x);
        }
    }

    #[test]
    fn stacked_sections_are_equal_halves_including_odd_heights() {
        for width in [FULL_W_SUG - 1, FULL_W_SUG] {
            for height in [24, 25] {
                let geometry = resolve(LayoutRequest {
                    footer_heights: FooterHeights {
                        clock: None,
                        todo: None,
                        done: None,
                    },
                    ..request_for_workspace(width, height)
                });
                let clock = geometry.clock().unwrap().area;
                let task = geometry.todo().unwrap();
                assert!(clock.height.abs_diff(task.height) <= 1);
                assert_eq!(clock.height.saturating_add(task.height), height);
                assert_eq!(clock.bottom(), task.y);
            }
        }
    }

    #[test]
    fn declared_clock_height_renders_glyphs_and_smaller_areas_degrade() {
        assert_eq!(
            clock_geometry(Rect::new(0, 0, 32, C_H_SUG - 1), DEFAULT_DURATION).face,
            ClockFace::Text
        );
        assert_eq!(
            clock_geometry(Rect::new(0, 0, 32, C_H_SUG), DEFAULT_DURATION).face,
            ClockFace::Glyphs { scale: 1 }
        );
        assert_eq!(
            clock_geometry(Rect::new(0, 0, 62, 19), DEFAULT_DURATION).face,
            ClockFace::Glyphs { scale: 2 }
        );
    }

    #[test]
    fn tiny_clock_adds_rows_in_content_priority_order() {
        let one_row = clock_geometry(Rect::new(0, 0, 20, 3), DEFAULT_DURATION);
        assert_eq!(one_row.remaining.height, 1);
        assert_eq!(one_row.state.height, 0);
        assert_eq!(one_row.session_controls[0].height, 0);
        assert_eq!(one_row.completed_sessions.height, 0);

        let two_rows = clock_geometry(Rect::new(0, 0, 20, 4), DEFAULT_DURATION);
        assert_eq!(two_rows.remaining.height, 1);
        assert_eq!(two_rows.state.height, 1);
        assert_eq!(two_rows.session_controls[0].height, 0);
        assert_eq!(two_rows.completed_sessions.height, 0);

        let three_rows = clock_geometry(Rect::new(0, 0, 20, 5), DEFAULT_DURATION);
        assert_eq!(three_rows.session_controls[0].height, 1);
        assert_eq!(three_rows.completed_sessions.height, 0);

        let four_rows = clock_geometry(Rect::new(0, 0, 20, 6), DEFAULT_DURATION);
        assert_eq!(four_rows.completed_sessions.height, 1);
    }

    #[test]
    fn mono_task_keeps_one_printable_list_row() {
        let geometry = resolve(LayoutRequest {
            focus: UiFocus::Todo,
            footer_heights: FooterHeights {
                clock: None,
                todo: None,
                done: None,
            },
            ..request_for_workspace(18, 3)
        });
        let todo = geometry.todo().unwrap();

        assert_eq!(geometry.mode(), WorkspaceMode::Mono);
        assert_eq!(Block::default().borders(Borders::ALL).inner(todo).height, 1);
        assert_eq!(geometry.footer().height, 0);
    }

    #[test]
    fn generated_regions_stay_inside_the_terminal_at_all_small_sizes() {
        const WIDTHS: [u16; 15] = [0, 1, 2, 8, 9, 10, 31, 32, 33, 49, 50, 51, 62, 63, 100];
        const HEIGHTS: [u16; 16] = [0, 1, 2, 3, 4, 8, 9, 10, 11, 12, 13, 14, 18, 19, 20, 40];

        for width in WIDTHS {
            for height in HEIGHTS {
                let geometry = resolve(LayoutRequest {
                    area: Rect::new(0, 0, width, height),
                    footer_heights: FooterHeights {
                        clock: None,
                        todo: None,
                        done: None,
                    },
                    footer_cutoff: 0,
                    focus: UiFocus::Clock,
                    last_task_focus: UiFocus::Todo,
                    duration: DEFAULT_DURATION,
                });
                let terminal = geometry.area();
                let mut regions = vec![geometry.footer()];
                if let Some(clock) = geometry.clock() {
                    regions.extend([
                        clock.area,
                        clock.state,
                        clock.remaining,
                        clock.completed_sessions,
                    ]);
                    regions.extend(clock.session_controls);
                }
                regions.extend(geometry.todo());
                regions.extend(geometry.done());
                for region in regions {
                    assert!(
                        region.x >= terminal.x
                            && region.y >= terminal.y
                            && region.right() <= terminal.right()
                            && region.bottom() <= terminal.bottom(),
                        "terminal: {terminal:?}, region: {region:?}, mode: {:?}",
                        geometry.mode()
                    );
                }
            }
        }
    }
}
