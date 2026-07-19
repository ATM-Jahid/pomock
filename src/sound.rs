//! Best-effort sound playback boundary.

use std::{fs::File, path::Path};

use rodio::{Decoder, OutputStream, OutputStreamBuilder, Sink, Source};

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
    stream: Option<OutputStream>,
    focus_sink: Option<Sink>,
    completion_sink: Option<Sink>,
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
        let sink = Sink::connect_new(stream.mixer());
        sink.append(source.take_duration(std::time::Duration::from_secs(5)));
        self.completion_sink = Some(sink);
    }

    fn stop_completion(&mut self) {
        if let Some(sink) = self.completion_sink.take() {
            sink.stop();
        }
    }

    fn start_or_resume_focus(&mut self, file: &Path) {
        if let Some(sink) = &self.focus_sink {
            sink.play();
            return;
        }
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
        let sink = Sink::connect_new(stream.mixer());
        sink.append(source.repeat_infinite());
        self.focus_sink = Some(sink);
    }

    fn pause_focus(&mut self) {
        if let Some(sink) = &self.focus_sink {
            sink.pause();
        }
    }

    fn stop_focus(&mut self) {
        if let Some(sink) = self.focus_sink.take() {
            sink.stop();
        }
    }
}

impl FileSoundPlayer {
    fn ensure_stream(&mut self) {
        if self.stream.is_some() {
            return;
        }
        let Ok(mut stream) = OutputStreamBuilder::open_default_stream() else {
            return;
        };
        stream.log_on_drop(false);
        self.stream = Some(stream);
    }
}

#[cfg(test)]
mod tests {
    use super::{FileSoundPlayer, SoundPlayer};

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
