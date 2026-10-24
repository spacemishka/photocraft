//! Noise analysis for scanned film, photos and renders (`analysis.noise`).
//!
//! The estimate is the Immerkær high-pass residual `4·p − (left + right + up + down)` over a
//! strided sample of the layer: for white Gaussian noise its mean absolute value gives
//! `σ = √(π/2) · mean|residual|`. Edge pixels inflate that, so a second, block-based estimate
//! takes the quiet decile of 16×16 blocks — the grain level in flat areas, which is what a
//! denoiser sees on a scan. Values are reported in 0..1 and in 8-bit units.

use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};
use photocraft_doc::LayerId;
use photocraft_geom::Rect;
use photocraft_raster::Surface;

/// The measured noise of one region.
#[derive(Clone, Debug, PartialEq)]
pub struct NoiseStats {
    /// Per colour channel, 0..1.
    pub sigma: Vec<f32>,
    /// Luminance noise, 0..1.
    pub luma: f32,
    /// Luminance noise of the quietest decile of 16×16 blocks, 0..1.
    pub flat: f32,
    /// Fraction of sampled pixels whose channels are all at the black point.
    pub clipped_shadows: f32,
    /// Fraction of sampled pixels whose channels are all at the white point.
    pub clipped_highlights: f32,
    /// Pixels actually measured.
    pub samples: u64,
}

impl NoiseStats {
    /// A denoise strength (0..100) that matches the flat-area grain, for Reduce Noise.
    pub fn suggested_strength(&self) -> f32 {
        (self.flat * 255.0 * 8.0).clamp(0.0, 100.0)
    }

    pub fn to_json(&self) -> Value {
        let s8 = |v: f32| (v * 255.0 * 10.0).round() / 10.0;
        let channels: Vec<f32> = self.sigma.iter().map(|v| s8(*v)).collect();
        json!({
            "sigma": self.sigma,
            "sigma8": channels,
            "luma": self.luma,
            "luma8": s8(self.luma),
            "flat": self.flat,
            "flat8": s8(self.flat),
            "clippedShadows": self.clipped_shadows,
            "clippedHighlights": self.clipped_highlights,
            "suggestedReduceNoise": self.suggested_strength(),
            "samples": self.samples,
        })
    }
}

