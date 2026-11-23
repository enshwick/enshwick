//! A mod's audio file read into the game's samples: WAV, FLAC, Ogg Vorbis or
//! MP3 at any rate, trimmed, mapped to the channels the game plays,
//! resampled to [`SAMPLE_RATE`] and scaled, as 16-bit samples.

use core::time::Duration;
use std::io::Cursor;

use super::recording::SAMPLE_RATE;
use super::{GAIN_DB, MAX_TAKE};

/// The channel counts the game plays: mono, stereo, quadrophonic, 5.1 and
/// 7.1, its `SoundChannelConfiguration`.
const GAME_CHANNELS: [usize; 5] = [1, 2, 4, 6, 8];
/// The highest rate a file is read at.
const MAX_RATE: u32 = 384_000;

/// Which channels a sound keeps.
///
/// # Examples
///
/// ```
/// use enshwick::content::id;
/// use enshwick::sound::{Channels, NewSound};
///
/// let steps = NewSound { channels: Channels::Mono, ..NewSound::new(id!("steps"), "steps.ogg") };
/// assert_eq!(steps.channels, Channels::Mono);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum Channels {
    /// The file's: 1, 2, 4, 6 or 8 channels, as the game plays them.
    #[default]
    Keep,
    /// One, all of the file's mixed evenly: what a sound placed in the
    /// world wants, since the game places each channel at the same point.
    Mono,
    /// Two: a mono file's channel twice, else the file's first two.
    Stereo,
}

/// The part of each file a sound plays.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use enshwick::sound::Trim;
///
/// let middle = Trim { start: Duration::from_millis(250), end: Some(Duration::from_secs(2)) };
/// assert!(middle.end > Some(middle.start));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, crate::Script)]
pub struct Trim {
    /// Where it starts in the file.
    pub start: Duration,
    /// Where it ends in the file; `None` for the file's end.
    pub end: Option<Duration>,
}

/// How a file becomes a recording: its channels, gain and trim.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Shape {
    pub(crate) channels: Channels,
    pub(crate) gain_db: f32,
    pub(crate) trim: Trim,
}

/// A file's samples as read: interleaved, from -1 to 1.
#[derive(Debug, PartialEq)]
pub(crate) struct Read {
    pub(crate) samples: Vec<f32>,
    pub(crate) channels: usize,
    pub(crate) rate: u32,
}

/// A recording's samples as the game holds them: 16-bit, interleaved, at
/// [`SAMPLE_RATE`].
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Samples {
    pub(crate) samples: Vec<i16>,
    pub(crate) channels: usize,
}

impl Samples {
    pub(crate) fn frames(&self) -> usize {
        self.samples.len() / self.channels
    }
}

/// Why a file is no recording a mod adds.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub(crate) enum DecodeError {
    /// The bytes are none of the formats taken.
    #[error("not a WAV, FLAC, Ogg Vorbis or MP3 file")]
    Unknown,
    /// The file does not read as its format.
    #[error("not a {format} file this reads: {why}")]
    Malformed {
        /// The format its first bytes name.
        format: &'static str,
        /// What the reader said.
        why: String,
    },
    /// A channel count or rate out of reach.
    #[error("{channels} channels at {rate} Hz: a file has 1 to 8 channels at 1 to {MAX_RATE} Hz")]
    Format {
        /// Its channels.
        channels: usize,
        /// Its frames per second.
        rate: u32,
    },
    /// A channel count the game does not play, kept.
    #[error("{0} channels: the game plays 1, 2, 4, 6 or 8; mix it to mono or stereo")]
    Channels(usize),
    /// Longer than a take may be.
    #[error("longer than {} seconds", MAX_TAKE.as_secs())]
    TooLong,
    /// Nothing left to play.
    #[error("no samples between the trim's start and end")]
    Empty,
    /// A gain out of its bounds.
    #[error("gain {0} dB is outside {GAIN_DB:?}")]
    Gain(f32),
    /// The resampler refused the file.
    #[error("resampling to {SAMPLE_RATE} Hz: {0}")]
    Resample(String),
}

