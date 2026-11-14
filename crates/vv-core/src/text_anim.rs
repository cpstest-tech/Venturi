//! Text animations as data: a `TextAnimations` descriptor (In / Out / Loop /
//! karaoke) lives in `TitleParams`, and `plan` turns it into per-unit effects
//! for one frame. Pure functions of (descriptor, text, `AnimState`): the same
//! frame gives the same result in preview and export.

use crate::easing::Easing;
use crate::model::Rgba;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub enum Granularity {
    #[default]
    Block,
    Line,
    Word,
    Char,
}

impl Granularity {
    pub const ALL: [Self; 4] = [Self::Block, Self::Line, Self::Word, Self::Char];

    pub fn id(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Line => "line",
            Self::Word => "word",
            Self::Char => "char",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UnitOrder {
    #[default]
    Forward,
    Backward,
    Random,
    FromCenter,
}

impl UnitOrder {
    pub const ALL: [Self; 4] = [
        Self::Forward,
        Self::Backward,
        Self::Random,
        Self::FromCenter,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Forward => "forward",
            Self::Backward => "backward",
            Self::Random => "random",
            Self::FromCenter => "center",
        }
    }
}

/// What a track animates. Distances are in font sizes, so an animation
/// does not depend on the text size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimProp {
    PosX,
    PosY,
    /// Uniform scale, 1 = unchanged.
    Scale,
    ScaleX,
    ScaleY,
    /// Degrees, clockwise.
    Rotation,
    /// Flip around the horizontal / vertical axis, degrees (drawn as a squash).
    TurnX,
    TurnY,
    /// 0-1, 1 = unchanged.
    Opacity,
    /// Blur radius, in font sizes.
    Blur,
    /// 0-1 mix towards the descriptor's `tint`.
    Tint,
    /// Extra letter spacing, in thousandths of an em.
    Tracking,
    /// 0-1 wipe, 1 = fully shown.
    Reveal,
    /// Scrolls the gradient fill (the whole block).
    GradientOffset,
    /// 0-1 halo in the `tint` colour.
    Glow,
}

