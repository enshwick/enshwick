//! `FramelessQOA`, the game's compressed sound format: the Quite OK Audio
//! codec without its file and frame headers, so a channel's predictor runs
//! from the first sample to the last.
//!
//! One little-endian `u64` holds 20 samples of one channel: bits 63..60 the
//! scale factor, then a 3-bit residual per sample from bit 59 down; a short
//! last slice is left-aligned with zero spare bits. Slice `i` of channel `c`
//! is the `u64` at `i * channels + c`. Every channel starts from history 0
//! and weights `{0, 0, -8192, 16384}`. The game's encoder is the reference
//! `qoa.h` search without the weights penalty `qoa.h` added in 2024: with it
//! long multichannel sounds encode to other slices (client 26430625).
//!
//! The crates that write QOA files write their headers and reset the
//! predictor per frame, so none writes this format.

/// Samples per slice.
pub(crate) const SLICE: usize = 20;

const SCALE: [i32; 16] = [
    1, 7, 21, 45, 84, 138, 211, 304, 421, 562, 731, 928, 1157, 1419, 1715, 2048,
];
const RECIPROCAL: [i32; 16] = reciprocals();
const QUANT: [u8; 17] = [7, 7, 7, 5, 5, 3, 3, 1, 0, 0, 2, 2, 4, 4, 6, 6, 6];
/// `round(scale * {0.75, -0.75, 2.5, -2.5, 4.5, -4.5, 7, -7})`, the table
/// the client holds.
const DEQUANT: [[i32; 8]; 16] = dequant();

const fn reciprocals() -> [i32; 16] {
    let mut out = [0; 16];
    let mut s = 0;
    while s < 16 {
        out[s] = ((1 << 16) + SCALE[s] - 1) / SCALE[s];
        s += 1;
    }
    out
}

const fn dequant() -> [[i32; 8]; 16] {
    // The factors times 4, so the table rounds in integers: 3, 10, 18, 28.
    const QUARTERS: [i32; 4] = [3, 10, 18, 28];
    let mut out = [[0; 8]; 16];
    let mut s = 0;
    while s < 16 {
        let mut q = 0;
        while q < 4 {
            // Half away from zero, as `round` does: every product is positive.
            let value = (SCALE[s] * QUARTERS[q] + 2) / 4;
            out[s][2 * q] = value;
            out[s][2 * q + 1] = -value;
            q += 1;
        }
        s += 1;
    }
    out
}

/// A channel's predictor: its last four samples and their weights.
#[derive(Clone, Copy)]
struct Lms {
    history: [i32; 4],
    weights: [i32; 4],
}

impl Lms {
    const START: Self = Self {
        history: [0; 4],
        weights: [0, 0, -(1 << 13), 1 << 14],
    };

    // The reference runs in C `int`; wrapping keeps a pathological input
    // from panicking where the game's encoder would wrap.
    fn predict(&self) -> i32 {
        let mut sum = 0i32;
        for i in 0..4 {
            sum = sum.wrapping_add(self.history[i].wrapping_mul(self.weights[i]));
        }
        sum >> 13
    }

    fn update(&mut self, sample: i32, residual: i32) {
        let delta = residual >> 4;
        for i in 0..4 {
            let step = if self.history[i] < 0 { -delta } else { delta };
            self.weights[i] = self.weights[i].wrapping_add(step);
        }
        self.history = [self.history[1], self.history[2], self.history[3], sample];
    }
}

/// `v / scale` rounded away from zero, as the reference divides.
fn div(v: i32, scale: usize) -> i32 {
    let n = (v.wrapping_mul(RECIPROCAL[scale]).wrapping_add(1 << 15)) >> 16;
    n + (v.signum() - n.signum())
}

fn clamp16(v: i32) -> i32 {
    v.clamp(i32::from(i16::MIN), i32::from(i16::MAX))
}

/// The encoded length of `frames` frames of `channels` channels, in bytes.
#[must_use]
pub(crate) const fn encoded_len(frames: usize, channels: usize) -> usize {
    frames.div_ceil(SLICE) * channels * 8
}

