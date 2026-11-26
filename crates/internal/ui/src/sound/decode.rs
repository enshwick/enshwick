//! A mod's audio file made into a recording as it decodes: WAV, FLAC, Ogg
//! Vorbis or MP3 at 8 to 384 kHz, trimmed, mapped to the channels the game
//! plays, resampled to [`SAMPLE_RATE`] one chunk at a time and scaled, as
//! 16-bit samples, so it holds the recording, never the whole file.

use core::ops::RangeInclusive;
use core::time::Duration;
use std::io::Cursor;

use super::qoa;
use super::recording::SAMPLE_RATE;
use super::{GAIN_DB, MAX_MOD_BYTES, MAX_TAKE};

/// The channel counts the game plays: mono, stereo, quadrophonic, 5.1 and
/// 7.1, its `SoundChannelConfiguration`.
const GAME_CHANNELS: [usize; 5] = [1, 2, 4, 6, 8];
/// The rates a file is read at; below 8 kHz the resampler's buffers grow
/// with the ratio, to gigabytes at 1 Hz.
const RATES: RangeInclusive<u32> = 8_000..=384_000;
/// The input frames of each chunk the resampler takes.
const CHUNK: usize = 1024;
/// For each channel of a Vorbis file of 1 to 8 channels, in the order WAV
/// and FLAC files keep them, the Vorbis channel it is (Vorbis I
/// specification, 4.3.9).
const VORBIS_ORDER: [&[usize]; 8] = [
    &[0],
    &[0, 1],
    &[0, 2, 1],
    &[0, 1, 2, 3],
    &[0, 2, 1, 3, 4],
    &[0, 2, 1, 5, 3, 4],
    &[0, 2, 1, 6, 5, 3, 4],
    &[0, 2, 1, 7, 5, 6, 3, 4],
];

/// Which channels a sound keeps.
///
/// # Examples
///
/// ```
/// use enshwick::content::id;
/// use enshwick::sound::{Channels, NewSound};
///
/// let mut steps = NewSound::new(id!("steps"), "steps.ogg");
/// steps.channels = Channels::Mono;
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
/// let middle = Trim { from: Duration::from_millis(250), to: Some(Duration::from_secs(2)) };
/// assert!(middle.to > Some(middle.from));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, crate::Script)]
#[expect(
    clippy::exhaustive_structs,
    reason = "the part of a file a sound plays: where it starts and ends"
)]
pub struct Trim {
    /// Where it starts in the file.
    pub from: Duration,
    /// Where it ends in the file; `None` for the file's end.
    pub to: Option<Duration>,
}

/// How a file becomes a recording: its channels, gain and trim.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Shape {
    pub(crate) channels: Channels,
    pub(crate) gain_db: f32,
    pub(crate) trim: Trim,
}

/// How long a recording may play and how many bytes it may take encoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Bounds {
    pub(crate) take: Duration,
    pub(crate) bytes: u64,
}

impl Bounds {
    /// [`MAX_TAKE`] and what is left of the mod's [`MAX_MOD_BYTES`] after
    /// `held` bytes.
    pub(crate) const fn left(held: u64) -> Self {
        Self {
            take: MAX_TAKE,
            bytes: MAX_MOD_BYTES.saturating_sub(held),
        }
    }
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
    #[error(
        "{channels} channels at {rate} Hz: a file has 1 to 8 channels at {} to {} Hz",
        RATES.start(),
        RATES.end()
    )]
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
    /// More than the mod's recordings may hold.
    #[error("the mod's recordings would pass {MAX_MOD_BYTES} bytes")]
    Budget,
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

/// Makes `bytes`, in the format their first bytes name, into a recording
/// as `shape` says, within `bounds`.
pub(crate) fn recording(
    bytes: &[u8],
    shape: Shape,
    bounds: Bounds,
) -> Result<Samples, DecodeError> {
    if !GAIN_DB.contains(&shape.gain_db) {
        return Err(DecodeError::Gain(shape.gain_db));
    }
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
        ] => wav(bytes, shape, bounds),
        [b'f', b'L', b'a', b'C', ..] => flac(bytes, shape, bounds),
        [b'O', b'g', b'g', b'S', ..] => ogg(bytes, shape, bounds),
        [b'I', b'D', b'3', ..] | [0xff, 0xe0..=0xff, ..] => mp3(bytes, shape, bounds),
        _ => Err(DecodeError::Unknown),
    }
}

