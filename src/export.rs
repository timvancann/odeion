//! A loop or a section saved as an audio file (Tim, 2026-11-11, asked for on YouTube):
//! one pass of it, as recorded or at the speed and pitch it plays at, all stems at full
//! level or the mix as heard, as MP3 or Ogg Vorbis. Rendered from the files themselves,
//! apart from the player, so it can run on a thread while the song plays on. The global
//! EQ and the volume are for the speakers and stay out of it.

use rodio::{Decoder, Source};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::num::{NonZeroU8, NonZeroU32};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// Plays everywhere.
    #[default]
    Mp3,
    /// Open and free of patents and licences.
    Ogg,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::Mp3 => "mp3",
            Format::Ogg => "ogg",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Format::Mp3 => "MP3",
            Format::Ogg => "Ogg Vorbis",
        }
    }
}

/// What the export sheet was left saying, kept in settings for the next time.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Choices {
    /// At the speed it plays at, rather than as recorded.
    pub speed: bool,
    /// With its pitch shift.
    pub pitch: bool,
    /// The stems as heard (levels, mutes, solo), rather than all at full level.
    pub as_heard: bool,
    pub format: Format,
}

impl Default for Choices {
    fn default() -> Self {
        Choices { speed: true, pitch: true, as_heard: true, format: Format::Mp3 }
    }
}

/// A choice on the export sheet, each a pair: off (as recorded, all stems, MP3) or on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Speed,
    Pitch,
    Stems,
    Format,
}

impl Row {
    pub fn label(self) -> &'static str {
        match self {
            Row::Speed => "Speed",
            Row::Pitch => "Pitch",
            Row::Stems => "Stems",
            Row::Format => "Format",
        }
    }
}

/// The sheet's rows: speed and pitch only while they differ from the recording, stems
/// only for a stem set. The format always.
pub fn rows(slowed: bool, shifted: bool, stems: bool) -> Vec<Row> {
    [(Row::Speed, slowed), (Row::Pitch, shifted), (Row::Stems, stems), (Row::Format, true)]
        .into_iter()
        .filter_map(|(row, shown)| shown.then_some(row))
        .collect()
}

impl Choices {
    /// Whether `row` is on its second choice (as played, as heard, Ogg).
    pub fn get(&self, row: Row) -> bool {
        match row {
            Row::Speed => self.speed,
            Row::Pitch => self.pitch,
            Row::Stems => self.as_heard,
            Row::Format => self.format == Format::Ogg,
        }
    }

    pub fn set(&mut self, row: Row, on: bool) {
        match row {
            Row::Speed => self.speed = on,
            Row::Pitch => self.pitch = on,
            Row::Stems => self.as_heard = on,
            Row::Format => self.format = if on { Format::Ogg } else { Format::Mp3 },
        }
    }
}

/// One export: the recording's files, each at its gain, from `start` to `end` in its own
/// time, at `speed` (1 as recorded) and shifted by `semitones`.
#[derive(Debug, Clone)]
pub struct Job {
    pub files: Vec<PathBuf>,
    pub gains: Vec<f32>,
    pub start: Duration,
    pub end: Duration,
    pub speed: f32,
    pub semitones: f32,
    pub format: Format,
}

/// Interleaved samples, ready to encode.
#[derive(Debug)]
pub struct Rendered {
    pub channels: u16,
    pub rate: u32,
    pub samples: Vec<f32>,
}

/// The name a file is offered under: the song, then the section or "loop", without the
/// characters a file name can't hold on macOS or Windows.
pub fn file_name(song: &str, part: &str, format: Format) -> String {
    let name: String = format!("{song} - {part}")
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { '-' } else { c })
        .collect();
    format!("{}.{}", name.trim().trim_end_matches('.'), format.extension())
}

