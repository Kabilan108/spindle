//! A sink between librespot's player and the sound card that keeps a copy of what it plays.
//!
//! The player hands [`TapSink`] full-scale samples: the engine gives the player no volume
//! control, and the tap applies the volume itself after copying. Quiet or muted playback still
//! drives the visualizers.
//!
//! Samples reach the speaker well after `write`: librespot's rodio backend queues 26 packets
//! (about half a second) before `write` blocks, and the output stream adds its own buffer. The
//! tap keeps an estimate of the play head. While the queue is full, each blocked `write` returns
//! just as one packet finished playing, so the play head sits one queue behind the newest
//! sample; while it fills (a start or a seek) the play head runs at the sample rate from where
//! the first packet landed. [`Tap::window`] centres its window on what is heard now.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use librespot_playback::audio_backend::{Sink, SinkResult};
use librespot_playback::convert::Converter;
use librespot_playback::decoder::AudioPacket;
use librespot_playback::mixer::VolumeGetter;

use super::spectrum::{RATE, Spectrum, WINDOW};
use super::{BANDS, Source};

/// Mono frames kept: about 1.5 s, three times the backend's queue.
const RING: usize = 1 << 16;
/// librespot's rodio backend blocks `write` while more packets than this are queued.
const QUEUED_PACKETS: usize = 26;
/// A `write` that took longer than this waited on a full queue; an open one returns at once.
const BLOCKED: Duration = Duration::from_millis(4);
/// The backend polls its queue every 10 ms, so a blocked `write` returns on average 5 ms into
/// the next packet.
const POLL_SLACK: f64 = 0.005;
/// From rodio's mixer to the speaker. Through PipeWire's ALSA plugin the stream asks for 1102
/// frames (25 ms at 44.1 kHz) and the graph runs 1024-frame quanta at 48 kHz (21 ms), plus the
/// card's own headroom: about 45 to 65 ms.
const OUTPUT_LATENCY: f64 = 0.055;
/// A frame drawn now is on screen about a frame later.
const DISPLAY_LEAD: f64 = 0.016;
/// A new play-head measurement moves the estimate this far toward it, smoothing out the
/// backend's 10 ms polling.
const SETTLE: f64 = 0.25;

#[derive(Clone, Copy)]
struct Head {
    frames: f64,
    at: Instant,
}

impl Head {
    fn at(self, now: Instant) -> f64 {
        self.frames + now.saturating_duration_since(self.at).as_secs_f64() * f64::from(RATE)
    }
}

struct Ring {
    samples: Box<[f32]>,
    written: u64,
    /// Frames in each of the last packets, newest last.
    packets: VecDeque<u32>,
    /// `None` while the output is stopped.
    head: Option<Head>,
}

pub(crate) struct Tap {
    ring: Mutex<Ring>,
}

/// The process's one tap: the player that writes into it and the analysis that reads it are
/// built in different places, and both outlive any one connection.
pub(crate) fn tap() -> Arc<Tap> {
    static TAP: OnceLock<Arc<Tap>> = OnceLock::new();
    TAP.get_or_init(Tap::new).clone()
}

impl Tap {
    pub(crate) fn new() -> Arc<Self> {
        let ring = Ring { samples: vec![0.0; RING].into_boxed_slice(), written: 0, packets: VecDeque::new(), head: None };
        Arc::new(Self { ring: Mutex::new(ring) })
    }

    fn ring(&self) -> std::sync::MutexGuard<'_, Ring> {
        self.ring.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn push(&self, interleaved: &[f64]) -> u32 {
        let mut ring = self.ring();
        let frames = interleaved.len() / 2;
        for pair in interleaved.as_chunks::<2>().0 {
            let at = (ring.written % RING as u64) as usize;
            ring.samples[at] = ((pair[0] + pair[1]) * 0.5) as f32;
            ring.written += 1;
        }
        ring.packets.push_back(frames as u32);
        while ring.packets.len() > QUEUED_PACKETS {
            ring.packets.pop_front();
        }
        frames as u32
    }

    /// The backend took a packet of `frames`: `write` was called at `began` and returned at
    /// `now`.
    fn played(&self, frames: u32, began: Instant, now: Instant) {
        let blocked = now - began > BLOCKED;
        let mut ring = self.ring();
        if blocked {
            let queued: u64 = ring.packets.iter().map(|n| u64::from(*n)).sum();
            let measured = ring.written.saturating_sub(queued) as f64 + POLL_SLACK * f64::from(RATE);
            let frames = match ring.head {
                Some(head) => {
                    let predicted = head.at(now);
                    if (measured - predicted).abs() < 0.1 * f64::from(RATE) { predicted + SETTLE * (measured - predicted) } else { measured }
                }
                None => measured,
            };
            ring.head = Some(Head { frames, at: now });
        } else if ring.head.is_none() {
            ring.head = Some(Head { frames: ring.written.saturating_sub(u64::from(frames)) as f64, at: began });
        }
    }

