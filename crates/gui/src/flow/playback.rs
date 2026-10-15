//! Time-driven playback engine for Flow mode: playhead, transport states,
//! and the pure step-index math shared by the canvas, scrub slider, and
//! sidebar readout. OWNER: TL-GRAPH (GRAPH-1).
use std::time::Duration;

use eframe::egui;

use crate::theme;

/// Speed presets offered by the transport control (cycle order).
pub const SPEEDS: [f32; 3] = [0.5, 1.0, 2.0];

const MS_PER_S: f64 = 1000.0;

/// Seconds of wall time each step dwells — sourced from the motion token.
pub fn step_secs() -> f64 {
    theme::STEP_DWELL_MS as f64 / MS_PER_S
}

/// Total duration of an `steps`-long sequence, in seconds.
pub fn total_secs(steps: usize) -> f64 {
    steps as f64 * step_secs()
}

/// Current step index + intra-step progress in [0,1] for a playhead. A
/// past-the-end playhead clamps to the last step at progress 1.0 so the final
/// frame renders fully lit; an empty sequence is (0, 0.0).
pub fn step_pos(playhead: f64, steps: usize) -> (usize, f32) {
    if steps == 0 {
        return (0, 0.0);
    }
    let t = playhead.max(0.0) / step_secs();
    let idx = t.floor();
    if idx >= steps as f64 {
        (steps - 1, 1.0)
    } else {
        (idx as usize, (t - idx) as f32)
    }
}

/// Looping pulse phase in [0,1) for the connector dot, eased with the theme
/// smoothstep. `now` is the egui frame clock in seconds.
pub fn pulse_phase(now: f64) -> f32 {
    let period = theme::PULSE_MS as f64 / MS_PER_S;
    let t = now.max(0.0).rem_euclid(period) / period;
    theme::ease_in_out(t as f32)
}

/// Transport state. The playhead is wall-clock driven (frame time × speed),
/// never frame-count driven, so playback speed stays honest on slow machines.
#[derive(Debug)]
pub struct Playback {
    pub playing: bool,
    pub looping: bool,
    /// One of [`SPEEDS`].
    pub speed: f32,
    /// Seconds along the current sequence.
    pub playhead: f64,
    /// Previous frame-clock reading (None while parked).
    last_time: Option<f64>,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            playing: false,
            looping: false,
            speed: 1.0,
            playhead: 0.0,
            last_time: None,
        }
    }
}

impl Playback {
    /// Advance from the egui frame clock. Call once per frame regardless of
    /// transport state so a paused playhead never accrues stale time; dt is
    /// capped at one dwell so a stalled frame cannot skip steps.
    pub fn advance(&mut self, ctx: &egui::Context, steps: usize) {
        let now = ctx.input(|i| i.time);
        let dt = self
            .last_time
            .map_or(0.0, |prev| (now - prev).clamp(0.0, step_secs()));
        self.last_time = Some(now);
        if !self.playing || steps == 0 {
            return;
        }
        self.playhead += dt * f64::from(self.speed);
        let total = total_secs(steps);
        if self.playhead >= total {
            if self.looping {
                self.playhead %= total;
            } else {
                // Park fully lit on the last step instead of flickering off.
                self.playhead = total;
                self.playing = false;
            }
        }
    }

    /// While (and only while) playing, ask egui for the next repaint in time
    /// for the following pulse tick. An idle transport requests nothing: no
    /// repaint, no battery drain.
    pub fn request_repaint_while_playing(&self, ctx: &egui::Context, steps: usize) {
        if self.playing && steps > 0 {
            ctx.request_repaint_after(Duration::from_millis(theme::PULSE_MS / 4));
        }
    }

    /// Play — or pause; starting while parked at the end rewinds first.
    pub fn toggle_play(&mut self, steps: usize) {
        if self.playing {
            self.playing = false;
            return;
        }
        if steps == 0 {
            return;
        }
        if self.playhead >= total_secs(steps) {
            self.playhead = 0.0;
        }
        self.last_time = None;
        self.playing = true;
    }

    /// Pause and advance one step; clamps at the end of the sequence.
    pub fn step_forward(&mut self, steps: usize) {
        if steps == 0 {
            return;
        }
        self.playing = false;
        let (idx, _) = step_pos(self.playhead, steps);
        self.playhead = ((idx + 1) as f64 * step_secs()).min(total_secs(steps));
    }

    /// Pause and rewind to the start of the current step, or of the previous
    /// one when already past its midpoint; clamps at step 0.
    pub fn step_back(&mut self, steps: usize) {
        if steps == 0 {
            return;
        }
        self.playing = false;
        let (idx, progress) = step_pos(self.playhead, steps);
        let target = if progress < 0.5 {
            idx.saturating_sub(1)
        } else {
            idx
        };
        self.playhead = target as f64 * step_secs();
    }

    /// Pause and seek to the start of `step` (clamped to the sequence).
    pub fn goto_step(&mut self, step: usize, steps: usize) {
        if steps == 0 {
            return;
        }
        self.playing = false;
        self.playhead = step.min(steps - 1) as f64 * step_secs();
    }

    /// Cycle 0.5x → 1x → 2x → 0.5x.
    pub fn cycle_speed(&mut self) {
        let i = SPEEDS.iter().position(|s| *s == self.speed).unwrap_or(1);
        self.speed = SPEEDS[(i + 1) % SPEEDS.len()];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_pos_walks_the_dwell_grid() {
        let d = step_secs();
        assert_eq!(step_pos(0.0, 3), (0, 0.0));
        let (i, p) = step_pos(d * 0.5, 3);
        assert_eq!(i, 0);
        assert!((p - 0.5).abs() < 1e-6);
        assert_eq!(step_pos(d, 3).0, 1);
        // Past-the-end playheads clamp onto the last step, fully lit.
        assert_eq!(step_pos(d * 10.0, 3), (2, 1.0));
    }

    #[test]
    fn step_pos_handles_empty_and_single_step_sequences() {
        assert_eq!(step_pos(5.0, 0), (0, 0.0));
        assert_eq!(step_pos(5.0, 1), (0, 1.0));
    }

    #[test]
    fn pulse_phase_is_bounded_and_starts_at_zero() {
        for now in [0.0, 0.15, 0.3, 0.75, 12.34] {
            let t = pulse_phase(now);
            assert!((0.0..=1.0).contains(&t), "phase out of range at {now}");
        }
        assert_eq!(pulse_phase(0.0), 0.0);
    }

    #[test]
    fn speed_cycles_through_presets() {
        let mut pb = Playback::default();
        assert_eq!(pb.speed, 1.0);
        pb.cycle_speed();
        assert_eq!(pb.speed, 2.0);
        pb.cycle_speed();
        assert_eq!(pb.speed, 0.5);
    }

    #[test]
    fn stepping_clamps_at_the_sequence_ends() {
        let mut pb = Playback::default();
        pb.step_back(3);
        assert_eq!(pb.playhead, 0.0, "step back clamps at the first step");
        pb.step_forward(3);
        pb.step_forward(3);
        pb.step_forward(3);
        assert_eq!(pb.playhead, total_secs(3), "step forward clamps at the end");
        assert!(!pb.playing, "manual stepping always pauses");
    }
}
