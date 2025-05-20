use crate::configuration::SoundSettings;
use sdl2::{
    Sdl,
    audio::{AudioCallback, AudioDevice, AudioSpecDesired},
};

/// An SDL2 Audio Device the represents a speaker that can be played through the actual device
/// speaker(s) when the Chip-8 VM sets the buzzer enable flag.
pub struct Buzzer {
    phase_increment: f32, // Essentially what tone (in Hz) the generated waveform will play at
    phase: f32,
    volume: f32, // The max intensity (amplitude) the generated wave will reach
}

impl Buzzer {
    /// Generates an AudioDevice that plays a square wave consisting of a 44.1 kHz sample rate with
    /// a user specified tone at a user specified volume.
    pub fn initialize(
        sdl_context: &Sdl,
        settings: &SoundSettings,
    ) -> anyhow::Result<AudioDevice<Buzzer>> {
        let audio_subsystem = sdl_context.audio().map_err(anyhow::Error::msg)?;

        let desired_spec = AudioSpecDesired {
            freq: Some(44100), // 44.1 kHz sample rate (CD quality)
            channels: Some(1), // Mono sound.
            samples: None, // Use the fallback sample size by supplying None as it doesn't matter.
        };
        audio_subsystem
            .open_playback(None, &desired_spec, |spec| {
                // Initialize the Audio Callback
                Buzzer {
                    phase_increment: settings.tone / (spec.freq as f32),
                    phase: 0.0,
                    volume: settings.volume / 20.0,
                }
            })
            .map_err(anyhow::Error::msg)
    }
}

impl AudioCallback for Buzzer {
    type Channel = f32;

    fn callback(&mut self, out: &mut [f32]) {
        // Generate a square wave for that "cheap motherboard speaker" kind of sound
        for x in out.iter_mut() {
            *x = if self.phase <= 0.5 {
                self.volume
            } else {
                -self.volume
            };
            self.phase = (self.phase + self.phase_increment) % 1.0;
        }
    }
}