impl AnimProp {
    pub const ALL: [Self; 15] = [
        Self::PosX,
        Self::PosY,
        Self::Scale,
        Self::ScaleX,
        Self::ScaleY,
        Self::Rotation,
        Self::TurnX,
        Self::TurnY,
        Self::Opacity,
        Self::Blur,
        Self::Tint,
        Self::Tracking,
        Self::Reveal,
        Self::GradientOffset,
        Self::Glow,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::PosX => "pos_x",
            Self::PosY => "pos_y",
            Self::Scale => "scale",
            Self::ScaleX => "scale_x",
            Self::ScaleY => "scale_y",
            Self::Rotation => "rotation",
            Self::TurnX => "turn_x",
            Self::TurnY => "turn_y",
            Self::Opacity => "opacity",
            Self::Blur => "blur",
            Self::Tint => "tint",
            Self::Tracking => "tracking",
            Self::Reveal => "reveal",
            Self::GradientOffset => "gradient_offset",
            Self::Glow => "glow",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Wave {
    #[default]
    Sine,
    Triangle,
    Saw,
    /// Smooth random.
    Noise,
    /// Random steps.
    Flicker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RevealDir {
    #[default]
    Right,
    Left,
    Down,
    Up,
}

/// In / Out: `from` -> `to` through the easing. Loop: oscillates between
/// them following `wave`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PropTrack {
    pub prop: AnimProp,
    pub from: f32,
    pub to: f32,
    /// Random amplitude added on top (stepped, deterministic).
    #[serde(default)]
    pub jitter: f32,
    #[serde(default)]
    pub wave: Wave,
}

impl PropTrack {
    pub const fn new(prop: AnimProp, from: f32, to: f32) -> Self {
        Self {
            prop,
            from,
            to,
            jitter: 0.0,
            wave: Wave::Sine,
        }
    }

    pub const fn jitter(mut self, amount: f32) -> Self {
        self.jitter = amount;
        self
    }

    pub const fn wave(mut self, wave: Wave) -> Self {
        self.wave = wave;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecorShape {
    /// Straight line under the text.
    Underline,
    /// Hand-drawn squiggle under the text.
    Scribble,
    /// Strip behind the text (lower thirds).
    Bar,
    /// Thin accent bar on the left, growing downwards.
    SideBar,
}

impl DecorShape {
    pub const ALL: [Self; 4] = [Self::Underline, Self::Scribble, Self::Bar, Self::SideBar];

    pub fn id(self) -> &'static str {
        match self {
            Self::Underline => "underline",
            Self::Scribble => "scribble",
            Self::Bar => "bar",
            Self::SideBar => "side_bar",
        }
    }
}

/// A shape drawn progressively (trim path) while the animation runs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Decor {
    pub shape: DecorShape,
    pub color: Rgba,
    /// Line / bar thickness, in font sizes.
    pub thickness: f32,
    /// Distance from the text, in font sizes (positive = away).
    pub offset: f32,
    /// Grows from the middle instead of from the left (bars).
    #[serde(default)]
    pub from_center: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimDescriptor {
    /// Preset it came from; informational.
    pub id: String,
    /// Seconds. Loop: the period.
    pub duration: f32,
    /// 0-0.95: share of the duration spread over the units (Loop: phase spread).
    pub stagger: f32,
    pub order: UnitOrder,
    pub granularity: Granularity,
    pub easing: Easing,
    pub tracks: Vec<PropTrack>,
    /// Colour of `Tint` and `Glow`.
    pub tint: Rgba,
    pub reveal_dir: RevealDir,
    /// In / Out only: plays the In curve backwards (mirror of the In).
    #[serde(default)]
    pub mirror: bool,
    #[serde(default)]
    pub decor: Option<Decor>,
}

impl Default for AnimDescriptor {
    fn default() -> Self {
        Self {
            id: String::new(),
            duration: 0.6,
            stagger: 0.0,
            order: UnitOrder::Forward,
            granularity: Granularity::Block,
            easing: Easing::QuadOut,
            tracks: Vec::new(),
            tint: Rgba::WHITE,
            reveal_dir: RevealDir::Right,
            mirror: false,
            decor: None,
        }
    }
}

/// Highlights the word being spoken.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Karaoke {
    pub color: Rgba,
    /// Scale of the active word, 1 = none.
    pub scale: f32,
    /// Words already spoken keep the colour.
    pub keep_past: bool,
    /// Start / end of every word, in ms from the start of the clip: the
    /// transcription words. Empty (or not matching the text): evenly spread.
    #[serde(default)]
    pub word_ms: Vec<[u32; 2]>,
}

impl Default for Karaoke {
    fn default() -> Self {
        Self {
            color: Rgba {
                r: 1.0,
                g: 0.85,
                b: 0.1,
                a: 1.0,
            },
            scale: 1.12,
            keep_past: false,
            word_ms: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TextAnimations {
    pub enter: Option<AnimDescriptor>,
    pub exit: Option<AnimDescriptor>,
    pub looped: Option<AnimDescriptor>,
    pub karaoke: Option<Karaoke>,
}

/// Where a frame is in the animations. Values stop changing once an
/// animation is over, so the renderer's caches keep hitting.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AnimState {
    /// Seconds into the In, up to its duration.
    pub t_in: f32,
    /// Seconds into the Out, 0 before it starts.
    pub t_out: f32,
    /// Seconds since the clip start.
    pub t_loop: f32,
    /// Index of the word being spoken, -1 = none yet.
    pub kara_word: i32,
    /// Clip length in seconds (karaoke without word times spreads over it).
    pub clip_len: f32,
}

impl TextAnimations {
    pub fn is_empty(&self) -> bool {
        self.enter.is_none()
            && self.exit.is_none()
            && self.looped.is_none()
            && self.karaoke.is_none()
    }

    /// The state at `t` seconds into a clip lasting `len`; `None` if the
    /// title looks like plain text at that moment.
    pub fn state_at(&self, text: &str, t: f32, len: f32) -> Option<AnimState> {
        if self.is_empty() {
            return None;
        }
        let mut state = AnimState::default();
        let mut live = false;
        if let Some(d) = &self.enter {
            let dur = d.duration.max(1e-3);
            state.t_in = t.clamp(0.0, dur);
            live |= t < dur || d.decor.is_some();
        }
        if let Some(d) = &self.exit {
            let dur = d.duration.max(1e-3);
            state.t_out = (t - (len - dur)).clamp(0.0, dur);
            live |= state.t_out > 0.0 || d.decor.is_some();
        }
        if self.looped.is_some() {
            state.t_loop = t.max(0.0);
            live = true;
        }
        if let Some(k) = &self.karaoke {
            state.kara_word = active_word(k, text, t, len);
            state.clip_len = len;
            live = true;
        }
        live.then_some(state)
    }

    /// Finest granularity in use: what the renderer splits the text by.
    pub fn finest(&self) -> Granularity {
        let mut finest = Granularity::Block;
        for d in [&self.enter, &self.exit, &self.looped]
            .into_iter()
            .flatten()
        {
            finest = finest.max(d.granularity);
        }
        if self.karaoke.is_some() {
            finest = finest.max(Granularity::Word);
        }
        finest
    }
}

fn active_word(k: &Karaoke, text: &str, t: f32, len: f32) -> i32 {
    let words = split_units(text, Granularity::Word).len();
    if words == 0 {
        return -1;
    }
    let ms = (t * 1000.0).round() as i64;
    if k.word_ms.len() == words {
        return k
            .word_ms
            .iter()
            .rposition(|[start, _]| i64::from(*start) <= ms)
            .map_or(-1, |i| i as i32);
    }
    if len <= 0.0 || t < 0.0 {
        return -1;
    }
    ((t / len * words as f32).floor() as i32).min(words as i32 - 1)
}

/// A run of text: byte range inside line `line`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextUnit {
    pub line: usize,
    pub start: usize,
    pub end: usize,
}

impl TextUnit {
    pub fn contains(&self, line: usize, byte: usize) -> bool {
        if self.end == usize::MAX {
            // A block over several lines.
            return line > self.line || (line == self.line && byte >= self.start);
        }
        self.line == line && (self.start..self.end).contains(&byte)
    }
}

/// Splits `text` (lines separated by `\n`) into units. Whitespace belongs
/// to no unit: it has no ink.
pub fn split_units(text: &str, granularity: Granularity) -> Vec<TextUnit> {
    let mut units = Vec::new();
    for (line, content) in text.split('\n').enumerate() {
        let first = content.find(|c: char| !c.is_whitespace());
        let last = content
            .char_indices()
            .rfind(|(_, c)| !c.is_whitespace())
            .map(|(i, c)| i + c.len_utf8());
        let (Some(first), Some(last)) = (first, last) else {
            continue;
        };
        match granularity {
            Granularity::Block | Granularity::Line => units.push(TextUnit {
                line,
                start: first,
                end: last,
            }),
            Granularity::Word => {
                let mut start = None;
                for (i, c) in content.char_indices() {
                    match (c.is_whitespace(), start) {
                        (false, None) => start = Some(i),
                        (true, Some(s)) => {
                            units.push(TextUnit {
                                line,
                                start: s,
                                end: i,
                            });
                            start = None;
                        }
                        _ => {}
                    }
                }
                if let Some(s) = start {
                    units.push(TextUnit {
                        line,
                        start: s,
                        end: content.len(),
                    });
                }
            }
            Granularity::Char => {
                for (i, c) in content.char_indices() {
                    if !c.is_whitespace() {
                        units.push(TextUnit {
                            line,
                            start: i,
                            end: i + c.len_utf8(),
                        });
                    }
                }
            }
        }
    }
    if granularity == Granularity::Block && units.len() > 1 {
        // One unit for the whole block; the renderer only needs its first glyph.
        let (first, last) = (units[0], units[units.len() - 1]);
        units = vec![TextUnit {
            line: first.line,
            start: first.start,
            end: if first.line == last.line {
                last.end
            } else {
                usize::MAX
            },
        }];
    }
    units
}

/// The effect on one group of glyphs at one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitFx {
    /// Font sizes, Y upwards.
    pub offset: [f32; 2],
    pub scale: [f32; 2],
    pub rotation: f32,
    pub opacity: f32,
    pub blur: f32,
    /// Colour and amount of the fill mixed towards it.
    pub tint: (Rgba, f32),
    pub glow: (Rgba, f32),
    /// Thousandths of an em, per character from the line centre.
    pub tracking: f32,
    pub reveal: f32,
    pub reveal_dir: RevealDir,
}

impl UnitFx {
    pub const REST: Self = Self {
        offset: [0.0; 2],
        scale: [1.0; 2],
        rotation: 0.0,
        opacity: 1.0,
        blur: 0.0,
        tint: (Rgba::WHITE, 0.0),
        glow: (Rgba::WHITE, 0.0),
        tracking: 0.0,
        reveal: 1.0,
        reveal_dir: RevealDir::Right,
    };

    pub fn is_rest(&self) -> bool {
        *self == Self::REST
    }

    fn apply(&mut self, prop: AnimProp, v: f32, d: &AnimDescriptor, turn: &mut [f32; 2]) {
        match prop {
            AnimProp::PosX => self.offset[0] += v,
            AnimProp::PosY => self.offset[1] += v,
            AnimProp::Scale => {
                self.scale[0] *= v;
                self.scale[1] *= v;
            }
            AnimProp::ScaleX => self.scale[0] *= v,
            AnimProp::ScaleY => self.scale[1] *= v,
            AnimProp::Rotation => self.rotation += v,
            AnimProp::TurnX => turn[0] += v,
            AnimProp::TurnY => turn[1] += v,
            AnimProp::Opacity => self.opacity *= v.clamp(0.0, 1.0),
            AnimProp::Blur => self.blur += v.max(0.0),
            AnimProp::Tint => {
                let amount = v.clamp(0.0, 1.0);
                if amount > self.tint.1 {
                    self.tint = (d.tint, amount);
                }
            }
            AnimProp::Tracking => self.tracking += v,
            AnimProp::Reveal => {
                self.reveal *= v.clamp(0.0, 1.0);
                self.reveal_dir = d.reveal_dir;
            }
            AnimProp::GradientOffset => {}
            AnimProp::Glow => {
                let amount = v.clamp(0.0, 1.0);
                if amount > self.glow.1 {
                    self.glow = (d.tint, amount);
                }
            }
        }
    }
}

/// What the renderer needs for one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct DecorDraw {
    pub decor: Decor,
    /// 0-1 share of the path drawn.
    pub progress: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimPlan {
    /// The glyph groups, in reading order; `fx` is parallel.
    pub groups: Vec<TextUnit>,
    pub fx: Vec<UnitFx>,
    pub gradient_offset: f32,
    pub decor: Vec<DecorDraw>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    In,
    Out,
    Loop,
}

/// Evaluates `anims` over `text` at `state`.
pub fn plan(anims: &TextAnimations, text: &str, state: &AnimState) -> AnimPlan {
    let groups = split_units(text, anims.finest());
    let mut fx = vec![UnitFx::REST; groups.len()];
    let mut turns = vec![[0.0f32; 2]; groups.len()];
    let mut gradient_offset = 0.0;
    let mut decor = Vec::new();

    let slots = [
        (Role::In, anims.enter.as_ref()),
        (Role::Out, anims.exit.as_ref()),
        (Role::Loop, anims.looped.as_ref()),
    ];
    for (role, descriptor) in slots {
        let Some(d) = descriptor else { continue };
        let units = split_units(text, d.granularity);
        let n = units.len();
        let ranks = ranks(d.order, n);
        let seed = hash_str(&d.id);
        for (gi, group) in groups.iter().enumerate() {
            let unit = units
                .iter()
                .position(|u| u.contains(group.line, group.start))
                .or_else(|| (d.granularity == Granularity::Block && n > 0).then_some(0));
            let Some(ui) = unit else { continue };
            let rank = ranks[ui];
            let dur = d.duration.max(1e-3);
            let t = match role {
                Role::In => state.t_in,
                Role::Out => state.t_out,
                Role::Loop => 0.0,
            };
            let p = unit_progress(d, if d.mirror { dur - t } else { t }, rank, n);
            let eased = d.easing.ease(p);
            let phase = state.t_loop / dur - rank as f32 / n.max(1) as f32 * d.stagger;
            let step = (match role {
                Role::Loop => state.t_loop,
                _ => t,
            } * 24.0)
                .floor() as i64;
            // Jitter dies down as the unit settles.
            let settle = match role {
                Role::Loop => 1.0,
                Role::Out if !d.mirror => p,
                _ => 1.0 - p,
            };
            for track in &d.tracks {
                let base = match role {
                    Role::Loop => {
                        let w = wave(track.wave, phase, seed ^ ui as u64);
                        track.from + (track.to - track.from) * w
                    }
                    _ => track.from + (track.to - track.from) * eased,
                };
                let noise_seed = seed ^ ((ui as u64) << 8) ^ track.prop as u64;
                let noise = signed_noise(noise_seed, step) * track.jitter * settle;
                let value = base + noise;
                if track.prop == AnimProp::GradientOffset {
                    gradient_offset += value;
                } else {
                    fx[gi].apply(track.prop, value, d, &mut turns[gi]);
                }
            }
        }
        if role != Role::Loop
            && let Some(decor_spec) = d.decor
        {
            let (t, dur) = if role == Role::In {
                (state.t_in, d.duration)
            } else {
                (state.t_out, d.duration)
            };
            let progress = d
                .easing
                .ease((t / dur.max(1e-3)).clamp(0.0, 1.0))
                .clamp(0.0, 1.0);
            let progress = if role == Role::In {
                progress
            } else {
                1.0 - progress
            };
            match decor
                .iter_mut()
                .find(|x: &&mut DecorDraw| x.decor == decor_spec)
            {
                Some(existing) => existing.progress *= progress,
                None => decor.push(DecorDraw {
                    decor: decor_spec,
                    progress,
                }),
            }
        }
    }

    for (f, turn) in fx.iter_mut().zip(&turns) {
        f.scale[1] *= turn[0].to_radians().cos().abs().max(0.02);
        f.scale[0] *= turn[1].to_radians().cos().abs().max(0.02);
    }

    if let Some(k) = &anims.karaoke {
        let words = split_units(text, Granularity::Word);
        for (gi, group) in groups.iter().enumerate() {
            let Some(w) = words
                .iter()
                .position(|u| u.contains(group.line, group.start))
            else {
                continue;
            };
            let w = w as i32;
            if w == state.kara_word {
                fx[gi].tint = (k.color, 1.0);
                fx[gi].scale = [fx[gi].scale[0] * k.scale, fx[gi].scale[1] * k.scale];
            } else if k.keep_past && w < state.kara_word {
                fx[gi].tint = (k.color, 1.0);
            }
        }
    }

    AnimPlan {
        groups,
        fx,
        gradient_offset,
        decor,
    }
}

/// Progress 0-1 of the unit at `rank` of `n` at `t` seconds.
fn unit_progress(d: &AnimDescriptor, t: f32, rank: usize, n: usize) -> f32 {
    let stagger = d.stagger.clamp(0.0, 0.95);
    let dur = d.duration.max(1e-3);
    let share = if n > 1 {
        rank as f32 / (n - 1) as f32
    } else {
        0.0
    };
    let delay = share * stagger * dur;
    let own = (dur * (1.0 - stagger)).max(1e-3);
    ((t - delay) / own).clamp(0.0, 1.0)
}

/// Position of every unit in the reveal order.
fn ranks(order: UnitOrder, n: usize) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..n).collect();
    match order {
        UnitOrder::Forward => {}
        UnitOrder::Backward => idx.reverse(),
        UnitOrder::Random => idx.sort_by_key(|&i| (splitmix(i as u64 + 0x9e37), i)),
        UnitOrder::FromCenter => {
            let mid = (n as f32 - 1.0) / 2.0;
            idx.sort_by(|&a, &b| {
                let (da, db) = ((a as f32 - mid).abs(), (b as f32 - mid).abs());
                da.total_cmp(&db).then(a.cmp(&b))
            });
        }
    }
    let mut ranks = vec![0; n];
    for (rank, unit) in idx.into_iter().enumerate() {
        ranks[unit] = rank;
    }
    ranks
}

fn wave(kind: Wave, phase: f32, seed: u64) -> f32 {
    let cycle = phase.rem_euclid(1.0);
    match kind {
        Wave::Sine => 0.5 - 0.5 * (cycle * std::f32::consts::TAU).cos(),
        Wave::Triangle => 1.0 - (2.0 * cycle - 1.0).abs(),
        Wave::Saw => cycle,
        Wave::Noise => {
            let (i, f) = (phase.floor() as i64, cycle);
            let (a, b) = (unit_noise(seed, i), unit_noise(seed, i + 1));
            let s = f * f * (3.0 - 2.0 * f);
            a + (b - a) * s
        }
        Wave::Flicker => unit_noise(seed, (phase * 8.0).floor() as i64),
    }
}

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn unit_noise(seed: u64, step: i64) -> f32 {
    (splitmix(seed ^ splitmix(step as u64)) >> 40) as f32 / (1u64 << 24) as f32
}

fn signed_noise(seed: u64, step: i64) -> f32 {
    unit_noise(seed, step) * 2.0 - 1.0
}

fn hash_str(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    })
}

#[cfg(test)]
#[path = "tests/text_anim.rs"]
mod tests;
