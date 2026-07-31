use super::{Action, App, AppOutcome, Direction, ScrollTarget};
use crate::{config::Config, settings::SettingsOverlay};

impl App {
    pub(super) fn open_settings(&mut self) {
        self.settings = Some(SettingsOverlay::new(&self.config));
        self.clear_pending_click();
    }

    pub(super) fn dispatch_settings(&mut self, action: Action) -> AppOutcome {
        let mut close = false;
        {
            let settings = self.settings.as_mut().expect("settings overlay is open");
            match action {
                Action::SettingsMove(down) => settings.move_selection(down),
                Action::Scroll(ScrollTarget::Settings, Direction::Down) => {
                    settings.move_selection(true)
                }
                Action::Scroll(ScrollTarget::Settings, Direction::Up) => {
                    settings.move_selection(false)
                }
                Action::SettingsAdjust(forward) => settings.adjust(forward),
                Action::SettingsActivate => settings.activate(),
                Action::SettingsClose => {
                    close = settings.input().is_none() && !settings.is_capturing_key();
                }
                Action::SettingsPushInput(character) => settings.push_input(character),
                Action::SettingsPopInput => settings.pop_input(),
                Action::SettingsSubmitInput => settings.submit_input(),
                Action::SettingsCaptureKey(key) => settings.capture_key(key),
                Action::SettingsCancel => {
                    settings.cancel_nested();
                }
                _ => {}
            }
        }

        if close {
            self.settings = None;
            self.clear_pending_click();
            return AppOutcome::None;
        }

        let updated = self
            .settings
            .as_ref()
            .filter(|settings| settings.config() != &self.config)
            .map(|settings| settings.config().clone());
        updated.map_or(AppOutcome::None, |config| self.apply_settings(config))
    }

    fn apply_settings(&mut self, config: Config) -> AppOutcome {
        if config.timer() != self.config.timer() {
            let timer = config.timer();
            self.timer.reconfigure(
                timer.focus_duration(),
                timer.short_break_duration(),
                timer.long_break_duration(),
                timer.long_break_interval(),
            );
        }
        self.show_task_numbers = config.tasks().show_numbers();
        self.config = config.clone();
        AppOutcome::SettingsChanged(Box::new(config))
    }
}