/// The most samples a file of `channels` channels at `rate` gives before
/// its trim: [`MAX_TAKE`] past the trim's start.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a positive count of frames, at most ten minutes at 384 kHz"
)]
fn most_samples(channels: usize, rate: u32, trim: Trim) -> usize {
    let seconds = (trim.start + MAX_TAKE).as_secs_f64();
    // At most 10 minutes past a start inside a file at 384 kHz, so it fits.
    let frames = (seconds * f64::from(rate)).ceil() as usize;
    frames.saturating_mul(channels)
}

/// Checks a file's channels and rate before its samples are read.
fn check_format(channels: usize, rate: u32) -> Result<(), DecodeError> {
    if (1..=8).contains(&channels) && (1..=MAX_RATE).contains(&rate) {
        Ok(())
    } else {
        Err(DecodeError::Format { channels, rate })
    }
}

/// Reads `bytes` by the format its first bytes name, stopping past what
/// `trim` keeps of [`MAX_TAKE`].
pub(crate) fn read(bytes: &[u8], trim: Trim) -> Result<Read, DecodeError> {
    match bytes {
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'A',
            b'V',
            b'E',
            ..,
        ] => wav(bytes, trim),
        [b'f', b'L', b'a', b'C', ..] => flac(bytes, trim),
        [b'O', b'g', b'g', b'S', ..] => ogg(bytes, trim),
        [b'I', b'D', b'3', ..] | [0xff, 0xe0..=0xff, ..] => mp3(bytes, trim),
        _ => Err(DecodeError::Unknown),
    }
}

fn malformed(format: &'static str) -> impl Fn(&dyn core::fmt::Display) -> DecodeError {
    move |why| DecodeError::Malformed {
        format,
        why: why.to_string(),
    }
}

/// Collects `samples` up to `most`, [`DecodeError::TooLong`] past it.
fn bounded<E>(
    samples: impl Iterator<Item = Result<f32, E>>,
    most: usize,
    fail: impl Fn(E) -> DecodeError,
) -> Result<Vec<f32>, DecodeError> {
    let mut out = Vec::new();
    for sample in samples {
        if out.len() == most {
            return Err(DecodeError::TooLong);
        }
        out.push(sample.map_err(&fail)?);
    }
    Ok(out)
}

/// The scale of an integer sample of `bits` bits.
#[expect(clippy::cast_precision_loss, reason = "a power of two, exact in f32")]
fn scale(bits: u32) -> f32 {
    1.0 / (1u64 << bits.clamp(1, 32).saturating_sub(1)) as f32
}

#[expect(
    clippy::cast_precision_loss,
    reason = "a sample to float, as audio is mixed"
)]
fn wav(bytes: &[u8], trim: Trim) -> Result<Read, DecodeError> {
    let fail = malformed("WAV");
    let reader = hound::WavReader::new(Cursor::new(bytes)).map_err(|e| fail(&e))?;
    let spec = reader.spec();
    let channels = usize::from(spec.channels);
    check_format(channels, spec.sample_rate)?;
    // The header's length is a claim: past the file's bytes it would size
    // nothing yet, but say so before reading.
    let claimed = u64::from(reader.len()) * u64::from(spec.bits_per_sample.div_ceil(8));
    if claimed > u64::try_from(bytes.len()).unwrap_or(u64::MAX) {
        return Err(fail(&"its header claims more samples than the file holds"));
    }
    let most = most_samples(channels, spec.sample_rate, trim);
    let samples = match spec.sample_format {
        hound::SampleFormat::Float => bounded(reader.into_samples::<f32>(), most, |e| fail(&e))?,
        hound::SampleFormat::Int => {
            let scale = scale(u32::from(spec.bits_per_sample));
            let ints = reader
                .into_samples::<i32>()
                .map(|s| s.map(|s| s as f32 * scale));
            bounded(ints, most, |e| fail(&e))?
        }
    };
    Ok(Read {
        samples,
        channels,
        rate: spec.sample_rate,
    })
}

