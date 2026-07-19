use std::time::Duration;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders},
};

use crate::{
    app::UiFocus,
    display::{BIG_DURATION_HEIGHT, big_duration_width},
};

const MIN_CLOCK_HEIGHT: u16 = 10;
const MIN_TASK_HEIGHT: u16 = 3;
const MIN_TASK_WIDTH: u16 = 24;
const RESERVED_HELP_HEIGHT: u16 = 2;
const SPACED_CLOCK_MIN_INNER_HEIGHT: u16 = 12;
// Two gaps, two outer padding rows, and the status, count, and controls rows.
const NON_GLYPH_SPACED_HEIGHT: u16 = 7;

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
    pub(crate) glyph_scale: u16,
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
    TinyClock(Rect),
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

    pub(crate) fn compact_clock(self) -> Option<Rect> {
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
    let single_width = big_duration_width(duration).saturating_add(2);
    let split_width = single_width.max(MIN_TASK_WIDTH.saturating_mul(2));
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
        WorkspaceMode::Tiny => 0,
    };

    if mode == WorkspaceMode::Tiny {
        let available_help_height = inner_area.height.saturating_sub(1);
        if inner_area.width >= 5 && desired_height <= available_help_height {
            desired_height
        } else {
            0
        }
    } else {
        desired_height.min(inner_area.height.saturating_sub(minimum_workspace_height))
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
        WorkspaceMode::Tiny => WorkspaceGeometry::TinyClock(area),
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
    let scale_for_height =
        inner.height.saturating_sub(NON_GLYPH_SPACED_HEIGHT) / BIG_DURATION_HEIGHT;
    let glyph_scale = scale_for_width.min(scale_for_height).max(1);
    let content_gap = u16::from(inner.height >= SPACED_CLOCK_MIN_INNER_HEIGHT);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Length(content_gap),
            Constraint::Length(BIG_DURATION_HEIGHT * glyph_scale),
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
        glyph_scale,
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
            (50, 17, WorkspaceMode::Full),
            (50, 16, WorkspaceMode::Short),
            (49, 17, WorkspaceMode::Narrow),
            (34, 14, WorkspaceMode::Compact),
            (33, 14, WorkspaceMode::Tiny),
            (34, 13, WorkspaceMode::Tiny),
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
    fn generated_regions_stay_inside_the_terminal_at_all_small_sizes() {
        for width in 0..=100 {
            for height in 0..=40 {
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
                regions.extend(geometry.compact_clock());

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
