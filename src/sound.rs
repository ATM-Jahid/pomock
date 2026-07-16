//! Best-effort sound playback boundary.

use std::{fs::File, path::Path};

use rodio::{OutputStream, OutputStreamBuilder, play};

/// Plays user-selected sound files outside the application domain.
pub trait SoundPlayer {
    /// Starts playing a sound file without blocking the event loop.
    fn play(&mut self, file: &Path);
}

/// Cross-platform audio-device adapter backed by `rodio`.
#[derive(Default)]
pub struct FileSoundPlayer {
    stream: Option<OutputStream>,
}

impl SoundPlayer for FileSoundPlayer {
    fn play(&mut self, file: &Path) {
        let Ok(input) = File::open(file) else {
            return;
        };
        if self.stream.is_none() {
            let Ok(mut stream) = OutputStreamBuilder::open_default_stream() else {
                return;
            };
            stream.log_on_drop(false);
            self.stream = Some(stream);
        }
        if let Some(stream) = &self.stream
            && let Ok(sink) = play(stream.mixer(), input)
        {
            sink.detach();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FileSoundPlayer, SoundPlayer};

    #[test]
    fn missing_files_are_ignored_without_opening_an_audio_device() {
        let mut player = FileSoundPlayer::default();

        player.play(std::path::Path::new("a-file-that-does-not-exist.wav"));

        assert!(player.stream.is_none());
    }
}