/// One pass of the job's stretch, summed, stretched and limited as it plays.
pub fn render(job: &Job) -> Result<Rendered, String> {
    if job.end <= job.start {
        return Err("there's nothing between the start and the end".into());
    }
    let mut stems = job
        .files
        .iter()
        .map(|path| {
            File::open(path).map_err(|e| e.to_string()).and_then(|f| Decoder::try_from(f).map_err(|e| e.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let first = stems.first().ok_or("nothing to export")?;
    let (channels, rate) = (first.channels().get(), first.sample_rate().get());
    if stems.iter().any(|s| s.channels().get() != channels || s.sample_rate().get() != rate) {
        return Err("the stems differ in channels or sample rate".into());
    }
    let frames = ((job.end - job.start).as_secs_f64() * rate as f64).round() as usize;
    let len = frames * channels as usize;
    let mut mix = vec![0.0f32; len];
    for (i, stem) in stems.iter_mut().enumerate() {
        let gain = job.gains.get(i).copied().unwrap_or(1.0);
        if gain == 0.0 {
            continue;
        }
        stem.try_seek(job.start).map_err(|e| e.to_string())?;
        // A stem that ends early is silence from there on.
        for (sum, v) in mix.iter_mut().zip(stem.by_ref().take(len)) {
            *sum += v * gain;
        }
    }
    let as_recorded = (job.speed - 1.0).abs() < 1e-4 && job.semitones == 0.0;
    // Stems summed at full level can pass full scale, as stretched peaks can: both are
    // bent under it rather than clipped.
    let samples = if as_recorded {
        mix.into_iter().map(crate::audio::soft_limit).collect()
    } else {
        let mut stretch = signalsmith_stretch::Stretch::preset_default(channels as u32, rate);
        stretch.set_transpose_factor_semitones(job.semitones, None);
        let out_frames = (frames as f64 / job.speed as f64).round() as usize;
        let mut out = vec![0.0f32; out_frames * channels as usize];
        if !stretch.exact(&mix, &mut out) {
            return Err("the slow-down couldn't process it".into());
        }
        out.into_iter().map(crate::audio::soft_limit).collect()
    };
    Ok(Rendered { channels, rate, samples })
}

/// The rendered audio as a file's bytes.
pub fn encode(audio: &Rendered, format: Format) -> Result<Vec<u8>, String> {
    match format {
        Format::Mp3 => mp3(audio),
        Format::Ogg => ogg(audio),
    }
}

/// Frames handed to an encoder at a time.
const CHUNK: usize = 4096;

fn mp3(audio: &Rendered) -> Result<Vec<u8>, String> {
    use mp3lame_encoder::{Bitrate, Builder, FlushNoGap, InterleavedPcm, MonoPcm, Quality};
    let failed = |e: &dyn std::fmt::Display| format!("the MP3 encoder failed: {e}");
    if audio.channels > 2 {
        return Err("MP3 holds at most two channels".into());
    }
    let mut builder = Builder::new().ok_or("the MP3 encoder didn't start")?;
    builder.set_num_channels(audio.channels as u8).map_err(|e| failed(&e))?;
    builder.set_sample_rate(audio.rate).map_err(|e| failed(&e))?;
    // MP3 goes up to 48 kHz: a hi-res file comes out at CD's rate.
    if audio.rate > 48_000 {
        builder.set_output_sample_rate(NonZeroU32::new(44_100)).map_err(|e| failed(&e))?;
    }
    builder.set_brate(Bitrate::Kbps256).map_err(|e| failed(&e))?;
    builder.set_quality(Quality::Good).map_err(|e| failed(&e))?;
    let mut encoder = builder.build().map_err(|e| failed(&e))?;
    let mut out = Vec::new();
    let channels = audio.channels as usize;
    for chunk in audio.samples.chunks(CHUNK * channels) {
        out.reserve(mp3lame_encoder::max_required_buffer_size(chunk.len() / channels));
        let done = match channels {
            1 => encoder.encode_to_vec(MonoPcm(chunk), &mut out),
            _ => encoder.encode_to_vec(InterleavedPcm(chunk), &mut out),
        };
        done.map_err(|e| failed(&e))?;
    }
    out.reserve(7200);
    encoder.flush_to_vec::<FlushNoGap>(&mut out).map_err(|e| failed(&e))?;
    Ok(out)
}

fn ogg(audio: &Rendered) -> Result<Vec<u8>, String> {
    use vorbis_rs::VorbisEncoderBuilder;
    let failed = |e: vorbis_rs::VorbisError| format!("the Ogg Vorbis encoder failed: {e}");
    let rate = NonZeroU32::new(audio.rate).ok_or("no sample rate")?;
    let channels = u8::try_from(audio.channels).ok().and_then(NonZeroU8::new).ok_or("too many channels")?;
    let mut encoder = VorbisEncoderBuilder::new(rate, channels, Vec::new()).map_err(failed)?.build().map_err(failed)?;
    let n = audio.channels as usize;
    let mut planes = vec![Vec::with_capacity(CHUNK); n];
    for chunk in audio.samples.chunks(CHUNK * n) {
        for plane in &mut planes {
            plane.clear();
        }
        for frame in chunk.chunks_exact(n) {
            for (plane, &v) in planes.iter_mut().zip(frame) {
                plane.push(v);
            }
        }
        encoder.encode_audio_block(&planes).map_err(failed)?;
    }
    encoder.finish().map_err(failed)
}

/// Renders, encodes and writes the job to `to`, through a temporary file beside it so
/// a failure leaves nothing half written.
pub fn export(job: &Job, to: &Path) -> Result<(), String> {
    let bytes = encode(&render(job)?, job.format)?;
    let tmp = to.with_extension(format!("{}.part", job.format.extension()));
    std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, to)).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(secs: f64) -> Duration {
        Duration::from_secs_f64(secs)
    }

    /// A stereo 16-bit WAV of `seconds` of a 440 Hz tone at `level`.
    fn tone(name: &str, seconds: f64, level: f32) -> PathBuf {
        let rate = 44_100u32;
        let frames = (seconds * rate as f64) as usize;
        let mut data = Vec::with_capacity(frames * 4);
        for i in 0..frames {
            let v = (level * (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * i16::MAX as f32) as i16;
            data.extend_from_slice(&v.to_le_bytes());
            data.extend_from_slice(&v.to_le_bytes());
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // PCM
        out.extend_from_slice(&2u16.to_le_bytes()); // stereo
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * 4).to_le_bytes());
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&data);
        let path = std::env::temp_dir().join(format!("odeion-export-{}-{name}.wav", std::process::id()));
        std::fs::write(&path, out).unwrap();
        path
    }

    fn job(files: Vec<PathBuf>, gains: Vec<f32>, speed: f32, semitones: f32) -> Job {
        Job { files, gains, start: s(1.0), end: s(3.0), speed, semitones, format: Format::Mp3 }
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0, |m, v| m.max(v.abs()))
    }

    #[test]
    fn the_sheet_asks_only_what_applies() {
        assert_eq!(rows(false, false, false), [Row::Format]);
        assert_eq!(rows(true, false, true), [Row::Speed, Row::Stems, Row::Format]);
        assert_eq!(rows(true, true, true).len(), 4);
        let mut choices = Choices::default();
        assert!(choices.get(Row::Speed) && choices.get(Row::Stems) && !choices.get(Row::Format));
        choices.set(Row::Format, true);
        choices.set(Row::Pitch, false);
        assert_eq!((choices.format, choices.pitch), (Format::Ogg, false));
    }

    #[test]
    fn a_file_name_holds_the_song_and_part_and_nothing_a_system_refuses() {
        assert_eq!(file_name("Wish You Were Here", "Solo", Format::Mp3), "Wish You Were Here - Solo.mp3");
        assert_eq!(file_name("AC/DC: T.N.T.", "loop 0:12?", Format::Ogg), "AC-DC- T.N.T. - loop 0-12-.ogg");
    }

    #[test]
    fn one_pass_is_as_long_as_the_stretch_and_slowed_is_longer() {
        let song = tone("pass", 4.0, 0.5);
        let as_recorded = render(&job(vec![song.clone()], vec![], 1.0, 0.0)).unwrap();
        assert_eq!((as_recorded.channels, as_recorded.rate), (2, 44_100));
        assert_eq!(as_recorded.samples.len(), 2 * 2 * 44_100);
        let slowed = render(&job(vec![song.clone()], vec![], 0.5, 0.0)).unwrap();
        assert_eq!(slowed.samples.len(), 2 * 4 * 44_100);
        // Shifted in pitch only: the same length, and still sounding.
        let shifted = render(&job(vec![song.clone()], vec![], 1.0, 2.0)).unwrap();
        assert_eq!(shifted.samples.len(), as_recorded.samples.len());
        assert!(peak(&shifted.samples[44_100..]) > 0.2);
        let _ = std::fs::remove_file(song);
    }

    #[test]
    fn stems_are_summed_at_their_gains() {
        let (a, b) = (tone("a", 4.0, 0.25), tone("b", 4.0, 0.25));
        let both = render(&job(vec![a.clone(), b.clone()], vec![1.0, 1.0], 1.0, 0.0)).unwrap();
        let one = render(&job(vec![a.clone(), b.clone()], vec![1.0, 0.0], 1.0, 0.0)).unwrap();
        assert!((peak(&both.samples) - 0.5).abs() < 0.01, "{}", peak(&both.samples));
        assert!((peak(&one.samples) - 0.25).abs() < 0.01, "{}", peak(&one.samples));
        let _ = (std::fs::remove_file(a), std::fs::remove_file(b));
    }

    #[test]
    fn both_formats_decode_back_to_the_pass() {
        let song = tone("encode", 4.0, 0.5);
        let audio = render(&job(vec![song.clone()], vec![], 1.0, 0.0)).unwrap();
        for format in [Format::Mp3, Format::Ogg] {
            let bytes = encode(&audio, format).unwrap();
            let decoded = Decoder::try_from(std::io::Cursor::new(bytes)).unwrap();
            assert_eq!(decoded.channels().get(), 2);
            let frames = decoded.count() / 2;
            // Encoders pad a little at the ends; the pass is two seconds.
            let seconds = frames as f64 / 44_100.0;
            assert!((seconds - 2.0).abs() < 0.1, "{} gave {seconds} s", format.name());
        }
        let _ = std::fs::remove_file(song);
    }

    #[test]
    fn an_export_writes_the_file_and_nothing_beside_it() {
        let song = tone("write", 4.0, 0.5);
        let to = std::env::temp_dir().join(format!("odeion-export-{}-out.ogg", std::process::id()));
        let job = Job { format: Format::Ogg, ..job(vec![song.clone()], vec![], 0.8, 0.0) };
        export(&job, &to).unwrap();
        assert!(std::fs::metadata(&to).unwrap().len() > 1000);
        assert!(!to.with_extension("ogg.part").exists());
        assert!(export(&Job { end: s(1.0), ..job }, &to).is_err());
        let _ = (std::fs::remove_file(song), std::fs::remove_file(to));
    }
}