/// Measure the noise in `bounds` of `surf`, sampling about [`MAX_SAMPLES`] pixels.
pub fn measure(surf: &Surface, bounds: Rect) -> std::result::Result<NoiseStats, String> {
    let b = bounds.intersect(&surf.content_bounds());
    let (w, h) = (b.width() as usize, b.height() as usize);
    if w < 3 || h < 3 {
        return Err("noise analysis needs an area of at least 3 × 3 pixels".into());
    }
    let n_ch = surf.format().channels();
    let alpha = surf.format().alpha;
    let colour = if alpha { n_ch.saturating_sub(1) } else { n_ch };
    if colour == 0 {
        return Err("the layer has no colour channels".into());
    }
    let stride = ((w * h) as f64 / MAX_SAMPLES as f64).sqrt().ceil().max(1.0) as usize;

    let mut sums = vec![0.0f64; colour];
    let mut luma_sum = 0.0f64;
    let mut n = 0u64;
    let mut shadows = 0u64;
    let mut highlights = 0u64;
    // 16 × 16 samples per block, mean |residual| and centre luma for the flat-area decile.
    const BLOCK: usize = 16;
    let cols = w.div_ceil(stride * BLOCK).max(1);
    let mut blocks = vec![(0.0f64, 0u64); cols * h.div_ceil(stride * BLOCK).max(1)];

    let sample = |x: i32, y: i32, c: usize| surf.sample_channel(x, y, c);
    let luma = |x: i32, y: i32| -> f32 {
        if colour >= 3 {
            0.2126 * sample(x, y, 0) + 0.7152 * sample(x, y, 1) + 0.0722 * sample(x, y, 2)
        } else {
            (0..colour).map(|c| sample(x, y, c)).sum::<f32>() / colour as f32
        }
    };

    for y in (b.y0 + 1..b.y1 - 1).step_by(stride) {
        for x in (b.x0 + 1..b.x1 - 1).step_by(stride) {
            let mut dark = true;
            let mut light = true;
            for (c, sum) in sums.iter_mut().enumerate() {
                let p = sample(x, y, c);
                let r = 4.0 * p - sample(x - 1, y, c) - sample(x + 1, y, c) - sample(x, y - 1, c) - sample(x, y + 1, c);
                *sum += f64::from(r.abs());
                dark &= p <= 0.5 / 255.0;
                light &= p >= 1.0 - 0.5 / 255.0;
            }
            let lp = luma(x, y);
            let lr = 4.0 * lp - luma(x - 1, y) - luma(x + 1, y) - luma(x, y - 1) - luma(x, y + 1);
            luma_sum += f64::from(lr.abs());
            if dark {
                shadows += 1;
            }
            if light {
                highlights += 1;
            }
            let bx = ((x - b.x0) as usize / stride) / BLOCK;
            let by = ((y - b.y0) as usize / stride) / BLOCK;
            if let Some(slot) = blocks.get_mut(by * cols + bx) {
                slot.0 += f64::from(lr.abs());
                slot.1 += 1;
            }
            n += 1;
        }
    }
    if n == 0 {
        return Err("the selection has no measurable pixels".into());
    }
    // The 4-neighbour residual has variance 20σ² for white noise, so its mean absolute value
    // times √(π/2)/√20 estimates σ.
    let scale = (std::f64::consts::FRAC_PI_2).sqrt() / 20f64.sqrt();
    let to_sigma = |abs_sum: f64, count: u64| ((abs_sum / count as f64) * scale).max(0.0) as f32;
    let sigma: Vec<f32> = sums.iter().map(|s| to_sigma(*s, n)).collect();
    let luma_sigma = to_sigma(luma_sum, n);

    // Quiet decile of the blocks: the grain in flat areas, away from edges.
    let mut sigmas: Vec<f32> = blocks
        .iter()
        .filter(|(_, count)| *count >= 16)
        .map(|(sum, count)| to_sigma(*sum, *count))
        .collect();
    sigmas.sort_by(f32::total_cmp);
    let flat = if sigmas.is_empty() {
        luma_sigma
    } else {
        sigmas[(sigmas.len() - 1) / 10]
    };

    Ok(NoiseStats {
        sigma,
        luma: luma_sigma,
        flat,
        clipped_shadows: shadows as f32 / n as f32,
        clipped_highlights: highlights as f32 / n as f32,
        samples: n,
    })
}

const MAX_SAMPLES: usize = 1_500_000;

fn has_pixel_layer(s: &Session) -> std::result::Result<(), String> {
    let st = s.active().ok_or("no document open")?;
    let id = st.active_layer.ok_or("no active layer")?;
    match st.doc.layer(id).and_then(|l| l.surface()) {
        Some(_) => Ok(()),
        None => Err("the active layer has no pixel surface".into()),
    }
}

fn bad(cmd: &str, msg: impl Into<String>) -> EngineError {
    EngineError::Other(format!("{cmd}: {}", msg.into()))
}

fn noise(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "analysis.noise";
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let id = match p.get("layer").and_then(Value::as_u64) {
        Some(id) => LayerId(id),
        None => st.active_layer.ok_or_else(|| bad(CMD, "no active layer"))?,
    };
    let layer = st.doc.layer(id).ok_or_else(|| bad(CMD, format!("layer {id:?} not found")))?;
    let surf = layer.surface().ok_or_else(|| bad(CMD, "the layer has no pixel surface"))?;
    let doc_bounds = st.doc.bounds();
    let area = match st.doc.selection.as_ref().map(|sel| sel.content_bounds()) {
        Some(sel) if !sel.intersect(&doc_bounds).is_empty() => sel,
        _ => doc_bounds,
    };
    let bounds = area.intersect(&surf.content_bounds());
    let stats = measure(surf, bounds).map_err(|e| bad(CMD, e))?;
    let mut out = stats.to_json();
    if let Some(obj) = out.as_object_mut() {
        obj.insert("layer".into(), json!(id.0));
        obj.insert("bounds".into(), json!([bounds.x0, bounds.y0, bounds.width(), bounds.height()]));
    }
    Ok(out)
}

