use std::time::Duration;

use display::{format_duration, format_state};
use timer::PomodoroTimer;

mod display;
mod timer;

fn main() {
    let timer = PomodoroTimer::new(Duration::from_secs(25 * 60), Duration::from_secs(5 * 60));

    println!("State: {}", format_state(timer.state()));
    println!("Remaining: {}", format_duration(timer.remaining()));
}