/// `samples`, `channels` interleaved per frame, encoded as `FramelessQOA`.
///
/// # Panics
///
/// Never for `channels` 1 to 8: a trailing partial frame is dropped.
#[must_use]
pub(crate) fn encode(samples: &[i16], channels: usize) -> Vec<u8> {
    let frames = samples.len() / channels.max(1);
    let slices = frames.div_ceil(SLICE);
    let mut out = vec![0u8; encoded_len(frames, channels)];
    for channel in 0..channels {
        let mut lms = Lms::START;
        let mut previous = 0usize;
        for slice in 0..slices {
            let first = slice * SLICE;
            let len = SLICE.min(frames - first);
            let sample = |k: usize| i32::from(samples[(first + k) * channels + channel]);
            let mut best = (u64::MAX, 0u64, lms, 0usize);
            for n in 0..16 {
                let scale = (n + previous) % 16;
                let mut trial = lms;
                let mut word = u64::try_from(scale).unwrap_or(0);
                let mut rank = 0u64;
                for k in 0..len {
                    let wanted = sample(k);
                    let predicted = trial.predict();
                    let scaled = div(wanted.wrapping_sub(predicted), scale).clamp(-8, 8);
                    // Clamped to -8..=8 above, so the index is 0..=16.
                    let quantized = QUANT[usize::try_from(scaled + 8).unwrap_or(8)];
                    let residual = DEQUANT[scale][usize::from(quantized)];
                    let made = clamp16(predicted.wrapping_add(residual));
                    let error = i64::from(wanted - made);
                    rank += error.unsigned_abs().pow(2);
                    if rank > best.0 {
                        break;
                    }
                    trial.update(made, residual);
                    word = (word << 3) | u64::from(quantized);
                }
                // An equal error keeps the earlier scale factor, as the
                // reference does.
                if rank < best.0 {
                    best = (rank, word, trial, scale);
                }
            }
            let (_, word, next, scale) = best;
            lms = next;
            previous = scale;
            let at = (slice * channels + channel) * 8;
            let word = word << ((SLICE - len) * 3);
            out[at..at + 8].copy_from_slice(&word.to_le_bytes());
        }
    }
    out
}

