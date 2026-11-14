//! Animated stickers drawn in code. Each one is a function of the loop phase
//! `t` in 0..1 returning white SVG in a 100x100 box; the store renders the
//! frames and encodes them as a looping WebM, and the sticker takes any color
//! through the Colorize filter.

use std::f32::consts::{PI, TAU};

use crate::library_ribbon::StickerCategory;

pub(crate) const FRAME_RATE: u32 = 30;

type Draw = fn(f32) -> String;

#[derive(Clone, Copy)]
pub(crate) enum Body {
    Draw(Draw),
    /// A Phosphor icon (fill weight) moved by a `Motion`.
    Icon(&'static str, Motion),
}

#[derive(Clone, Copy)]
pub(crate) struct AnimDef {
    pub slug: &'static str,
    pub name: &'static str,
    pub category: StickerCategory,
    /// Length of one loop.
    pub secs: f32,
    /// Phase of the still shown on the tile.
    pub still: f32,
    pub body: Body,
}

#[derive(Clone, Copy)]
pub(crate) enum Motion {
    Pulse,
    Heartbeat,
    Bounce,
    /// Swings around the top (a bell).
    Ring,
    /// Swings around the bottom (a waving hand).
    Wave,
    Spin,
    Flicker,
    Shake,
    Pop,
    Float,
}

const WHITE: &str = "#ffffff";

fn ease_in_out(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn ease_out_back(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0) - 1.0;
    1.0 + x * x * (2.70158 * x + 1.70158)
}

/// 0 -> 1 -> 0 over `from..to`, smooth.
fn bump(t: f32, from: f32, to: f32) -> f32 {
    if t <= from || t >= to {
        return 0.0;
    }
    let u = (t - from) / (to - from);
    (PI * u).sin().powi(2)
}

/// 0 -> 1 with an overshoot, held, then back to 0: a loop that starts and ends small.
fn pop_scale(t: f32) -> f32 {
    if t < 0.3 {
        ease_out_back(t / 0.3)
    } else if t > 0.85 {
        let u = ((t - 0.85) / 0.15).clamp(0.0, 1.0);
        1.0 - u * u
    } else {
        1.0
    }
}

fn svg(body: &str) -> String {
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">{body}</svg>"#)
}

fn about(cx: f32, cy: f32, angle_deg: f32, scale: f32, dx: f32, dy: f32) -> String {
    format!(
        "translate({:.2} {:.2}) rotate({angle_deg:.2}) scale({scale:.3}) translate({:.2} {:.2})",
        cx + dx,
        cy + dy,
        -cx,
        -cy
    )
}

impl Motion {
    fn transform(self, t: f32) -> (String, f32) {
        let decay = |end: f32| (1.0 - (t / end).min(1.0)).powi(2);
        let mut opacity = 1.0;
        let transform = match self {
            Self::Pulse => about(
                50.0,
                50.0,
                0.0,
                1.0 + 0.14 * (0.5 - 0.5 * (TAU * t).cos()),
                0.0,
                0.0,
            ),
            Self::Heartbeat => {
                let s = 1.0 + 0.2 * bump(t, 0.0, 0.18) + 0.13 * bump(t, 0.2, 0.4);
                about(50.0, 50.0, 0.0, s, 0.0, 0.0)
            }
            Self::Bounce => {
                let u = (t * 2.0).fract();
                let up = 4.0 * u * (1.0 - u);
                let squash = 1.0 - 0.1 * (1.0 - up);
                about(50.0, 90.0, 0.0, 1.0, 0.0, -16.0 * up) + &format!(" scale(1 {squash:.3})")
            }
            Self::Ring => {
                let angle = 22.0 * (TAU * 4.0 * t).sin() * decay(0.7);
                about(50.0, 18.0, angle, 1.0, 0.0, 0.0)
            }
            Self::Wave => {
                let angle = 20.0 * (TAU * 2.0 * t).sin();
                about(50.0, 88.0, angle, 1.0, 0.0, 0.0)
            }
            Self::Spin => {
                let angle = 360.0 * ease_in_out(t / 0.7);
                let s = 1.0 + 0.1 * bump(t, 0.0, 0.7);
                about(50.0, 50.0, angle, s, 0.0, 0.0)
            }
            Self::Flicker => {
                let s = 1.0 + 0.07 * (TAU * 3.0 * t).sin() + 0.04 * (TAU * 7.0 * t).sin();
                about(50.0, 92.0, 2.0 * (TAU * 2.0 * t).sin(), 1.0, 0.0, 0.0)
                    + &format!(" scale(1 {s:.3})")
            }
            Self::Shake => {
                let dx = 5.0 * (TAU * 6.0 * t).sin() * decay(0.7);
                about(
                    50.0,
                    50.0,
                    4.0 * (TAU * 6.0 * t).sin() * decay(0.7),
                    1.0,
                    dx,
                    0.0,
                )
            }
            Self::Pop => about(50.0, 50.0, 0.0, 0.7 + 0.3 * pop_scale(t), 0.0, 0.0),
            Self::Float => {
                let u = t;
                opacity = (u * 6.0).min(1.0).min((1.0 - u) * 5.0).clamp(0.0, 1.0);
                about(50.0, 50.0, 6.0 * (TAU * t).sin(), 1.0, 0.0, 22.0 - 44.0 * u)
            }
        };
        (transform, opacity)
    }
}

/// The inside of an `<svg>` element.
fn inner(svg: &str) -> &str {
    let start = svg.find('>').map_or(0, |i| i + 1);
    let end = svg.rfind("</svg>").unwrap_or(svg.len());
    &svg[start..end.max(start)]
}

/// An icon (the downloaded Phosphor SVG) moved by `motion`.
pub(crate) fn icon_frame(icon_svg: &str, motion: Motion, t: f32) -> String {
    let (transform, opacity) = motion.transform(t);
    svg(&format!(
        r##"<g transform="{transform}" opacity="{opacity:.3}"><svg x="9" y="9" width="82" height="82" viewBox="0 0 256 256" fill="#ffffff" color="#ffffff">{}</svg></g>"##,
        inner(icon_svg)
    ))
}

type Point = (f32, f32);

fn sample(curve: impl Fn(f32) -> Point, progress: f32) -> Vec<Point> {
    let steps = ((80.0 * progress) as usize).max(2);
    (0..=steps)
        .map(|i| curve(progress * i as f32 / steps as f32))
        .collect()
}

fn polyline(points: &[Point], width: f32, opacity: f32) -> String {
    let list: Vec<String> = points
        .iter()
        .map(|(x, y)| format!("{x:.2},{y:.2}"))
        .collect();
    format!(
        r#"<polyline points="{}" fill="none" stroke="{WHITE}" stroke-width="{width}" stroke-linecap="round" stroke-linejoin="round" opacity="{opacity:.3}"/>"#,
        list.join(" ")
    )
}

fn arrow_head(tip: Point, angle: f32, size: f32, opacity: f32) -> String {
    format!(
        r#"<polygon points="0,0 {a:.1},{b:.1} {a:.1},{c:.1}" transform="translate({:.2} {:.2}) rotate({:.2})" fill="{WHITE}" stroke="{WHITE}" stroke-width="3" stroke-linejoin="round" opacity="{opacity:.3}"/>"#,
        tip.0,
        tip.1,
        angle.to_degrees(),
        a = -size,
        b = -size * 0.62,
        c = size * 0.62,
    )
}

/// Progress of a stroke that draws itself, holds, then fades.
fn draw_timeline(t: f32) -> (f32, f32) {
    let progress = ease_in_out(t / 0.55);
    let opacity = if t < 0.85 {
        1.0
    } else {
        1.0 - (t - 0.85) / 0.15
    };
    (progress, opacity)
}

fn drawn_arrow(curve: impl Fn(f32) -> Point + Copy, t: f32) -> String {
    let (progress, opacity) = draw_timeline(t);
    let progress = progress.max(0.02);
    let points = sample(curve, progress);
    let tip = *points.last().unwrap();
    let before = curve((progress - 0.02).max(0.0));
    let angle = (tip.1 - before.1).atan2(tip.0 - before.0);
    let head_in = ((progress - 0.82) / 0.18).clamp(0.0, 1.0);
    let mut body = polyline(&points, 8.0, opacity);
    if head_in > 0.0 {
        body += &arrow_head(tip, angle, 20.0 * head_in, opacity);
    }
    svg(&body)
}

fn lerp(a: Point, b: Point, u: f32) -> Point {
    (a.0 + (b.0 - a.0) * u, a.1 + (b.1 - a.1) * u)
}

fn bezier(p: [Point; 4], u: f32) -> Point {
    let a = lerp(p[0], p[1], u);
    let b = lerp(p[1], p[2], u);
    let c = lerp(p[2], p[3], u);
    lerp(lerp(a, b, u), lerp(b, c, u), u)
}

fn block_arrow(t: f32, angle: f32) -> String {
    let push = 12.0 * (0.5 - 0.5 * (TAU * t).cos()) - 6.0;
    svg(&format!(
        r#"<g transform="rotate({angle} 50 50) translate({push:.2} 0)"><polygon points="12,39 50,39 50,22 88,50 50,78 50,61 12,61" fill="{WHITE}" stroke="{WHITE}" stroke-width="6" stroke-linejoin="round"/></g>"#
    ))
}

fn chevrons(t: f32, angle: f32) -> String {
    let mut body = String::new();
    for i in 0..3 {
        let phase = (TAU * (t - i as f32 / 3.0)).cos().max(0.0).powi(2);
        let opacity = 0.2 + 0.8 * phase;
        let x = -24.0 + 24.0 * i as f32;
        body += &format!(
            r#"<polyline points="{:.1},18 {:.1},50 {:.1},82" fill="none" stroke="{WHITE}" stroke-width="12" stroke-linecap="round" stroke-linejoin="round" opacity="{opacity:.3}"/>"#,
            34.0 + x,
            58.0 + x,
            34.0 + x
        );
    }
    svg(&format!(
        r#"<g transform="rotate({angle} 50 50)">{body}</g>"#
    ))
}

fn pointer_click(t: f32) -> String {
    let press = bump(t, 0.25, 0.5);
    let s = 1.0 - 0.1 * press;
    let ring = ((t - 0.3) / 0.6).clamp(0.0, 1.0);
    let mut body = format!(
        r#"<g transform="translate(34 14) scale({s:.3}) translate(-34 -14)"><path d="M34 14 L34 74 L48 61 L58 85 L69 80 L59 57 L79 57 Z" fill="{WHITE}" stroke="{WHITE}" stroke-width="5" stroke-linejoin="round"/></g>"#
    );
    if ring > 0.0 && ring < 1.0 {
        body += &format!(
            r#"<circle cx="34" cy="14" r="{:.2}" fill="none" stroke="{WHITE}" stroke-width="5" opacity="{:.3}"/>"#,
            4.0 + 26.0 * ring,
            1.0 - ring
        );
    }
    svg(&body)
}

fn ripple(t: f32) -> String {
    let mut body = String::new();
    for k in 0..3 {
        let u = (t + k as f32 / 3.0).fract();
        body += &format!(
            r#"<circle cx="50" cy="50" r="{:.2}" fill="none" stroke="{WHITE}" stroke-width="{:.2}" opacity="{:.3}"/>"#,
            10.0 + 38.0 * u,
            1.5 + 6.0 * (1.0 - u),
            1.0 - u
        );
    }
    svg(&(body
        + &format!(
            r#"<circle cx="50" cy="50" r="{:.2}" fill="{WHITE}"/>"#,
            8.0 + 2.0 * (TAU * t).sin()
        )))
}

fn pulse_circle(t: f32) -> String {
    let s = 0.82 + 0.18 * (0.5 - 0.5 * (TAU * t).cos());
    svg(&format!(
        r#"<circle cx="50" cy="50" r="{:.2}" fill="{WHITE}"/><circle cx="50" cy="50" r="46" fill="none" stroke="{WHITE}" stroke-width="4" opacity="{:.3}"/>"#,
        34.0 * s,
        0.4 + 0.6 * (1.0 - s) * 5.0
    ))
}

fn spinner(t: f32) -> String {
    let mut body = String::new();
    for i in 0..12 {
        let angle = TAU * i as f32 / 12.0;
        let age = (i as f32 / 12.0 - t).rem_euclid(1.0);
        let opacity = (1.0 - age).powi(2).max(0.15);
        body += &format!(
            r#"<circle cx="{:.2}" cy="{:.2}" r="{:.2}" fill="{WHITE}" opacity="{opacity:.3}"/>"#,
            50.0 + 36.0 * angle.cos(),
            50.0 + 36.0 * angle.sin(),
            4.0 + 4.0 * opacity
        );
    }
    svg(&body)
}

const SPARKLE: &str = "M0 -1C0.08 -0.3 0.3 -0.08 1 0C0.3 0.08 0.08 0.3 0 1C-0.08 0.3 -0.3 0.08 -1 0C-0.3 -0.08 -0.08 -0.3 0 -1Z";

fn twinkle(t: f32) -> String {
    let mut body = String::new();
    for (i, (x, y, size)) in [(50.0, 50.0, 34.0), (22.0, 24.0, 15.0), (78.0, 74.0, 18.0)]
        .into_iter()
        .enumerate()
    {
        let u = (t + i as f32 * 0.33).fract();
        let s = size * (PI * u).sin().max(0.0).powi(2);
        let angle = 45.0 * u;
        if s > 0.5 {
            body += &format!(
                r#"<path d="{SPARKLE}" transform="translate({x} {y}) rotate({angle:.1}) scale({s:.2})" fill="{WHITE}"/>"#
            );
        }
    }
    svg(&body)
}

fn burst_pop(t: f32) -> String {
    let s = 0.3 + 0.7 * pop_scale(t);
    let wobble = 5.0 * (TAU * t).sin();
    svg(&format!(
        r#"<g transform="translate(50 50) rotate({wobble:.2}) scale({s:.3}) translate(-50 -50)"><polygon points="50,4 58,20 74,10 76,28 94,28 86,44 98,56 82,64 88,82 70,80 66,96 52,86 40,98 32,82 14,86 18,68 2,58 16,46 8,30 26,28 28,10 44,18" fill="{WHITE}" stroke="{WHITE}" stroke-width="3" stroke-linejoin="round"/></g>"#
    ))
}

fn spin_star(t: f32) -> String {
    let angle = 360.0 * ease_in_out(t / 0.8);
    let s = 1.0 + 0.12 * bump(t, 0.0, 0.8);
    svg(&format!(
        r#"<g transform="translate(50 50) rotate({angle:.1}) scale({s:.3}) translate(-50 -50)"><polygon points="50,6 62,36 94,38 69,58 78,90 50,72 22,90 31,58 6,38 38,36" fill="{WHITE}" stroke="{WHITE}" stroke-width="6" stroke-linejoin="round"/></g>"#
    ))
}

fn circle_scribble(t: f32) -> String {
    let curve = |u: f32| {
        let a = -PI / 2.0 - 0.4 + TAU * 1.12 * u;
        let k = 1.0 - 0.06 * u;
        (50.0 + 43.0 * k * a.cos(), 50.0 + 33.0 * k * a.sin())
    };
    let (progress, opacity) = draw_timeline(t);
    svg(&polyline(&sample(curve, progress.max(0.02)), 7.0, opacity))
}

fn underline_curve(u: f32, y: f32, amp: f32) -> Point {
    (8.0 + 84.0 * u, y + amp * (TAU * 1.5 * u).sin() * u)
}

fn underline(t: f32) -> String {
    let (progress, opacity) = draw_timeline(t);
    svg(&polyline(
        &sample(|u| underline_curve(u, 52.0, 6.0), progress.max(0.02)),
        9.0,
        opacity,
    ))
}

fn double_underline(t: f32) -> String {
    let (progress, opacity) = draw_timeline(t);
    let first = ease_in_out((t / 0.35).min(1.0)).max(0.02);
    let second = ease_in_out(((t - 0.2) / 0.35).clamp(0.0, 1.0));
    let _ = progress;
    let mut body = polyline(
        &sample(|u| underline_curve(u, 40.0, 3.0), first),
        8.0,
        opacity,
    );
    if second > 0.0 {
        body += &polyline(
            &sample(|u| underline_curve(u, 62.0, 3.0), second.max(0.02)),
            8.0,
            opacity,
        );
    }
    svg(&body)
}

fn wavy_underline(t: f32) -> String {
    let (progress, opacity) = draw_timeline(t);
    let curve = |u: f32| (8.0 + 84.0 * u, 50.0 + 14.0 * (TAU * 3.0 * u).sin());
    svg(&polyline(&sample(curve, progress.max(0.02)), 9.0, opacity))
}

fn box_draw(t: f32) -> String {
    let corners = [
        (10.0, 14.0),
        (90.0, 12.0),
        (92.0, 86.0),
        (8.0, 88.0),
        (11.0, 10.0),
    ];
    let curve = move |u: f32| {
        let scaled = u * 4.0;
        let i = (scaled as usize).min(3);
        lerp(corners[i], corners[i + 1], scaled - i as f32)
    };
    let (progress, opacity) = draw_timeline(t);
    svg(&polyline(&sample(curve, progress.max(0.02)), 8.0, opacity))
}

fn cross_out(t: f32) -> String {
    let (_, opacity) = draw_timeline(t);
    let first = ease_in_out(t / 0.28).max(0.02);
    let second = ease_in_out((t - 0.3) / 0.28);
    let mut body = polyline(
        &sample(|u| lerp((16.0, 16.0), (84.0, 84.0), u), first),
        11.0,
        opacity,
    );
    if second > 0.0 {
        body += &polyline(
            &sample(|u| lerp((84.0, 16.0), (16.0, 84.0), u), second.max(0.02)),
            11.0,
            opacity,
        );
    }
    svg(&body)
}

fn check_draw(t: f32) -> String {
    let curve = |u: f32| {
        if u < 0.35 {
            lerp((16.0, 54.0), (38.0, 76.0), u / 0.35)
        } else {
            lerp((38.0, 76.0), (86.0, 24.0), (u - 0.35) / 0.65)
        }
    };
    let (progress, opacity) = draw_timeline(t);
    svg(&polyline(&sample(curve, progress.max(0.02)), 13.0, opacity))
}

fn arc_arrow(t: f32) -> String {
    let p = [(12.0, 80.0), (20.0, 8.0), (80.0, 8.0), (88.0, 56.0)];
    drawn_arrow(move |u| bezier(p, u), t)
}

fn s_arrow(t: f32) -> String {
    let p = [(14.0, 22.0), (90.0, 12.0), (10.0, 88.0), (86.0, 78.0)];
    drawn_arrow(move |u| bezier(p, u), t)
}

fn straight_arrow(t: f32) -> String {
    drawn_arrow(|u| lerp((12.0, 82.0), (84.0, 20.0), u), t)
}

fn down_arrow(t: f32) -> String {
    let p = [(40.0, 10.0), (90.0, 30.0), (10.0, 52.0), (54.0, 88.0)];
    drawn_arrow(move |u| bezier(p, u), t)
}

fn loop_arrow(t: f32) -> String {
    let curve = |u: f32| {
        if u < 0.35 {
            lerp((8.0, 78.0), (36.0, 58.0), u / 0.35)
        } else if u < 0.8 {
            let a = PI * 0.8 + TAU * (u - 0.35) / 0.45;
            (46.0 + 14.0 * (a.cos() + 0.6), 44.0 + 18.0 * a.sin())
        } else {
            let from = {
                let a = PI * 0.8 + TAU;
                (46.0 + 14.0 * (a.cos() + 0.6), 44.0 + 18.0 * a.sin())
            };
            lerp(from, (92.0, 22.0), (u - 0.8) / 0.2)
        }
    };
    drawn_arrow(curve, t)
}

const RIGHT: f32 = 0.0;
const DOWN: f32 = 90.0;
const LEFT: f32 = 180.0;
const UP: f32 = 270.0;

macro_rules! draw {
    ($slug:literal, $name:literal, $cat:ident, $secs:expr, $still:expr, $f:expr) => {
        AnimDef {
            slug: $slug,
            name: $name,
            category: StickerCategory::$cat,
            secs: $secs,
            still: $still,
            body: Body::Draw($f),
        }
    };
}

macro_rules! icon {
    ($slug:literal, $name:literal, $cat:ident, $secs:expr, $still:expr, $icon:literal, $motion:ident) => {
        AnimDef {
            slug: $slug,
            name: $name,
            category: StickerCategory::$cat,
            secs: $secs,
            still: $still,
            body: Body::Icon($icon, Motion::$motion),
        }
    };
}

pub(crate) const DEFS: &[AnimDef] = &[
    draw!(
        "arrow-right-bounce",
        "Arrow right bounce",
        Arrows,
        1.0,
        0.5,
        |t| block_arrow(t, RIGHT)
    ),
    draw!(
        "arrow-down-bounce",
        "Arrow down bounce",
        Arrows,
        1.0,
        0.5,
        |t| block_arrow(t, DOWN)
    ),
    draw!(
        "arrow-left-bounce",
        "Arrow left bounce",
        Arrows,
        1.0,
        0.5,
        |t| block_arrow(t, LEFT)
    ),
    draw!(
        "arrow-up-bounce",
        "Arrow up bounce",
        Arrows,
        1.0,
        0.5,
        |t| block_arrow(t, UP)
    ),
    draw!("chevrons-right", "Chevrons right", Arrows, 1.2, 0.0, |t| {
        chevrons(t, RIGHT)
    }),
    draw!("chevrons-down", "Chevrons down", Arrows, 1.2, 0.0, |t| {
        chevrons(t, DOWN)
    }),
    draw!("chevrons-left", "Chevrons left", Arrows, 1.2, 0.0, |t| {
        chevrons(t, LEFT)
    }),
    draw!(
        "chevrons-up",
        "Chevrons up",
        Arrows,
        1.2,
        0.0,
        |t| chevrons(t, UP)
    ),
    draw!(
        "arrow-draw-arc",
        "Arrow draw arc",
        Arrows,
        2.0,
        0.7,
        arc_arrow
    ),
    draw!(
        "arrow-draw-wave",
        "Arrow draw wave",
        Arrows,
        2.0,
        0.7,
        s_arrow
    ),
    draw!(
        "arrow-draw-straight",
        "Arrow draw straight",
        Arrows,
        1.6,
        0.7,
        straight_arrow
    ),
    draw!(
        "arrow-draw-down",
        "Arrow draw down",
        Arrows,
        2.0,
        0.7,
        down_arrow
    ),
    draw!(
        "arrow-draw-loop",
        "Arrow draw loop",
        Arrows,
        2.4,
        0.7,
        loop_arrow
    ),
    draw!(
        "pointer-click",
        "Pointer click",
        Arrows,
        1.4,
        0.45,
        pointer_click
    ),
    draw!("ripple", "Ripple", Shapes, 1.6, 0.2, ripple),
    draw!(
        "pulse-circle",
        "Pulse circle",
        Shapes,
        1.2,
        0.5,
        pulse_circle
    ),
    draw!("spinner", "Spinner", Shapes, 1.0, 0.4, spinner),
    draw!("twinkle", "Twinkle", Shapes, 1.8, 0.45, twinkle),
    draw!("burst-pop", "Burst pop", Shapes, 1.6, 0.6, burst_pop),
    draw!("star-spin", "Star spin", Shapes, 1.8, 0.85, spin_star),
    draw!(
        "circle-scribble",
        "Circle scribble",
        Other,
        2.0,
        0.7,
        circle_scribble
    ),
    draw!(
        "underline-draw",
        "Underline draw",
        Other,
        1.6,
        0.7,
        underline
    ),
    draw!(
        "underline-double",
        "Double underline",
        Other,
        2.0,
        0.7,
        double_underline
    ),
    draw!(
        "underline-wavy",
        "Wavy underline",
        Other,
        1.8,
        0.7,
        wavy_underline
    ),
    draw!("box-draw", "Box draw", Other, 2.0, 0.7, box_draw),
    draw!("cross-out", "Cross out", Other, 1.8, 0.7, cross_out),
    draw!("check-draw", "Check draw", Other, 1.6, 0.7, check_draw),
    icon!(
        "heart-beat",
        "Heart beat",
        Social,
        1.2,
        0.05,
        "heart",
        Heartbeat
    ),
    icon!("bell-ring", "Bell ring", Social, 1.6, 0.1, "bell", Ring),
    icon!(
        "thumbs-up-bounce",
        "Thumbs up bounce",
        Social,
        1.2,
        0.25,
        "thumbs-up",
        Bounce
    ),
    icon!("star-pulse", "Star pulse", Social, 1.2, 0.5, "star", Pulse),
    icon!(
        "fire-flicker",
        "Fire flicker",
        Social,
        1.0,
        0.2,
        "fire",
        Flicker
    ),
    icon!(
        "smiley-bounce",
        "Smiley bounce",
        Social,
        1.2,
        0.25,
        "smiley",
        Bounce
    ),
    icon!(
        "hand-wave",
        "Hand wave",
        Social,
        1.6,
        0.1,
        "hand-waving",
        Wave
    ),
    icon!("gift-shake", "Gift shake", Social, 1.6, 0.1, "gift", Shake),
    icon!(
        "megaphone-shake",
        "Megaphone shake",
        Social,
        1.6,
        0.1,
        "megaphone",
        Shake
    ),
    icon!(
        "trophy-bounce",
        "Trophy bounce",
        Social,
        1.4,
        0.25,
        "trophy",
        Bounce
    ),
    icon!("crown-pop", "Crown pop", Social, 1.6, 0.4, "crown", Pop),
    icon!(
        "music-float",
        "Music float",
        Social,
        2.0,
        0.5,
        "music-notes",
        Float
    ),
    icon!(
        "lightning-pulse",
        "Lightning pulse",
        Other,
        1.0,
        0.5,
        "lightning",
        Pulse
    ),
    icon!("camera-pop", "Camera pop", Other, 1.6, 0.4, "camera", Pop),
    icon!(
        "star-spin-icon",
        "Star twirl",
        Other,
        2.0,
        0.85,
        "star",
        Spin
    ),
    icon!("eye-pulse", "Eye pulse", Other, 1.4, 0.5, "eye", Pulse),
];

/// SVG of the procedural sticker `def` at phase `t`; `icon_svg` is the
/// downloaded icon of an `Icon` body.
pub(crate) fn frame(def: &AnimDef, icon_svg: Option<&str>, t: f32) -> String {
    match (def.body, icon_svg) {
        (Body::Draw(draw), _) => draw(t),
        (Body::Icon(_, motion), Some(icon)) => icon_frame(icon, motion, t),
        (Body::Icon(..), None) => svg(""),
    }
}

#[cfg(test)]
#[path = "tests/sticker_anim.rs"]
mod tests;