fn malformed(format: &'static str) -> impl Fn(&dyn core::fmt::Display) -> DecodeError {
    move |why| DecodeError::Malformed {
        format,
        why: why.to_string(),
    }
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
fn wav(bytes: &[u8], shape: Shape, bounds: Bounds) -> Result<Samples, DecodeError> {
    let fail = malformed("WAV");
    let reader = hound::WavReader::new(Cursor::new(bytes)).map_err(|e| fail(&e))?;
    let spec = reader.spec();
    let mut pipeline = Pipeline::new(usize::from(spec.channels), spec.sample_rate, shape, bounds)?;
    // The header's length is a claim: past the file's bytes it would size
    // nothing yet, but say so before reading.
    let claimed = u64::from(reader.len()) * u64::from(spec.bits_per_sample.div_ceil(8));
    if claimed > u64::try_from(bytes.len()).unwrap_or(u64::MAX) {
        return Err(fail(&"its header claims more samples than the file holds"));
    }
    match spec.sample_format {
        hound::SampleFormat::Float => pipeline.feed(reader.into_samples::<f32>(), |e| fail(&e))?,
        hound::SampleFormat::Int => {
            let scale = scale(u32::from(spec.bits_per_sample));
            let ints = reader
                .into_samples::<i32>()
                .map(|s| s.map(|s| s as f32 * scale));
            pipeline.feed(ints, |e| fail(&e))?;
        }
    }
    pipeline.finish()
}

#[expect(
    clippy::cast_precision_loss,
    reason = "a sample to float, as audio is mixed"
)]
fn flac(bytes: &[u8], shape: Shape, bounds: Bounds) -> Result<Samples, DecodeError> {
    let fail = malformed("FLAC");
    let mut reader = claxon::FlacReader::new(Cursor::new(bytes)).map_err(|e| fail(&e))?;
    let info = reader.streaminfo();
    let channels = usize::try_from(info.channels).unwrap_or(0);
    let mut pipeline = Pipeline::new(channels, info.sample_rate, shape, bounds)?;
    let scale = scale(info.bits_per_sample);
    let samples = reader.samples().map(|s| s.map(|s| s as f32 * scale));
    pipeline.feed(samples, |e| fail(&e))?;
    pipeline.finish()
}

fn ogg(bytes: &[u8], shape: Shape, bounds: Bounds) -> Result<Samples, DecodeError> {
    let fail = malformed("Ogg Vorbis");
    let mut reader =
        lewton::inside_ogg::OggStreamReader::new(Cursor::new(bytes)).map_err(|e| fail(&e))?;
    let channels = usize::from(reader.ident_hdr.audio_channels);
    let mut pipeline = Pipeline::new(channels, reader.ident_hdr.audio_sample_rate, shape, bounds)?;
    // Checked to be 1 to 8 by the pipeline.
    let order = VORBIS_ORDER[channels - 1];
    let scale = scale(16);
    'read: while let Some(packet) = reader.read_dec_packet_itl().map_err(|e| fail(&e))? {
        for frame in packet.chunks_exact(channels) {
            for &at in order {
                if !pipeline.push(f32::from(frame[at]) * scale)? {
                    break 'read;
                }
            }
        }
    }
    pipeline.finish()
}

fn mp3(bytes: &[u8], shape: Shape, bounds: Bounds) -> Result<Samples, DecodeError> {
    let fail = malformed("MP3");
    let mut reader = nanomp3::Reader::<_, f32>::new(Cursor::new(bytes)).map_err(|e| fail(&e))?;
    let channels = reader.channels().map_or(0, |c| c.num() as usize);
    let mut pipeline = Pipeline::new(channels, reader.sample_rate(), shape, bounds)?;
    let mut chunk = vec![0.0f32; 4096 * channels];
    'read: loop {
        let read = reader.read(&mut chunk).map_err(|e| fail(&e))?;
        if read == 0 {
            break;
        }
        for &sample in &chunk[..read] {
            if !pipeline.push(sample)? {
                break 'read;
            }
        }
    }
    pipeline.finish()
}

/// The frame of `rate` at `time`; one past the counter's range saturates.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a rounded, positive frame number, saturating"
)]
fn frame_at(time: Duration, rate: u32) -> u64 {
    (time.as_secs_f64() * f64::from(rate)).round() as u64
}