/// The samples of `data`, `frames` frames of `channels` channels, as the
/// client decodes them, interleaved per frame.
#[cfg(test)]
pub(crate) fn decode(data: &[u8], frames: usize, channels: usize) -> Vec<i16> {
    let mut out = vec![0i16; frames * channels];
    for channel in 0..channels {
        let mut lms = Lms::START;
        for slice in 0..frames.div_ceil(SLICE) {
            let at = (slice * channels + channel) * 8;
            let word = u64::from_le_bytes(data[at..at + 8].try_into().unwrap());
            let scale = usize::try_from(word >> 60).unwrap();
            for k in 0..SLICE.min(frames - slice * SLICE) {
                let quantized = usize::try_from((word >> (57 - 3 * k)) & 7).unwrap();
                let predicted = lms.predict();
                let residual = DEQUANT[scale][quantized];
                let made = clamp16(predicted + residual);
                lms.update(made, residual);
                out[(slice * SLICE + k) * channels + channel] = i16::try_from(made).unwrap();
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    /// A deterministic test signal: a tone of `period` frames per cycle in
    /// each channel, a different phase per channel, plus a little noise.
    pub(crate) fn tone(frames: usize, channels: usize, period: usize) -> Vec<i16> {
        let mut noise = 0x2545_f491_u32;
        let mut out = Vec::with_capacity(frames * channels);
        for frame in 0..frames {
            for channel in 0..channels {
                noise = noise.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                let phase = (frame + channel * period / 4) % period;
                let saw = i32::try_from(phase * 2000 / period).unwrap() - 1000;
                let tri = 1000 - saw.abs() * 2;
                let jitter = i32::try_from(noise >> 26).unwrap() - 32;
                out.push(i16::try_from(tri * 12 + jitter * 8).unwrap());
            }
        }
        out
    }

    /// The signal-to-noise ratio of `made` against `wanted`, in decibels.
    fn snr(wanted: &[i16], made: &[i16]) -> f64 {
        let power: f64 = wanted.iter().map(|&s| f64::from(s).powi(2)).sum();
        let noise: f64 = wanted
            .iter()
            .zip(made)
            .map(|(&w, &m)| (f64::from(w) - f64::from(m)).powi(2))
            .sum();
        10.0 * (power / noise.max(1.0)).log10()
    }

    #[test]
    fn the_tables_are_the_specification_s() {
        assert_eq!(RECIPROCAL[0], 65536);
        assert_eq!(RECIPROCAL[15], 32);
        assert_eq!(DEQUANT[0], [1, -1, 3, -3, 5, -5, 7, -7]);
        assert_eq!(
            DEQUANT[15],
            [1536, -1536, 5120, -5120, 9216, -9216, 14336, -14336]
        );
        for (s, row) in DEQUANT.iter().enumerate() {
            for (q, factor) in [0.75, -0.75, 2.5, -2.5, 4.5, -4.5, 7.0, -7.0]
                .iter()
                .enumerate()
            {
                let rounded = (f64::from(SCALE[s]) * factor).round();
                assert_eq!(f64::from(row[q]), rounded, "{s} {q}");
            }
        }
    }

    /// A sine of `hz` at half of full scale in each channel, a quarter
    /// turn apart, at the game's rate.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "within half of the i16 range"
    )]
    fn sine(frames: usize, channels: usize, hz: f64) -> Vec<i16> {
        let step = core::f64::consts::TAU * hz / 48_000.0;
        (0..frames * channels)
            .map(|i| {
                let (frame, channel) = (i / channels, i % channels);
                let turn = f64::from(u32::try_from(frame).unwrap()) * step
                    + f64::from(u32::try_from(channel).unwrap()) * core::f64::consts::FRAC_PI_2;
                (turn.sin() * 16_000.0) as i16
            })
            .collect()
    }

    #[test]
    fn generated_tones_round_trip_above_40_db() {
        for (channels, frames, hz) in [(1, 48_000, 440.0), (2, 30_011, 1_000.0), (6, 4_001, 200.0)]
        {
            let wanted = sine(frames, channels, hz);
            let data = encode(&wanted, channels);
            assert_eq!(data.len(), encoded_len(frames, channels));
            let made = decode(&data, frames, channels);
            let ratio = snr(&wanted, &made);
            assert!(ratio > 40.0, "{channels} channels: {ratio} dB");
        }
    }

    #[test]
    fn a_short_last_slice_is_left_aligned_with_zero_spare_bits() {
        let data = encode(&tone(23, 1, 9), 1);
        assert_eq!(data.len(), 16);
        let last = u64::from_le_bytes(data[8..16].try_into().unwrap());
        assert_eq!(last & ((1 << 51) - 1), 0, "17 spare residuals are zero");
    }

    #[test]
    fn the_encoder_writes_the_research_re_encoder_s_bytes() {
        // FNV-1a 64 of the bytes the reference encoder that re-encodes the
        // game's own sounds writes for these signals, without the weights
        // penalty.
        for (channels, frames, period, expected) in REFERENCE {
            let data = encode(&tone(frames, channels, period), channels);
            assert_eq!(
                fnv1a64(&data),
                expected,
                "{channels} channels, {frames} frames"
            );
        }
    }

    const REFERENCE: [(usize, usize, usize, u64); 4] = [
        (1, 48_000, 109, 0x4fc9_887c_bb3e_0c3c),
        (2, 30_011, 55, 0x1131_d746_cfd7_93c8),
        (6, 4_001, 240, 0xfa1d_478a_5fb3_71d5),
        (1, 23, 9, 0x8be3_bbf5_7c1a_8156),
    ];

    fn fnv1a64(bytes: &[u8]) -> u64 {
        enshwick_values::fnv::fnv1a64(enshwick_values::fnv::BASIS64, bytes)
    }

    proptest! {
        #[test]
        fn any_samples_encode_to_the_length_the_game_reads_and_decode_back(
            channels in 1usize..=8,
            samples in prop::collection::vec(any::<i16>(), 0..800),
        ) {
            let frames = samples.len() / channels;
            let data = encode(&samples, channels);
            prop_assert_eq!(data.len(), encoded_len(frames, channels));
            prop_assert_eq!(decode(&data, frames, channels).len(), frames * channels);
        }
    }
}