#[expect(
    clippy::cast_precision_loss,
    reason = "a sample to float, as audio is mixed"
)]
fn flac(bytes: &[u8], trim: Trim) -> Result<Read, DecodeError> {
    let fail = malformed("FLAC");
    let mut reader = claxon::FlacReader::new(Cursor::new(bytes)).map_err(|e| fail(&e))?;
    let info = reader.streaminfo();
    let channels = usize::try_from(info.channels).unwrap_or(0);
    check_format(channels, info.sample_rate)?;
    let most = most_samples(channels, info.sample_rate, trim);
    let scale = scale(info.bits_per_sample);
    let samples = reader.samples().map(|s| s.map(|s| s as f32 * scale));
    let samples = bounded(samples, most, |e| fail(&e))?;
    Ok(Read {
        samples,
        channels,
        rate: info.sample_rate,
    })
}

fn ogg(bytes: &[u8], trim: Trim) -> Result<Read, DecodeError> {
    let fail = malformed("Ogg Vorbis");
    let mut reader =
        lewton::inside_ogg::OggStreamReader::new(Cursor::new(bytes)).map_err(|e| fail(&e))?;
    let channels = usize::from(reader.ident_hdr.audio_channels);
    let rate = reader.ident_hdr.audio_sample_rate;
    check_format(channels, rate)?;
    let most = most_samples(channels, rate, trim);
    let scale = scale(16);
    let mut samples = Vec::new();
    while let Some(packet) = reader.read_dec_packet_itl().map_err(|e| fail(&e))? {
        if samples.len() + packet.len() > most {
            return Err(DecodeError::TooLong);
        }
        samples.extend(packet.into_iter().map(|s| f32::from(s) * scale));
    }
    Ok(Read {
        samples,
        channels,
        rate,
    })
}

fn mp3(bytes: &[u8], trim: Trim) -> Result<Read, DecodeError> {
    let fail = malformed("MP3");
    let mut reader = nanomp3::Reader::<_, f32>::new(Cursor::new(bytes)).map_err(|e| fail(&e))?;
    let channels = reader.channels().map_or(0, |c| c.num() as usize);
    let rate = reader.sample_rate();
    check_format(channels, rate)?;
    let most = most_samples(channels, rate, trim);
    let mut samples = Vec::new();
    let mut chunk = vec![0.0f32; 4096 * channels];
    loop {
        let read = reader.read(&mut chunk).map_err(|e| fail(&e))?;
        if read == 0 {
            break;
        }
        if samples.len() + read > most {
            return Err(DecodeError::TooLong);
        }
        samples.extend_from_slice(&chunk[..read]);
    }
    Ok(Read {
        samples,
        channels,
        rate,
    })
}

/// The frame of `rate` at `time`, at most `frames`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a rounded, positive frame number, clamped to the file"
)]
fn frame_at(time: Duration, rate: u32, frames: usize) -> usize {
    let at = (time.as_secs_f64() * f64::from(rate)).round();
    // Not negative, and anything past `frames` clamps to it.
    (at as usize).min(frames)
}

/// `read` trimmed, its channels mapped, resampled to [`SAMPLE_RATE`] and
/// scaled by the gain, as the game's samples.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "frame counts within ten minutes, and samples clamped into the i16 range"
)]
pub(crate) fn shape(read: Read, shape: Shape) -> Result<Samples, DecodeError> {
    if !GAIN_DB.contains(&shape.gain_db) {
        return Err(DecodeError::Gain(shape.gain_db));
    }
    let Read {
        samples,
        channels,
        rate,
    } = read;
    let frames = samples.len() / channels;
    let start = frame_at(shape.trim.start, rate, frames);
    let end = shape
        .trim
        .end
        .map_or(frames, |end| frame_at(end, rate, frames));
    if end <= start {
        return Err(DecodeError::Empty);
    }
    if Duration::from_secs_f64((end - start) as f64 / f64::from(rate)) > MAX_TAKE {
        return Err(DecodeError::TooLong);
    }
    let kept = &samples[start * channels..end * channels];
    let (mapped, out_channels) = map_channels(kept, channels, shape.channels)?;
    let resampled = resample(mapped, out_channels, rate)?;
    let gain = 10f32.powf(shape.gain_db / 20.0) * f32::from(i16::MAX);
    let samples = resampled
        .into_iter()
        // Clamped into the i16 range first.
        .map(|s| (s * gain).round().clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16)
        .collect();
    Ok(Samples {
        samples,
        channels: out_channels,
    })
}

