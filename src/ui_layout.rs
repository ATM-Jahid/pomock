use std::time::Duration;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders},
};

use crate::{
    app::UiFocus,
    display::{BIG_DURATION_HEIGHT, big_duration_width},
};

// Border, state, duration, completed count, and session controls.
const MIN_CLOCK_HEIGHT: u16 = 6;
const MIN_TASK_HEIGHT: u16 = 3;
const MIN_TASK_WIDTH: u16 = 24;
const RESERVED_HELP_HEIGHT: u16 = 2;
const SPACED_CLOCK_MIN_INNER_HEIGHT: u16 = 12;
const NON_GLYPH_HEIGHT: u16 = 3;
const SCALED_GLYPH_PADDING_HEIGHT: u16 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceMode {
    Full,
    Short,
    Narrow,
    Compact,
    Tiny,
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
pub(crate) struct TextClockGeometry {
    pub(crate) area: Rect,
    pub(crate) remaining: Rect,
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
    CompactClock(ClockGeometry),
    CompactTask(TaskGeometry),
    TinyClock(TextClockGeometry),
}

/// Exact rectangles used to render one application frame.
#[derive(Debug, Clone, Copy)]
pub struct FrameGeometry {
    area: Rect,
    mode: WorkspaceMode,
    workspace: WorkspaceGeometry,
    controls: Rect,
}

impl FrameGeometry {
    pub(crate) fn area(self) -> Rect {
        self.area
    }

    pub(crate) fn mode(self) -> WorkspaceMode {
        self.mode
    }

    pub(crate) fn clock(self) -> Option<ClockGeometry> {
        match self.workspace {
            WorkspaceGeometry::Full { clock, .. }
            | WorkspaceGeometry::ShortClock(clock)
            | WorkspaceGeometry::Narrow { clock, .. }
            | WorkspaceGeometry::CompactClock(clock) => Some(clock),
            WorkspaceGeometry::ShortTasks { .. }
            | WorkspaceGeometry::CompactTask(_)
            | WorkspaceGeometry::TinyClock(_) => None,
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
            | WorkspaceGeometry::CompactTask(TaskGeometry::Todo(area)) => Some(area),
            WorkspaceGeometry::ShortClock(_)
            | WorkspaceGeometry::Narrow {
                task: TaskGeometry::Done(_),
                ..
            }
            | WorkspaceGeometry::CompactClock(_)
            | WorkspaceGeometry::CompactTask(TaskGeometry::Done(_))
            | WorkspaceGeometry::TinyClock(_) => None,
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
            | WorkspaceGeometry::CompactTask(TaskGeometry::Done(area)) => Some(area),
            WorkspaceGeometry::ShortClock(_)
            | WorkspaceGeometry::Narrow {
                task: TaskGeometry::Todo(_),
                ..
            }
            | WorkspaceGeometry::CompactClock(_)
            | WorkspaceGeometry::CompactTask(TaskGeometry::Todo(_))
            | WorkspaceGeometry::TinyClock(_) => None,
        }
    }

    pub(crate) fn compact_clock(self) -> Option<TextClockGeometry> {
        match self.workspace {
            WorkspaceGeometry::TinyClock(area) => Some(area),
            _ => None,
        }
    }

    pub(crate) fn controls(self) -> Rect {
        self.controls
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LayoutRequest {
    pub(crate) area: Rect,
    pub(crate) controls_height: u16,
    pub(crate) focus: UiFocus,
    pub(crate) last_task_focus: UiFocus,
    pub(crate) duration: Duration,
}

pub(crate) fn resolve(request: LayoutRequest) -> FrameGeometry {
    let inner_area = Block::default().borders(Borders::ALL).inner(request.area);
    let mode = classify(inner_area, request.duration);
    let controls_height = budget_help(mode, inner_area, request.controls_height);
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(controls_height)])
        .split(inner_area);
    let workspace_area = vertical[0];
    let controls = vertical[1];
    let workspace = allocate_workspace(mode, workspace_area, request);

    FrameGeometry {
        area: request.area,
        mode,
        workspace,
        controls,
    }
}

fn classify(inner_area: Rect, duration: Duration) -> WorkspaceMode {
    let single_width = u16::try_from(crate::display::format_duration(duration).len())
        .unwrap_or(u16::MAX)
        .saturating_add(2);
    let split_width = MIN_TASK_WIDTH.saturating_mul(2);
    let stacked_height = MIN_CLOCK_HEIGHT.saturating_add(MIN_TASK_HEIGHT);
    let classifiable_height = inner_area.height.saturating_sub(RESERVED_HELP_HEIGHT);
    let can_show_single =
        inner_area.width >= single_width && classifiable_height >= MIN_CLOCK_HEIGHT;
    let can_split_width = inner_area.width >= split_width;
    let can_stack_height = classifiable_height >= stacked_height;

    match (can_show_single, can_split_width, can_stack_height) {
        (true, true, true) => WorkspaceMode::Full,
        (true, true, false) => WorkspaceMode::Short,
        (true, false, true) => WorkspaceMode::Narrow,
        (true, false, false) => WorkspaceMode::Compact,
        (false, _, _) => WorkspaceMode::Tiny,
    }
}

fn budget_help(mode: WorkspaceMode, inner_area: Rect, desired_height: u16) -> u16 {
    let stacked_height = MIN_CLOCK_HEIGHT.saturating_add(MIN_TASK_HEIGHT);
    let minimum_workspace_height = match mode {
        WorkspaceMode::Full | WorkspaceMode::Narrow => stacked_height,
        WorkspaceMode::Short | WorkspaceMode::Compact => MIN_CLOCK_HEIGHT,
        // A boxed text clock needs its two borders and one printable timer row.
        WorkspaceMode::Tiny => 3,
    };
    let available_help_height = inner_area.height.saturating_sub(minimum_workspace_height);
    let clock_can_show_text = mode != WorkspaceMode::Tiny || inner_area.width >= 7;

    if clock_can_show_text && desired_height <= available_help_height {
        desired_height
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
        WorkspaceMode::Compact => match request.focus {
            UiFocus::Clock => {
                WorkspaceGeometry::CompactClock(clock_geometry(area, request.duration))
            }
            UiFocus::Todo => WorkspaceGeometry::CompactTask(TaskGeometry::Todo(area)),
            UiFocus::Done => WorkspaceGeometry::CompactTask(TaskGeometry::Done(area)),
        },
        WorkspaceMode::Tiny => {
            let remaining = Block::default().borders(Borders::ALL).inner(area);
            WorkspaceGeometry::TinyClock(TextClockGeometry { area, remaining })
        }
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
    let proportional_clock_height = area.height.saturating_mul(55) / 100;
    let clock_height = proportional_clock_height
        .max(MIN_CLOCK_HEIGHT)
        .min(area.height.saturating_sub(MIN_TASK_HEIGHT));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(clock_height), Constraint::Min(0)])
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
        ClockFace::Text => 1,
        ClockFace::Glyphs { scale } => BIG_DURATION_HEIGHT.saturating_mul(scale),
    };
    let content_gap = u16::from(inner.height >= SPACED_CLOCK_MIN_INNER_HEIGHT);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Length(content_gap),
            Constraint::Length(duration_height),
            Constraint::Length(content_gap),
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
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

    fn request(width: u16, height: u16) -> LayoutRequest {
        LayoutRequest {
            area: Rect::new(0, 0, width, height),
            controls_height: 2,
            focus: UiFocus::Clock,
            last_task_focus: UiFocus::Todo,
            duration: DEFAULT_DURATION,
        }
    }

    #[test]
    fn exact_default_duration_boundaries_select_each_mode() {
        for (width, height, expected) in [
            (50, 13, WorkspaceMode::Full),
            (50, 12, WorkspaceMode::Short),
            (49, 13, WorkspaceMode::Narrow),
            (9, 12, WorkspaceMode::Compact),
            (8, 12, WorkspaceMode::Tiny),
            (9, 9, WorkspaceMode::Tiny),
        ] {
            assert_eq!(
                resolve(request(width, height)).mode(),
                expected,
                "terminal: {width}x{height}"
            );
        }
    }

    #[test]
    fn help_height_never_changes_the_space_class() {
        for (width, height) in [(80, 24), (80, 14), (40, 24), (40, 16), (20, 10)] {
            let baseline = resolve(request(width, height)).mode();
            for controls_height in [0, 1, 10, u16::MAX] {
                assert_eq!(
                    resolve(LayoutRequest {
                        controls_height,
                        ..request(width, height)
                    })
                    .mode(),
                    baseline
                );
            }
        }
    }

    #[test]
    fn contextual_help_is_complete_or_omitted_in_every_workspace_mode() {
        for (width, height, desired_height, expected_mode, expected_height) in [
            (50, 13, 2, WorkspaceMode::Full, 2),
            (50, 13, 3, WorkspaceMode::Full, 0),
            (50, 12, 4, WorkspaceMode::Short, 4),
            (50, 12, 5, WorkspaceMode::Short, 0),
            (20, 13, 2, WorkspaceMode::Narrow, 2),
            (20, 13, 3, WorkspaceMode::Narrow, 0),
            (20, 10, 2, WorkspaceMode::Compact, 2),
            (20, 10, 3, WorkspaceMode::Compact, 0),
            (80, 9, 4, WorkspaceMode::Tiny, 4),
            (80, 9, 5, WorkspaceMode::Tiny, 0),
        ] {
            let geometry = resolve(LayoutRequest {
                controls_height: desired_height,
                ..request(width, height)
            });

            assert_eq!(geometry.mode(), expected_mode, "terminal: {width}x{height}");
            assert_eq!(
                geometry.controls().height,
                expected_height,
                "terminal: {width}x{height}, desired help: {desired_height}"
            );
        }
    }

    #[test]
    fn clock_face_steps_from_text_through_every_fitting_glyph_scale() {
        let duration = DEFAULT_DURATION;

        assert_eq!(
            clock_geometry(Rect::new(0, 0, 20, 10), duration).face,
            ClockFace::Text
        );
        assert_eq!(
            clock_geometry(Rect::new(0, 0, 32, 10), duration).face,
            ClockFace::Glyphs { scale: 1 }
        );
        assert_eq!(
            clock_geometry(Rect::new(0, 0, 62, 19), duration).face,
            ClockFace::Glyphs { scale: 2 }
        );
    }

    #[test]
    fn generated_regions_stay_inside_the_terminal_at_all_small_sizes() {
        const WIDTHS: [u16; 15] = [0, 1, 2, 8, 9, 10, 31, 32, 33, 49, 50, 51, 62, 63, 100];
        const HEIGHTS: [u16; 16] = [0, 1, 2, 3, 4, 8, 9, 10, 11, 12, 13, 14, 18, 19, 20, 40];

        for width in WIDTHS {
            for height in HEIGHTS {
                let geometry = resolve(request(width, height));
                let terminal = geometry.area();
                let mut regions = vec![geometry.controls()];
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
                if let Some(clock) = geometry.compact_clock() {
                    regions.extend([clock.area, clock.remaining]);
                }

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