/// One frame of `frame.len()` channels as `to` maps it, and its channels.
#[expect(clippy::cast_precision_loss, reason = "1 to 8 channels")]
fn map_frame(frame: &[f32], to: Channels) -> ([f32; 8], usize) {
    let mut out = [0.0; 8];
    match to {
        Channels::Keep => {
            out[..frame.len()].copy_from_slice(frame);
            (out, frame.len())
        }
        Channels::Mono => {
            out[0] = frame.iter().sum::<f32>() / frame.len() as f32;
            (out, 1)
        }
        Channels::Stereo => {
            out[0] = frame[0];
            out[1] = frame[frame.len().min(2) - 1];
            (out, 2)
        }
    }
}

fn resample_failed(why: impl core::fmt::Display) -> DecodeError {
    DecodeError::Resample(why.to_string())
}

/// A recording made as its file decodes: the frames before the trim are
/// skipped, each kept frame's channels mapped, then resampled and scaled.
struct Pipeline {
    channels: usize,
    map: Channels,
    frame: [f32; 8],
    filled: usize,
    /// Frames still to skip before the trim's start.
    skip: u64,
    /// Frames still to keep before the trim's end or the take's bound.
    left: u64,
    /// Whether a frame past `left` makes the take too long, rather than
    /// lying past the trim's end.
    past_is_long: bool,
    taken: u64,
    resample: Option<Resample>,
    out: Out,
}

impl Pipeline {
    fn new(channels: usize, rate: u32, shape: Shape, bounds: Bounds) -> Result<Self, DecodeError> {
        if !(1..=8).contains(&channels) || !RATES.contains(&rate) {
            return Err(DecodeError::Format { channels, rate });
        }
        let out_channels = match shape.channels {
            Channels::Keep if GAME_CHANNELS.contains(&channels) => channels,
            Channels::Keep => return Err(DecodeError::Channels(channels)),
            Channels::Mono => 1,
            Channels::Stereo => 2,
        };
        let skip = frame_at(shape.trim.from, rate);
        let most = frame_at(bounds.take, rate);
        let span = shape
            .trim
            .to
            .map(|to| frame_at(to, rate).saturating_sub(skip));
        let (left, past_is_long) = match span {
            Some(span) if span <= most => (span, false),
            _ => (most, true),
        };
        let resample = (rate != SAMPLE_RATE)
            .then(|| Resample::new(rate, out_channels))
            .transpose()?;
        let out = Out {
            samples: Vec::new(),
            channels: out_channels,
            gain: 10f32.powf(shape.gain_db / 20.0) * f32::from(i16::MAX),
            bytes: bounds.bytes,
            slack: resample.as_ref().map_or(0, Resample::slack),
        };
        Ok(Self {
            channels,
            map: shape.channels,
            frame: [0.0; 8],
            filled: 0,
            skip,
            left,
            past_is_long,
            taken: 0,
            resample,
            out,
        })
    }

    /// Takes the file's next sample; `false` once the trim's end is reached.
    fn push(&mut self, sample: f32) -> Result<bool, DecodeError> {
        self.frame[self.filled] = sample;
        self.filled += 1;
        if self.filled < self.channels {
            return Ok(true);
        }
        self.filled = 0;
        if self.skip > 0 {
            self.skip -= 1;
            return Ok(true);
        }
        if self.left == 0 {
            return if self.past_is_long {
                Err(DecodeError::TooLong)
            } else {
                Ok(false)
            };
        }
        self.left -= 1;
        self.taken += 1;
        let (mapped, channels) = map_frame(&self.frame[..self.channels], self.map);
        match &mut self.resample {
            Some(resample) => resample.push(&mapped[..channels], &mut self.out)?,
            None => self.out.push(&mapped[..channels])?,
        }
        Ok(self.left > 0 || self.past_is_long)
    }

    /// Takes `samples` until the trim's end.
    fn feed<E>(
        &mut self,
        samples: impl Iterator<Item = Result<f32, E>>,
        fail: impl Fn(E) -> DecodeError,
    ) -> Result<(), DecodeError> {
        for sample in samples {
            if !self.push(sample.map_err(&fail)?)? {
                break;
            }
        }
        Ok(())
    }

    fn finish(mut self) -> Result<Samples, DecodeError> {
        if self.taken == 0 {
            return Err(DecodeError::Empty);
        }
        if let Some(resample) = self.resample.take() {
            resample.finish(&mut self.out)?;
        }
        let Out {
            samples,
            channels,
            bytes,
            ..
        } = self.out;
        let encoded = qoa::encoded_len(samples.len() / channels, channels);
        if u64::try_from(encoded).unwrap_or(u64::MAX) > bytes {
            return Err(DecodeError::Budget);
        }
        Ok(Samples { samples, channels })
    }
}