/// `samples` of `channels` channels as `to` maps them, and their count.
#[expect(clippy::cast_precision_loss, reason = "1 to 8 channels")]
fn map_channels(
    samples: &[f32],
    channels: usize,
    to: Channels,
) -> Result<(Vec<f32>, usize), DecodeError> {
    let frames = samples.chunks_exact(channels);
    match to {
        Channels::Keep if GAME_CHANNELS.contains(&channels) => Ok((samples.to_vec(), channels)),
        Channels::Keep => Err(DecodeError::Channels(channels)),
        Channels::Mono => {
            let share = 1.0 / channels as f32;
            Ok((frames.map(|f| f.iter().sum::<f32>() * share).collect(), 1))
        }
        Channels::Stereo => Ok((
            frames
                .flat_map(|f| [f[0], f[channels.min(2) - 1]])
                .collect(),
            2,
        )),
    }
}

/// `samples` of `channels` channels at `rate`, at [`SAMPLE_RATE`].
fn resample(samples: Vec<f32>, channels: usize, rate: u32) -> Result<Vec<f32>, DecodeError> {
    use rubato::audioadapter_buffers::direct::InterleavedSlice;
    use rubato::{Fft, FixedSync, Resampler};

    if rate == SAMPLE_RATE {
        return Ok(samples);
    }
    let frames = samples.len() / channels;
    let fail = |e: &dyn core::fmt::Display| DecodeError::Resample(e.to_string());
    let mut resampler = Fft::<f32>::new(
        rate as usize,
        SAMPLE_RATE as usize,
        1024,
        channels,
        FixedSync::Both,
    )
    .map_err(|e| fail(&e))?;
    let input = InterleavedSlice::new(&samples, channels, frames).map_err(|e| fail(&e))?;
    let output = resampler
        .process_all(&input, frames, None)
        .map_err(|e| fail(&e))?;
    Ok(output.take_data())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A WAV file of `samples`, written by hound.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "test samples within half of full scale"
    )]
    fn wav_file(
        channels: u16,
        rate: u32,
        format: hound::SampleFormat,
        bits: u16,
        samples: &[f32],
    ) -> Vec<u8> {
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: bits,
            sample_format: format,
        };
        let mut out = Cursor::new(Vec::new());
        let mut writer = hound::WavWriter::new(&mut out, spec).unwrap();
        for &s in samples {
            match format {
                hound::SampleFormat::Float => writer.write_sample(s).unwrap(),
                hound::SampleFormat::Int => {
                    let full = ((1i64 << (bits - 1)) - 1) as f32;
                    writer.write_sample((s * full).round() as i32).unwrap();
                }
            }
        }
        writer.finalize().unwrap();
        out.into_inner()
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "test frame counts well within f32"
    )]
    fn sine(frames: usize, rate: u32, hz: f32) -> Vec<f32> {
        (0..frames)
            .map(|i| (i as f32 * core::f32::consts::TAU * hz / rate as f32).sin() * 0.5)
            .collect()
    }

    #[test]
    fn every_wav_sample_format_reads_to_the_same_wave() {
        let wave = sine(480, SAMPLE_RATE, 440.0);
        for (format, bits) in [
            (hound::SampleFormat::Int, 8),
            (hound::SampleFormat::Int, 16),
            (hound::SampleFormat::Int, 24),
            (hound::SampleFormat::Int, 32),
            (hound::SampleFormat::Float, 32),
        ] {
            let read = read(
                &wav_file(1, SAMPLE_RATE, format, bits, &wave),
                Trim::default(),
            )
            .unwrap();
            assert_eq!((read.channels, read.rate), (1, SAMPLE_RATE));
            let off = read
                .samples
                .iter()
                .zip(&wave)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0, f32::max);
            assert!(off < 0.01, "{format:?} {bits}: {off}");
        }
    }

    #[test]
    fn a_file_of_no_format_taken_or_out_of_reach_is_refused_naming_why() {
        assert_eq!(
            read(b"MThd....", Trim::default()),
            Err(DecodeError::Unknown)
        );
        assert!(matches!(
            read(b"fLaC", Trim::default()),
            Err(DecodeError::Malformed { format: "FLAC", .. })
        ));
        assert!(matches!(
            read(b"OggS", Trim::default()),
            Err(DecodeError::Malformed {
                format: "Ogg Vorbis",
                ..
            })
        ));
        let nine = wav_file(9, SAMPLE_RATE, hound::SampleFormat::Int, 16, &[0.0; 9]);
        assert_eq!(
            read(&nine, Trim::default()),
            Err(DecodeError::Format {
                channels: 9,
                rate: SAMPLE_RATE
            })
        );
        let three = wav_file(3, SAMPLE_RATE, hound::SampleFormat::Int, 16, &[0.0; 3]);
        let read3 = read(&three, Trim::default()).unwrap();
        assert_eq!(
            shape(read3, Shape::default()),
            Err(DecodeError::Channels(3))
        );
    }

    #[test]
    fn a_file_past_ten_minutes_stops_reading_there() {
        // 1 Hz files keep the test small: 601 frames is past 600 seconds.
        let long = wav_file(1, 1, hound::SampleFormat::Int, 16, &[0.0; 602]);
        assert_eq!(read(&long, Trim::default()), Err(DecodeError::TooLong));
        let trimmed = Trim {
            start: Duration::from_secs(2),
            end: None,
        };
        let read = read(&long, trimmed).unwrap();
        assert_eq!(
            read.samples.len(),
            602,
            "a trimmed start reads that much further"
        );
    }

    #[test]
    fn a_sound_is_trimmed_mapped_resampled_and_scaled() {
        let stereo: Vec<f32> = sine(44_100, 44_100, 441.0)
            .into_iter()
            .flat_map(|s| [s, -s])
            .collect();
        let read = Read {
            samples: stereo,
            channels: 2,
            rate: 44_100,
        };
        let shaped = shape(
            read,
            Shape {
                channels: Channels::Stereo,
                gain_db: -6.0,
                trim: Trim {
                    start: Duration::from_millis(500),
                    end: None,
                },
            },
        )
        .unwrap();
        assert_eq!(shaped.channels, 2);
        let frames = shaped.frames();
        assert!(
            (23_990..=24_010).contains(&frames),
            "{frames} frames for half a second"
        );
        let peak = shaped
            .samples
            .iter()
            .map(|s| s.unsigned_abs())
            .max()
            .unwrap();
        assert!(
            (7_900..=8_500).contains(&peak),
            "half scale at -6 dB: {peak}"
        );
        let mono = Read {
            samples: vec![0.5, -0.5, 0.25, 0.25],
            channels: 2,
            rate: SAMPLE_RATE,
        };
        let shaped = shape(
            mono,
            Shape {
                channels: Channels::Mono,
                ..Shape::default()
            },
        )
        .unwrap();
        assert_eq!(shaped.samples, vec![0, 8192]);
    }

    #[test]
    fn a_trim_that_leaves_nothing_or_a_gain_past_its_bound_is_refused() {
        let read = || Read {
            samples: vec![0.0; 48],
            channels: 1,
            rate: SAMPLE_RATE,
        };
        let late = Trim {
            start: Duration::from_secs(1),
            end: None,
        };
        assert_eq!(
            shape(
                read(),
                Shape {
                    trim: late,
                    ..Shape::default()
                }
            ),
            Err(DecodeError::Empty)
        );
        assert_eq!(
            shape(
                read(),
                Shape {
                    gain_db: 30.0,
                    ..Shape::default()
                }
            ),
            Err(DecodeError::Gain(30.0))
        );
    }

    proptest::proptest! {
        #[test]
        fn any_bytes_after_a_format_s_magic_read_or_fail_without_panicking(
            magic in proptest::sample::select(vec![&b"RIFF\0\0\0\0WAVE"[..], b"fLaC", b"OggS", b"ID3", &[0xff, 0xfb][..]]),
            rest in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..512),
        ) {
            let mut bytes = magic.to_vec();
            bytes.extend(rest);
            if let Ok(read) = read(&bytes, Trim::default()) {
                let _shaped = shape(read, Shape::default());
            }
        }
    }
}
