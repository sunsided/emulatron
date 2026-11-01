//! Speaker/tape output through the system audio device.
//!
//! The emulator renders the output pin's level changes into a square wave
//! (see `emulatron_machines::tape::AudioLog`); this module shapes it like
//! the web version does (high-pass for the idle pin's DC, a soft low-pass
//! for the piezo) and feeds it to a cpal output stream through a small
//! queue. Without the `audio` feature it compiles to a silent stub.

#[cfg(feature = "audio")]
mod imp {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};

    /// Keep at most this much audio queued (seconds) to bound the latency.
    const MAX_QUEUE_S: f32 = 0.2;

    pub struct Audio {
        _stream: cpal::Stream,
        queue: Arc<Mutex<VecDeque<f32>>>,
        error: Arc<Mutex<Option<String>>>,
        pub sample_rate: u32,
        // Filter state.
        hp_prev_in: f32,
        hp_prev_out: f32,
        lp: f32,
    }

    impl Audio {
        pub fn open() -> Result<Self, String> {
            let host = cpal::default_host();
            let device = host
                .default_output_device()
                .ok_or("no audio output device")?;
            let supported = device.default_output_config().map_err(|e| e.to_string())?;
            let format = supported.sample_format();
            let config = supported.config();
            let queue = Arc::new(Mutex::new(VecDeque::<f32>::new()));
            let error = Arc::new(Mutex::new(None));
            let stream = match format {
                SampleFormat::F32 => build::<f32>(&device, config, &queue, &error),
                SampleFormat::I16 => build::<i16>(&device, config, &queue, &error),
                SampleFormat::U16 => build::<u16>(&device, config, &queue, &error),
                SampleFormat::I32 => build::<i32>(&device, config, &queue, &error),
                f => return Err(format!("unsupported sample format {f:?}")),
            }?;
            stream.play().map_err(|e| e.to_string())?;
            Ok(Audio {
                _stream: stream,
                queue,
                error,
                sample_rate: config.sample_rate,
                hp_prev_in: 0.0,
                hp_prev_out: 0.0,
                lp: 0.0,
            })
        }

        /// Queue mono samples (-1..1), filtered like a piezo would sound.
        pub fn push(&mut self, samples: &[f32]) {
            let sr = self.sample_rate as f32;
            // One-pole high-pass at ~40 Hz and low-pass at ~6.5 kHz.
            let hp_a = 1.0 / (1.0 + 2.0 * std::f32::consts::PI * 40.0 / sr);
            let lp_k = 1.0 - (-2.0 * std::f32::consts::PI * 6500.0 / sr).exp();
            let mut q = self.queue.lock().unwrap();
            for &x in samples {
                let hp = hp_a * (self.hp_prev_out + x - self.hp_prev_in);
                self.hp_prev_in = x;
                self.hp_prev_out = hp;
                self.lp += lp_k * (hp - self.lp);
                q.push_back(self.lp * 0.8);
            }
            let max = (MAX_QUEUE_S * sr) as usize;
            if q.len() > max {
                let excess = q.len() - max;
                q.drain(..excess);
            }
        }

        /// The last stream error, if any (reported in the status bar).
        pub fn take_error(&self) -> Option<String> {
            self.error.lock().unwrap().take()
        }
    }

    fn build<T: SizedSample + FromSample<f32>>(
        device: &cpal::Device,
        config: StreamConfig,
        queue: &Arc<Mutex<VecDeque<f32>>>,
        error: &Arc<Mutex<Option<String>>>,
    ) -> Result<cpal::Stream, String> {
        let channels = config.channels as usize;
        let q = Arc::clone(queue);
        let err = Arc::clone(error);
        device
            .build_output_stream(
                config,
                move |data: &mut [T], _| {
                    let mut q = q.lock().unwrap();
                    for frame in data.chunks_mut(channels) {
                        let v = T::from_sample(q.pop_front().unwrap_or(0.0));
                        frame.fill(v);
                    }
                },
                move |e| *err.lock().unwrap() = Some(e.to_string()),
                None,
            )
            .map_err(|e| e.to_string())
    }
}

#[cfg(not(feature = "audio"))]
mod imp {
    pub struct Audio {
        pub sample_rate: u32,
    }

    impl Audio {
        pub fn open() -> Result<Self, String> {
            Err("built without the `audio` feature".into())
        }
        pub fn push(&mut self, _samples: &[f32]) {}
        pub fn take_error(&self) -> Option<String> {
            None
        }
    }
}

pub use imp::Audio;

#[cfg(all(test, feature = "audio"))]
mod tests {
    use super::Audio;

    /// Needs a real audio device: `cargo test -p emulatron-tui -- --ignored`.
    #[test]
    #[ignore]
    fn plays_a_short_tone() {
        let mut a = Audio::open().expect("audio device");
        let n = a.sample_rate as usize / 5;
        let period = a.sample_rate as usize / 440;
        let tone: Vec<f32> = (0..n)
            .map(|i| {
                if (i / (period / 2)).is_multiple_of(2) {
                    0.25
                } else {
                    -0.25
                }
            })
            .collect();
        a.push(&tone);
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(a.take_error().is_none());
    }
}