/// The recording's samples as they are made, refused once past its bytes.
struct Out {
    samples: Vec<i16>,
    channels: usize,
    gain: f32,
    bytes: u64,
    /// Frames the resampler may make past the end, which it cuts at the
    /// end; they count against nothing until then.
    slack: usize,
}

impl Out {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "clamped into the i16 range first"
    )]
    fn push(&mut self, samples: &[f32]) -> Result<(), DecodeError> {
        let (low, high) = (f32::from(i16::MIN), f32::from(i16::MAX));
        self.samples.extend(
            samples
                .iter()
                .map(|s| (s * self.gain).round().clamp(low, high) as i16),
        );
        let frames = (self.samples.len() / self.channels).saturating_sub(self.slack);
        let encoded = qoa::encoded_len(frames, self.channels);
        if u64::try_from(encoded).unwrap_or(u64::MAX) > self.bytes {
            return Err(DecodeError::Budget);
        }
        Ok(())
    }
}

/// Resamples to [`SAMPLE_RATE`] one chunk at a time, as rubato's
/// `process_all_into_buffer` does a whole clip: the resampler's delay is
/// cut from the start and its padding from the end.
struct Resample {
    fft: rubato::Fft<f32>,
    channels: usize,
    input: Vec<f32>,
    output: Vec<f32>,
    /// Output frames still to cut for the resampler's delay.
    delay: usize,
    taken: usize,
    made: usize,
}

impl Resample {
    fn new(rate: u32, channels: usize) -> Result<Self, DecodeError> {
        use rubato::Resampler as _;

        let fft = rubato::Fft::<f32>::new(
            rate as usize,
            SAMPLE_RATE as usize,
            CHUNK,
            channels,
            rubato::FixedSync::Both,
        )
        .map_err(resample_failed)?;
        Ok(Self {
            input: Vec::with_capacity(fft.input_frames_max() * channels),
            output: vec![0.0; fft.output_frames_max() * channels],
            delay: fft.output_delay(),
            channels,
            taken: 0,
            made: 0,
            fft,
        })
    }

    /// Frames it may make past a clip's end before [`Resample::finish`].
    fn slack(&self) -> usize {
        self.output.len() / self.channels
    }

    fn push(&mut self, frame: &[f32], out: &mut Out) -> Result<(), DecodeError> {
        use rubato::Resampler as _;

        self.input.extend_from_slice(frame);
        self.taken += 1;
        if self.input.len() == self.fft.input_frames_next() * self.channels {
            self.run(None, out)?;
        }
        Ok(())
    }

    /// Resamples the waiting frames, `partial` of a chunk or silence with
    /// `Some(0)`, and passes the output past the delay to `out`.
    fn run(&mut self, partial: Option<usize>, out: &mut Out) -> Result<(), DecodeError> {
        use rubato::Resampler as _;
        use rubato::audioadapter_buffers::direct::InterleavedSlice;

        let frames = self.input.len() / self.channels;
        let room = self.output.len() / self.channels;
        let input = InterleavedSlice::new(&self.input[..], self.channels, frames)
            .map_err(resample_failed)?;
        let mut output = InterleavedSlice::new_mut(&mut self.output[..], self.channels, room)
            .map_err(resample_failed)?;
        let indexing = rubato::Indexing {
            input_offset: 0,
            output_offset: 0,
            partial_len: partial,
            active_channels_mask: None,
        };
        let (_, made) = self
            .fft
            .process_into_buffer(&input, &mut output, Some(&indexing))
            .map_err(resample_failed)?;
        self.input.clear();
        let cut = made.min(self.delay);
        self.delay -= cut;
        self.made += made - cut;
        out.push(&self.output[cut * self.channels..made * self.channels])
    }