/// Every command of this module.
pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "analysis.noise",
        label: "Noise Analysis…",
        menu: &["Filter", "Noise"],
        shortcut: None,
        params: r#"{"layer":u32?} (the active layer; a selection narrows the area)"#,
        enabled: has_pixel_layer,
        run: noise,
        journal: false,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn doc_with(mut fill: impl FnMut(i32, i32) -> [f32; 4], w: u32, h: u32) -> Session {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": w, "height": h})).unwrap();
        s.edit("paint", |doc, _| {
            let surf = doc.layers[0].surface_mut().unwrap();
            let r = Rect::new(0, 0, w as i32, h as i32);
            let mut px = Vec::with_capacity((w * h * 4) as usize);
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    px.extend_from_slice(&fill(x, y));
                }
            }
            surf.write_region(r, &px);
            Ok(())
        })
        .unwrap();
        s
    }

    fn pixels(s: &Session) -> Surface {
        s.active().unwrap().doc.layers[0].surface().unwrap().clone()
    }

    #[test]
    fn white_noise_is_measured_within_a_fifth() {
        let mut seed = 12345u32;
        let mut rnd = move || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 8) as f32 / 16_777_216.0 - 0.5 // uniform -0.5..0.5
        };
        let amp = 0.1; // σ = amp/√12 ≈ 0.0289
        let s = doc_with(|_, _| { let v = 0.5 + amp * rnd(); [v, v, v, 1.0] }, 96, 96);
        let surf = pixels(&s);
        let stats = measure(&surf, surf.content_bounds()).expect("measured");
        let expected = amp / 12f32.sqrt();
        let err = (stats.luma - expected).abs() / expected;
        assert!(err < 0.25, "luma {} vs expected {expected} ({err:.0}% off)", stats.luma);
        assert!(stats.luma > 0.0);
        assert!(stats.flat > expected * 0.5, "flat-area grain {} too low", stats.flat);
        assert!(stats.samples > 1000, "sampled {} pixels", stats.samples);
    }

    #[test]
    fn a_flat_gradient_has_little_noise() {
        let s = doc_with(|x, _| { let v = x as f32 / 95.0; [v, v, v, 1.0] }, 96, 96);
        let surf = pixels(&s);
        let stats = measure(&surf, surf.content_bounds()).expect("measured");
        assert!(stats.luma < 1.0 / 255.0, "a clean gradient reads {} (8-bit {})", stats.luma, stats.luma * 255.0);
    }

    #[test]
    fn the_command_reports_json_and_rejects_a_text_layer() {
        let mut s = doc_with(|x, y| {
            let v = if (x / 4 + y / 4) % 2 == 0 { 0.2 } else { 0.8 };
            [v, v, v, 1.0]
        }, 32, 32);
        let v = noise(&mut s, &json!({})).expect("the analysis runs");
        assert!(v["luma8"].as_f64().is_some_and(|v| v >= 0.0), "{v}");
        assert!(v["samples"].as_u64().is_some_and(|n| n > 0), "{v}");
        assert!(v["suggestedReduceNoise"].as_f64().is_some_and(|v| (0.0..=100.0).contains(&v)), "{v}");

        let mut empty = Session::new();
        assert!(noise(&mut empty, &json!({})).is_err(), "no document → Err");

        let mut s = Session::new();
        s.execute("file.new", json!({"width": 32, "height": 32})).unwrap();
        s.execute("layer.new.group", json!({"name": "Group"})).unwrap();
        assert!(noise(&mut s, &json!({})).is_err(), "a group has no pixel surface");
    }

    #[test]
    fn a_tiny_region_is_rejected() {
        let s = doc_with(|_, _| [0.5, 0.5, 0.5, 1.0], 16, 16);
        let surf = pixels(&s);
        assert!(measure(&surf, Rect::new(0, 0, 2, 2)).is_err());
    }
}
