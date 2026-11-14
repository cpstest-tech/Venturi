//! Where the audio actually holds a voice, from its loudness: whisper's word
//! times are fitted to it, so no word sits in a pause.

use std::ops::Range;

use crate::{SAMPLE_RATE, Word};

const FRAME_MS: u32 = 10;
/// Below this between the quiet floor and the loud peaks there is no clear
/// silence to find (music or noise under the voice): the times are left alone.
const MIN_RANGE_DB: f32 = 25.0;
/// Stop consonants and short breaths inside a word are not pauses.
const FILL_GAP_MS: u32 = 150;
const MIN_SOUND_MS: u32 = 50;
/// Soft onsets and tails (s, f, fading vowels) stay under the threshold.
const PAD_MS: u32 = 60;
/// A word whisper places wholly inside a pause this long is not in the audio.
const DROP_PAUSE_MS: u32 = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Speech {
    /// Voiced stretches in ms, in order and apart.
    pub spans: Vec<Range<u32>>,
    pub total_ms: u32,
}

/// Where 16 kHz mono `samples` hold a voice; `None` when the recording has
/// no quiet floor to tell it apart.
pub fn detect_speech(samples: &[f32]) -> Option<Speech> {
    let frame = (SAMPLE_RATE * FRAME_MS / 1000) as usize;
    let db: Vec<f32> = samples
        .chunks(frame)
        .map(|c| {
            let power = c.iter().map(|s| s * s).sum::<f32>() / c.len() as f32;
            10.0 * (power + 1e-12).log10()
        })
        .collect();
    if db.len() < 50 {
        return None;
    }
    let mut sorted = db.clone();
    sorted.sort_by(f32::total_cmp);
    let floor = sorted[sorted.len() / 10];
    let peak = sorted[sorted.len() * 95 / 100];
    if peak - floor < MIN_RANGE_DB {
        return None;
    }
    let threshold = floor + (0.3 * (peak - floor)).max(12.0);

    let mut spans: Vec<Range<u32>> = Vec::new();
    for (i, &level) in db.iter().enumerate() {
        if level < threshold {
            continue;
        }
        let (start, end) = (i as u32 * FRAME_MS, (i as u32 + 1) * FRAME_MS);
        match spans.last_mut() {
            Some(last) if start - last.end <= FILL_GAP_MS => last.end = end,
            _ => spans.push(start..end),
        }
    }
    let total = db.len() as u32 * FRAME_MS;
    spans.retain(|s| s.end - s.start >= MIN_SOUND_MS);
    let mut padded: Vec<Range<u32>> = Vec::new();
    for span in spans {
        let span = span.start.saturating_sub(PAD_MS)..(span.end + PAD_MS).min(total);
        match padded.last_mut() {
            Some(last) if span.start <= last.end => last.end = span.end,
            _ => padded.push(span),
        }
    }
    Some(Speech {
        spans: padded,
        total_ms: total,
    })
}

/// DTW can stack a few short words on one instant; shown one by one they
/// would flash for a single frame.
const MIN_WORD_GAP_MS: u32 = 80;

pub(crate) fn earliest_next(word: &Word) -> u32 {
    word.start_ms + MIN_WORD_GAP_MS
}

/// Upper bound of a word's length from its letters, for when nothing better
/// marks its end: a word before a pause would otherwise last the whole pause.
pub(crate) fn longest_word_ms(text: &str) -> u32 {
    300 + 90 * text.chars().count() as u32
}

/// Moves word starts that fall in a pause to where the voice resumes, drops
/// words placed wholly inside a long pause, and ends every word at the next
/// one or at the pause after it.
pub fn fit_to_speech(words: &mut Vec<Word>, speech: &Speech) {
    let (spans, total_ms) = (&speech.spans, speech.total_ms);
    let mut fitted: Vec<Word> = Vec::with_capacity(words.len());
    for i in 0..words.len() {
        let next_start = words.get(i + 1).map(|w| w.start_ms);
        let start = words[i]
            .start_ms
            .max(fitted.last().map_or(0, earliest_next));
        let start = match spans.iter().position(|s| s.end > start) {
            Some(n) if spans[n].start <= start => start,
            Some(n) if next_start.is_none_or(|next| next >= spans[n].start + MIN_WORD_GAP_MS) => {
                spans[n].start
            }
            found => {
                let after = found.unwrap_or(spans.len());
                let pause_start = after.checked_sub(1).map_or(0, |n| spans[n].end);
                let pause_end = found.map_or(total_ms, |n| spans[n].start);
                if pause_end - pause_start >= DROP_PAUSE_MS {
                    continue;
                }
                start
            }
        };
        let mut word = words[i].clone();
        word.start_ms = start;
        fitted.push(word);
    }
    for i in 0..fitted.len() {
        let start = fitted[i].start_ms;
        let voice_end = spans
            .iter()
            .find(|s| s.contains(&start))
            .map_or(u32::MAX, |s| s.end);
        let next = fitted.get(i + 1).map_or(u32::MAX, |w| w.start_ms);
        let end = voice_end
            .min(next)
            .min(start + longest_word_ms(&fitted[i].text));
        fitted[i].end_ms = end.max(start + 1);
    }
    *words = fitted;
}

#[cfg(test)]
#[path = "tests/speech.rs"]
mod tests;
