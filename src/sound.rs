//! Best-effort sound playback boundary.

use std::{
    fs::File,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use rodio::{Decoder, MixerDeviceSink, Player, Source};

const RETRY_DELAY: Duration = Duration::from_secs(2);

/// Plays user-selected sound files outside the application domain.
pub trait SoundPlayer {
    /// Plays a one-shot completion sound without blocking the event loop.
    fn play_completion(&mut self, file: &Path);

    /// Stops any still-playing one-shot completion sound.
    fn stop_completion(&mut self);

    /// Starts a Focus loop or resumes the already installed loop.
    fn start_or_resume_focus(&mut self, file: &Path);

    /// Pauses the installed Focus loop, retaining its position.
    fn pause_focus(&mut self);

    /// Stops and removes the installed Focus loop.
    fn stop_focus(&mut self);
}

/// Cross-platform audio-device adapter backed by `rodio`.
#[derive(Default)]
pub struct FileSoundPlayer {
    stream: Option<MixerDeviceSink>,
    focus_sink: Option<Player>,
    completion_sink: Option<Player>,
    stream_failed: Arc<AtomicBool>,
    retry_after: Option<Instant>,
    focus_retry_after: Option<Instant>,
}

impl SoundPlayer for FileSoundPlayer {
    fn play_completion(&mut self, file: &Path) {
        let Ok(input) = File::open(file) else {
            return;
        };
        let Ok(source) = Decoder::try_from(input) else {
            return;
        };
        self.ensure_stream();
        self.stop_completion();
        let Some(stream) = &self.stream else {
            return;
        };
        let sink = Player::connect_new(stream.mixer());
        sink.append(source.take_duration(std::time::Duration::from_secs(5)));
        self.completion_sink = Some(sink);
    }

    fn stop_completion(&mut self) {
        if let Some(sink) = self.completion_sink.take() {
            sink.stop();
        }
    }

    fn start_or_resume_focus(&mut self, file: &Path) {
        self.discard_failed_stream();
        if let Some(sink) = &self.focus_sink {
            sink.play();
            return;
        }
        let now = Instant::now();
        if self
            .focus_retry_after
            .is_some_and(|deadline| now < deadline)
            || (self.stream.is_none() && self.retry_after.is_some_and(|deadline| now < deadline))
        {
            return;
        }
        // Back off file I/O and decoder probing as well as device creation.
        self.focus_retry_after = Some(now + RETRY_DELAY);
        let Ok(input) = File::open(file) else {
            return;
        };
        let Ok(source) = Decoder::try_from(input) else {
            return;
        };
        self.ensure_stream();
        let Some(stream) = &self.stream else {
            return;
        };
        let sink = Player::connect_new(stream.mixer());
        sink.append(source.repeat_infinite());
        self.focus_sink = Some(sink);
        self.focus_retry_after = None;
    }

    fn pause_focus(&mut self) {
        if let Some(sink) = &self.focus_sink {
            sink.pause();
        }
    }

    fn stop_focus(&mut self) {
        self.focus_retry_after = None;
        if let Some(sink) = self.focus_sink.take() {
            sink.stop();
        }
    }
}

impl FileSoundPlayer {
    /// Recover active focus audio after a device failure, without replaying alerts.
    pub fn maintain(&mut self, active_focus_file: Option<&Path>) {
        self.discard_failed_stream();
        if self.focus_sink.is_none()
            && let Some(file) = active_focus_file
        {
            self.start_or_resume_focus(file);
        }
    }

    fn discard_failed_stream(&mut self) {
        if self.stream_failed.swap(false, Ordering::Relaxed) {
            self.stop_focus();
            self.stop_completion();
            self.stream = None;
        }
    }

    fn ensure_stream(&mut self) {
        self.discard_failed_stream();
        if self.stream.is_some() {
            return;
        }
        let now = Instant::now();
        if self.retry_after.is_some_and(|deadline| now < deadline) {
            return;
        }
        self.retry_after = Some(now + RETRY_DELAY);
        // Each stream gets its own flag so late callbacks cannot invalidate a replacement.
        let failed = Arc::new(AtomicBool::new(false));
        let callback_flag = Arc::clone(&failed);
        let Ok(mut stream) = rodio::DeviceSinkBuilder::from_default_device().and_then(|builder| {
            builder
                .with_error_callback(move |_| {
                    // Never write to stderr from the audio thread: it corrupts the TUI.
                    callback_flag.store(true, Ordering::Relaxed);
                })
                .open_sink_or_fallback()
        }) else {
            return;
        };
        self.stream_failed = failed;
        stream.log_on_drop(false);
        self.stream = Some(stream);
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{FileSoundPlayer, SoundPlayer};

    #[test]
    fn device_failure_discards_players_even_when_focus_is_paused() {
        let (focus, _focus_output) = rodio::Player::new();
        let (completion, _completion_output) = rodio::Player::new();
        focus.pause();
        let mut player = FileSoundPlayer {
            focus_sink: Some(focus),
            completion_sink: Some(completion),
            ..Default::default()
        };
        player
            .stream_failed
            .store(true, std::sync::atomic::Ordering::Relaxed);

        player.maintain(None);

        assert!(player.focus_sink.is_none());
        assert!(player.completion_sink.is_none());
        assert!(
            !player
                .stream_failed
                .load(std::sync::atomic::Ordering::Relaxed)
        );
    }

    #[test]
    fn healthy_paused_focus_is_retained() {
        let (focus, _output) = rodio::Player::new();
        focus.pause();
        let mut player = FileSoundPlayer {
            focus_sink: Some(focus),
            ..Default::default()
        };

        player.maintain(None);

        assert!(player.focus_sink.as_ref().unwrap().is_paused());
    }

    #[test]
    fn device_open_attempts_observe_retry_delay() {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let mut player = FileSoundPlayer {
            retry_after: Some(deadline),
            ..Default::default()
        };

        player.ensure_stream();

        assert!(player.stream.is_none());
        assert_eq!(player.retry_after, Some(deadline));
    }

    #[test]
    fn focus_recovery_backs_off_missing_files_and_stop_allows_a_fresh_attempt() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("missing.wav");
        let mut player = FileSoundPlayer::default();

        player.maintain(Some(&file));
        assert!(player.focus_retry_after.is_some());
        assert!(player.stream.is_none());

        let deadline = Instant::now() + Duration::from_secs(60);
        player.focus_retry_after = Some(deadline);
        player.maintain(Some(&file));
        assert_eq!(player.focus_retry_after, Some(deadline));

        player.stop_focus();
        assert!(player.focus_retry_after.is_none());
        player.maintain(Some(&file));
        assert!(player.focus_retry_after.is_some());
        assert_ne!(player.focus_retry_after, Some(deadline));
    }

    #[test]
    fn focus_recovery_waits_for_device_backoff_before_attempting_file() {
        let mut player = FileSoundPlayer {
            retry_after: Some(Instant::now() + Duration::from_secs(60)),
            ..Default::default()
        };

        player.maintain(Some(std::path::Path::new("missing.wav")));

        assert!(player.focus_retry_after.is_none());
        assert!(player.stream.is_none());
    }

    #[test]
    fn missing_files_are_ignored_without_opening_an_audio_device() {
        let mut player = FileSoundPlayer::default();

        player.play_completion(std::path::Path::new("a-file-that-does-not-exist.wav"));

        assert!(player.stream.is_none());
    }

    #[test]
    fn missing_focus_files_are_ignored_without_opening_an_audio_device() {
        let mut player = FileSoundPlayer::default();

        player.start_or_resume_focus(std::path::Path::new("a-file-that-does-not-exist.wav"));

        assert!(player.stream.is_none());
        assert!(player.focus_sink.is_none());
    }
}