    /// Resamples the last frames and cuts the output to the clip's length.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        reason = "a positive frame count within ten minutes at 384 kHz"
    )]
    fn finish(mut self, out: &mut Out) -> Result<(), DecodeError> {
        use rubato::Resampler as _;

        let rest = self.input.len() / self.channels;
        if rest > 0 {
            self.run(Some(rest), out)?;
        }
        let want = (self.fft.resample_ratio() * self.taken as f64).ceil() as usize;
        while self.made < want {
            self.run(Some(0), out)?;
        }
        out.samples.truncate(want * self.channels);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bounds that keep the tests small: 10 ms a take.
    const SMALL: Bounds = Bounds {
        take: Duration::from_millis(10),
        bytes: MAX_MOD_BYTES,
    };

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

    fn float_wav(channels: u16, rate: u32, samples: &[f32]) -> Vec<u8> {
        wav_file(channels, rate, hound::SampleFormat::Float, 32, samples)
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

    fn trimmed(from: Duration, to: Option<Duration>) -> Shape {
        Shape {
            trim: Trim { from, to },
            ..Shape::default()
        }
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
            let file = wav_file(1, SAMPLE_RATE, format, bits, &wave);
            let made = recording(&file, Shape::default(), SMALL).unwrap();
            assert_eq!((made.channels, made.frames()), (1, 480));
            let off = made
                .samples
                .iter()
                .zip(&wave)
                .map(|(&a, b)| (f32::from(a) / f32::from(i16::MAX) - b).abs())
                .fold(0.0, f32::max);
            assert!(off < 0.01, "{format:?} {bits}: {off}");
        }
    }

    #[test]
    fn a_file_of_no_format_taken_or_out_of_reach_is_refused_naming_why() {
        let made = |bytes: &[u8]| recording(bytes, Shape::default(), SMALL);
        assert_eq!(made(b"MThd...."), Err(DecodeError::Unknown));
        assert!(matches!(
            made(b"fLaC"),
            Err(DecodeError::Malformed { format: "FLAC", .. })
        ));
        assert!(matches!(
            made(b"OggS"),
            Err(DecodeError::Malformed {
                format: "Ogg Vorbis",
                ..
            })
        ));
        assert_eq!(
            made(&float_wav(9, SAMPLE_RATE, &[0.0; 9])),
            Err(DecodeError::Format {
                channels: 9,
                rate: SAMPLE_RATE
            })
        );
        assert_eq!(
            made(&float_wav(1, 7_999, &[0.0; 9])),
            Err(DecodeError::Format {
                channels: 1,
                rate: 7_999
            })
        );
        assert_eq!(
            made(&float_wav(3, SAMPLE_RATE, &[0.0; 3])),
            Err(DecodeError::Channels(3))
        );
    }

    #[test]
    fn a_header_that_claims_more_samples_than_the_file_holds_is_refused() {
        let mut file = float_wav(1, SAMPLE_RATE, &[0.0; 4]);
        let data = file.len() - 16;
        file[data - 4..data].copy_from_slice(&(1u32 << 30).to_le_bytes());
        assert!(matches!(
            recording(&file, Shape::default(), SMALL),
            Err(DecodeError::Malformed { format: "WAV", why }) if why.contains("claims more samples")
        ));
    }

    #[test]
    fn a_take_past_its_bound_is_refused_unless_the_trim_ends_it_sooner() {
        // 482 frames is past 10 ms, 480 frames at 48 kHz.
        let long = float_wav(1, SAMPLE_RATE, &[0.25; 482]);
        let ms = Duration::from_millis;
        assert_eq!(
            recording(&long, Shape::default(), SMALL),
            Err(DecodeError::TooLong)
        );
        let late = recording(&long, trimmed(ms(1), None), SMALL).unwrap();
        assert_eq!(
            late.frames(),
            482 - 48,
            "a later start reads that much further"
        );
        let short = recording(&long, trimmed(ms(2), Some(ms(7))), SMALL).unwrap();
        assert_eq!(short.frames(), 240, "a file running past the take's bound");
        assert_eq!(
            recording(&long, trimmed(Duration::ZERO, Some(ms(20))), SMALL),
            Err(DecodeError::TooLong),
            "a trim whose span passes the bound"
        );
    }

    #[test]
    fn a_trim_past_any_file_leaves_nothing_without_overflowing() {
        let file = float_wav(1, SAMPLE_RATE, &[0.25; 48]);
        for from in [Duration::from_secs(1), Duration::MAX] {
            assert_eq!(
                recording(&file, trimmed(from, None), SMALL),
                Err(DecodeError::Empty),
                "{from:?}"
            );
        }
        let ms = Duration::from_millis;
        assert_eq!(
            recording(&file, trimmed(ms(1), Some(ms(1))), SMALL),
            Err(DecodeError::Empty)
        );
    }

    #[test]
    fn a_gain_past_its_bound_is_refused_before_the_file_is_read() {
        let shape = Shape {
            gain_db: 30.0,
            ..Shape::default()
        };
        assert_eq!(
            recording(b"not audio", shape, SMALL),
            Err(DecodeError::Gain(30.0))
        );
    }

    #[test]
    fn a_recording_past_the_bytes_left_is_refused_while_it_decodes() {
        let file = float_wav(1, 44_100, &sine(441, 44_100, 441.0));
        let room = |bytes| Bounds { bytes, ..SMALL };
        let full = recording(&file, Shape::default(), SMALL).unwrap();
        let needed = qoa::encoded_len(full.frames(), 1) as u64;
        assert!(recording(&file, Shape::default(), room(needed)).is_ok());
        assert_eq!(
            recording(&file, Shape::default(), room(needed - 8)),
            Err(DecodeError::Budget)
        );
        let at_rate = float_wav(1, SAMPLE_RATE, &[0.25; 480]);
        assert_eq!(
            recording(&at_rate, Shape::default(), room(8)),
            Err(DecodeError::Budget)
        );
    }

    #[test]
    fn a_sound_is_trimmed_mapped_resampled_and_scaled() {
        let stereo: Vec<f32> = sine(44_100, 44_100, 441.0)
            .into_iter()
            .flat_map(|s| [s, -s])
            .collect();
        let shape = Shape {
            channels: Channels::Stereo,
            gain_db: -6.0,
            trim: Trim {
                from: Duration::from_millis(500),
                to: None,
            },
        };
        let bounds = Bounds {
            take: MAX_TAKE,
            ..SMALL
        };
        let shaped = recording(&float_wav(2, 44_100, &stereo), shape, bounds).unwrap();
        assert_eq!(shaped.channels, 2);
        let frames = shaped.frames();
        assert!(
            (24_000..=24_001).contains(&frames),
            "{frames} for half a second"
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
        let two = float_wav(2, SAMPLE_RATE, &[0.5, -0.5, 0.25, 0.25]);
        let mono = Shape {
            channels: Channels::Mono,
            ..Shape::default()
        };
        assert_eq!(recording(&two, mono, SMALL).unwrap().samples, vec![0, 8192]);
    }

    #[test]
    fn a_resampled_wave_keeps_its_pitch_and_starts_without_the_resampler_s_delay() {
        let file = float_wav(1, 44_100, &sine(4_410, 44_100, 441.0));
        let bounds = Bounds {
            take: MAX_TAKE,
            ..SMALL
        };
        let made = recording(&file, Shape::default(), bounds).unwrap();
        assert!(
            (4_800..=4_801).contains(&made.frames()),
            "{}",
            made.frames()
        );
        let crossings = made.samples[100..4_700]
            .windows(2)
            .filter(|w| (w[0] < 0) != (w[1] < 0))
            .count();
        // 441 Hz crosses zero 882 times a second: about 84 in 4,600 frames.
        assert!((83..=86).contains(&crossings), "{crossings}");
        // Five frames in, the half-scale sine is at about 4,700.
        let start = (made.samples[0], made.samples[5]);
        assert!(
            start.0.unsigned_abs() < 1_500 && (3_000..6_500).contains(&start.1),
            "the wave rises at once, with no lead-in of silence: {start:?}"
        );
    }

    #[test]
    fn a_vorbis_file_s_channels_take_the_order_wav_and_flac_keep() {
        for (count, order) in VORBIS_ORDER.iter().enumerate() {
            let mut sorted = order.to_vec();
            sorted.sort_unstable();
            assert_eq!(
                sorted,
                (0..=count).collect::<Vec<_>>(),
                "{} channels",
                count + 1
            );
        }
        // Vorbis 5.1 is FL, C, FR, RL, RR, LFE; WAV is FL, FR, C, LFE, RL, RR.
        assert_eq!(VORBIS_ORDER[5], [0, 2, 1, 5, 3, 4]);
    }

    proptest::proptest! {
        #[test]
        fn any_bytes_after_a_format_s_magic_read_or_fail_without_panicking(
            magic in proptest::sample::select(vec![&b"RIFF\0\0\0\0WAVE"[..], b"fLaC", b"OggS", b"ID3", &[0xff, 0xfb][..]]),
            rest in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..512),
        ) {
            let mut bytes = magic.to_vec();
            bytes.extend(rest);
            let _made = recording(&bytes, Shape::default(), SMALL);
        }
    }
}
