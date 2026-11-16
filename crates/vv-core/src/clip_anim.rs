use crate::Transform;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Entrance {
    Zoom,
    FromLeft,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Exit {
    Zoom,
    ToRight,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClipAnimations {
    pub in_asset: Option<String>,
    pub out_asset: Option<String>,
    pub combo_asset: Option<String>,
    pub entrance: Option<Entrance>,
    pub exit: Option<Exit>,
    pub pulse: bool,
    pub in_seconds: f64,
    pub out_seconds: f64,
    pub pulse_seconds: f64,
    pub combo_offset_seconds: f64,
    pub combo_length_seconds: Option<f64>,
}

impl Default for ClipAnimations {
    fn default() -> Self {
        Self {
            in_asset: None,
            out_asset: None,
            combo_asset: None,
            entrance: None,
            exit: None,
            pulse: false,
            in_seconds: 0.5,
            out_seconds: 0.5,
            pulse_seconds: 1.0,
            combo_offset_seconds: 0.0,
            combo_length_seconds: None,
        }
    }
}

impl ClipAnimations {
    /// Slot times use timeline seconds, independently of source trimming and speed.
    pub fn asset_times(&self, seconds: f64, length: f64) -> Vec<(&str, f32, f32)> {
        if !seconds.is_finite() || !length.is_finite() || length <= 0.0 {
            return Vec::new();
        }
        let duration = |v: f64| {
            if v.is_finite() {
                v.clamp(0.01, 60.0)
            } else {
                0.5
            }
        };
        let incoming = self
            .in_asset
            .as_ref()
            .map_or(0.0, |_| duration(self.in_seconds));
        let outgoing = self
            .out_asset
            .as_ref()
            .map_or(0.0, |_| duration(self.out_seconds));
        let fit = (length / (incoming + outgoing).max(0.01)).min(1.0);
        let mut out = Vec::new();
        let time = seconds.clamp(0.0, length);
        if let Some(id) = self.combo_asset.as_deref() {
            let offset = if self.combo_offset_seconds.is_finite() {
                self.combo_offset_seconds.max(0.0)
            } else {
                0.0
            };
            let local = time + offset;
            let length = self
                .combo_length_seconds
                .filter(|v| v.is_finite() && *v > 0.0)
                .unwrap_or(length)
                .max(0.01);
            out.push((id, (local / length).clamp(0.0, 1.0) as f32, local as f32));
        }
        if let Some(id) = self.in_asset.as_deref() {
            let d = incoming * fit;
            if time <= d {
                out.push((id, (time / d) as f32, time as f32));
            }
        }
        if let Some(id) = self.out_asset.as_deref() {
            let d = outgoing * fit;
            let local = time - (length - d);
            if local >= 0.0 {
                out.push((id, (local / d) as f32, local as f32));
            }
        }
        out
    }

    pub fn apply(&self, transform: &mut Transform, seconds: f64, length: f64, width: f32) {
        if self.entrance.is_none() && self.exit.is_none() && !self.pulse {
            return;
        }
        let time = seconds.clamp(0.0, length.max(0.0));
        let duration = |v: f64| {
            if v.is_finite() {
                v.clamp(0.01, 60.0)
            } else {
                0.5
            }
        };
        let incoming = if self.entrance.is_some() {
            duration(self.in_seconds)
        } else {
            0.0
        };
        let outgoing = if self.exit.is_some() {
            duration(self.out_seconds)
        } else {
            0.0
        };
        let fit = (length / (incoming + outgoing).max(0.01)).clamp(0.0, 1.0);
        let progress = |t: f64, d: f64| {
            let x = (t / (d * fit).max(0.000001)).clamp(0.0, 1.0) as f32;
            x * x * (3.0 - 2.0 * x)
        };
        let mut scale = 1.0;
        match self.entrance {
            Some(Entrance::Zoom) => scale *= progress(time, incoming).max(0.0001),
            Some(Entrance::FromLeft) => {
                transform.position[0] -= width * (1.0 - progress(time, incoming))
            }
            None => {}
        }
        match self.exit {
            Some(Exit::Zoom) => scale *= progress(length - time, outgoing).max(0.0001),
            Some(Exit::ToRight) => {
                transform.position[0] += width * (1.0 - progress(length - time, outgoing))
            }
            None => {}
        }
        if self.pulse {
            let phase = (time / duration(self.pulse_seconds) * std::f64::consts::TAU) as f32;
            scale *= 1.0 + 0.05 * (1.0 - phase.cos());
        }
        transform.zoom[0] *= scale;
        transform.zoom[1] *= scale;
    }
}

#[cfg(test)]
#[path = "tests/clip_anim.rs"]
mod tests;
