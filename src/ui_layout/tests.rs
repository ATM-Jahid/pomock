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