    /// The backend has played everything it was given: the estimate should be at the newest
    /// sample, and how far off it is shows how well the visuals track the sound.
    fn stopped(&self) {
        let mut ring = self.ring();
        if let Some(head) = ring.head {
            let off_ms = (head.at(Instant::now()) - ring.written as f64) / f64::from(RATE) * 1000.0;
            log::debug!("play head estimate {off_ms:+.1} ms from the output when it drained");
        }
        ring.head = None;
        ring.packets.clear();
    }

    /// The mono window centred on what the listener hears now. False while nothing is heard:
    /// the output is stopped or has played everything it was given.
    fn window(&self, now: Instant, out: &mut [f32; WINDOW]) -> bool {
        let ring = self.ring();
        let Some(head) = ring.head else { return false };
        let heard = head.at(now) - (OUTPUT_LATENCY - DISPLAY_LEAD) * f64::from(RATE);
        let written = ring.written as f64;
        if heard >= written || heard < 0.0 {
            return false;
        }
        let end = (heard as u64 + WINDOW as u64 / 2).min(ring.written);
        let oldest = ring.written.saturating_sub(RING as u64);
        let Some(start) = end.checked_sub(WINDOW as u64).filter(|start| *start >= oldest) else { return false };
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = ring.samples[((start + i as u64) % RING as u64) as usize];
        }
        true
    }
}

/// Wraps the real output: copies each packet into the tap, applies the volume, passes it on.
pub(crate) struct TapSink {
    inner: Box<dyn Sink>,
    tap: Arc<Tap>,
    volume: Box<dyn VolumeGetter + Send>,
}

impl TapSink {
    pub(crate) fn new(inner: Box<dyn Sink>, tap: Arc<Tap>, volume: Box<dyn VolumeGetter + Send>) -> Self {
        Self { inner, tap, volume }
    }
}

impl Sink for TapSink {
    fn start(&mut self) -> SinkResult<()> {
        self.inner.start()
    }

    fn stop(&mut self) -> SinkResult<()> {
        let stopped = self.inner.stop();
        self.tap.stopped();
        stopped
    }

    fn write(&mut self, mut packet: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
        let mut frames = 0;
        if let AudioPacket::Samples(samples) = &mut packet {
            frames = self.tap.push(samples);
            let volume = self.volume.attenuation_factor();
            if volume < 1.0 {
                for sample in samples.iter_mut() {
                    *sample *= volume;
                }
            }
        }
        let began = Instant::now();
        self.inner.write(packet, converter)?;
        if frames > 0 {
            self.tap.played(frames, began, Instant::now());
        }
        Ok(())
    }
}

/// The analysis side of the tap.
pub(crate) struct TapSource {
    tap: Arc<Tap>,
    spectrum: Spectrum,
    window: Box<[f32; WINDOW]>,
}

impl TapSource {
    pub(crate) fn new(tap: Arc<Tap>) -> Self {
        Self { tap, spectrum: Spectrum::default(), window: Box::new([0.0; WINDOW]) }
    }
}

impl Source for TapSource {
    fn bands(&mut self, _dt: f64, _playing: bool, out: &mut [f32; BANDS]) -> bool {
        if !self.tap.window(Instant::now(), &mut self.window) {
            return false;
        }
        self.spectrum.bands(&self.window, out);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seconds(frames: usize) -> Duration {
        Duration::from_secs_f64(frames as f64 / f64::from(RATE))
    }

    #[test]
    fn nothing_is_heard_before_the_first_packet_or_after_a_stop() {
        let tap = Tap::new();
        let mut window = [0.0; WINDOW];
        let start = Instant::now();
        assert!(!tap.window(start, &mut window));
        let frames = tap.push(&vec![0.25; 2 * 4096]);
        tap.played(frames, start, start);
        assert!(tap.window(start + Duration::from_millis(60), &mut window), "60 ms into a 93 ms packet");
        assert!(window.iter().all(|s| *s == 0.25));
        assert!(!tap.window(start + Duration::from_millis(200), &mut window), "played out");
        tap.stopped();
        assert!(!tap.window(start + Duration::from_millis(60), &mut window));
    }

    #[test]
    fn a_full_queue_puts_the_play_head_one_queue_behind() {
        let tap = Tap::new();
        let start = Instant::now();
        let mut now = start;
        for packet in 0..60 {
            let frames = tap.push(&[0.0; 2 * 1024]);
            let began = now;
            if packet >= QUEUED_PACKETS {
                now += seconds(1024);
            }
            tap.played(frames, began, now);
        }
        let ring = tap.ring();
        let head = ring.head.expect("playing").at(now);
        let expected = (ring.written - (QUEUED_PACKETS * 1024) as u64) as f64 + POLL_SLACK * f64::from(RATE);
        assert!((head - expected).abs() < 0.002 * f64::from(RATE), "play head {head:.0}, expected {expected:.0}");
    }
}
