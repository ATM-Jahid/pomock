//! Best-effort desktop notification boundary.

use notify_rust::Notification;

use crate::SessionKind;

/// Delivers session-completion notifications outside the application domain.
pub trait Notifier {
    /// Reports that a timer session reached zero.
    fn session_completed(&mut self, session: SessionKind);
}

/// Cross-platform native desktop notification adapter.
#[derive(Debug, Default)]
pub struct DesktopNotifier;

impl Notifier for DesktopNotifier {
    fn session_completed(&mut self, session: SessionKind) {
        let message = completion_message(session);
        let _ = Notification::new()
            .appname("pomock")
            .summary(message.summary)
            .body(message.body)
            .show();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CompletionMessage {
    summary: &'static str,
    body: &'static str,
}

fn completion_message(session: SessionKind) -> CompletionMessage {
    match session {
        SessionKind::Focus => CompletionMessage {
            summary: "Focus session complete",
            body: "Nice work. It is time for a break.",
        },
        SessionKind::ShortBreak => CompletionMessage {
            summary: "Short break complete",
            body: "Ready for another focus session?",
        },
        SessionKind::LongBreak => CompletionMessage {
            summary: "Long break complete",
            body: "Refreshed? Your next focus session is ready.",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{CompletionMessage, completion_message};
    use crate::SessionKind;

    #[test]
    fn completion_copy_is_specific_to_each_session_kind() {
        assert_eq!(
            completion_message(SessionKind::Focus),
            CompletionMessage {
                summary: "Focus session complete",
                body: "Nice work. It is time for a break.",
            }
        );
        assert_eq!(
            completion_message(SessionKind::ShortBreak),
            CompletionMessage {
                summary: "Short break complete",
                body: "Ready for another focus session?",
            }
        );
        assert_eq!(
            completion_message(SessionKind::LongBreak),
            CompletionMessage {
                summary: "Long break complete",
                body: "Refreshed? Your next focus session is ready.",
            }
        );
    }
}
