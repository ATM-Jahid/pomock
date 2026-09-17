use super::{advance_timer, should_handle_key_event};
use crossterm::event::KeyEventKind;
use pomock::app::{Action, App, AppOutcome, FocusAudioAction};
use std::time::{Duration, Instant};

#[test]
fn key_releases_are_ignored_while_presses_and_repeats_are_handled() {
    assert!(should_handle_key_event(KeyEventKind::Press));
    assert!(should_handle_key_event(KeyEventKind::Repeat));
    assert!(!should_handle_key_event(KeyEventKind::Release));
}

#[test]
fn ready_time_before_start_is_not_charged_to_the_running_session() {
    let mut app = App::new();
    let start = Instant::now();
    let mut last_tick = start;
    let key_time = start + Duration::from_millis(80);

    assert_eq!(
        advance_timer(&mut app, &mut last_tick, key_time),
        AppOutcome::None
    );
    assert_eq!(
        app.dispatch(Action::PrimaryAction),
        AppOutcome::FocusAudio(FocusAudioAction::StartOrResume)
    );

    assert_eq!(
        advance_timer(
            &mut app,
            &mut last_tick,
            key_time + Duration::from_secs(25 * 60) - Duration::from_millis(1),
        ),
        AppOutcome::None
    );
    assert_eq!(
        advance_timer(
            &mut app,
            &mut last_tick,
            key_time + Duration::from_secs(25 * 60),
        ),
        AppOutcome::SessionCompleted(pomock::SessionKind::Focus)
    );
}
