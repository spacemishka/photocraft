//! Image › Auto Correct: a non-destructive tone, colour and vibrance correction.
//!
//! The values come from the active layer's histogram (0.1 % clipped black/white points and
//! per-channel medians, as Auto Tone/Auto Color use) plus the mean HSV saturation; they are
//! written into **adjustment layers** above the layer, so nothing is baked into the pixels and
//! the result stays editable. `strength` blends every value towards "no change".

use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::edit_cmds::{clip_points, histograms, median};
use crate::{EngineError, Result, Session};
use photocraft_geom::Rect;
use photocraft_raster::Surface;

fn num(p: &Value, k: &str, d: f64) -> f64 {
    p.get(k).and_then(Value::as_f64).unwrap_or(d)
}

fn bad(cmd: &str, msg: impl Into<String>) -> EngineError {
    EngineError::Other(format!("{cmd}: {}", msg.into()))
}

/// Mean HSV saturation of the region, sampled to at most about one million points.
fn mean_saturation(surf: &Surface, area: Rect) -> f32 {
    let step = (((area.width() as u64 * area.height() as u64) / 1_000_000).max(1) as f64).sqrt() as i32;
    let step = step.max(1);
    let mut sum = 0.0f64;
    let mut n = 0u64;
    let mut y = area.y0;
    while y < area.y1 {
        let mut x = area.x0;
        while x < area.x1 {
            let (r, g, b) = (surf.sample_channel(x, y, 0), surf.sample_channel(x, y, 1), surf.sample_channel(x, y, 2));
            let max = r.max(g).max(b);
            let min = r.min(g).min(b);
            if max > 0.0 {
                sum += f64::from((max - min) / max);
                n += 1;
            }
            x += step;
        }
        y += step;
    }
    if n == 0 { 0.0 } else { (sum / n as f64) as f32 }
}

fn auto_correct(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "image.autoCorrect";
    let strength = (num(p, "strength", 100.0).clamp(0.0, 100.0) / 100.0) as f32;
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let id = st.active_layer.ok_or_else(|| bad(CMD, "no active layer"))?;
    let layer = st.doc.layer(id).ok_or_else(|| bad(CMD, "the active layer is missing"))?;
    let surf = layer.surface().ok_or_else(|| bad(CMD, "the active layer has no pixel surface"))?;
    let mode = st.doc.mode;
    let area = st
        .doc
        .selection
        .as_ref()
        .map_or(st.doc.bounds(), |m| m.content_bounds())
        .intersect(&st.doc.bounds())
        .intersect(&surf.content_bounds());
    if area.is_empty() {
        return Ok(json!({ "applied": false, "reason": "the layer has no pixels in the area" }));
    }

    let h = histograms(surf, area);
    let mut all = [0u64; 256];
    for c in &h {
        for (a, v) in all.iter_mut().zip(c) {
            *a += v;
        }
    }
    let (lo, hi) = clip_points(&all, 0.001);
    let lo = lo * strength;
    let hi = 1.0 - (1.0 - hi) * strength;
    let span = (hi - lo).max(1e-3);
    let meds: [f32; 3] = std::array::from_fn(|c| median(&h[c]));
    let target = (meds[0] + meds[1] + meds[2]) / 3.0;
    let gammas: [f32; 3] = std::array::from_fn(|c| {
        let t = ((meds[c] - lo) / span).clamp(0.02, 0.98);
        let want = ((target - lo) / span).clamp(0.02, 0.98);
        let g = (t.ln() / want.ln()).clamp(0.1, 9.99);
        1.0 + (g - 1.0) * strength
    });
    let saturation = if mode == photocraft_doc::ColorMode::Rgb { mean_saturation(surf, area) } else { 0.0 };
    // Only lift genuinely muted (but coloured) images: grey stays grey.
    let raw = (((0.35 - saturation) * 80.0).round()).clamp(0.0, 40.0);
    let vibrance = if saturation >= 0.05 && raw >= 3.0 { (raw * strength).round() } else { 0.0 };

    let levels = json!({
        "inBlack": (lo * 255.0).round(),
        "inWhite": (hi * 255.0).round(),
        "red": { "gamma": (gammas[0] * 1000.0).round() / 1000.0 },
        "green": { "gamma": (gammas[1] * 1000.0).round() / 1000.0 },
        "blue": { "gamma": (gammas[2] * 1000.0).round() / 1000.0 },
    });
    s.execute("layer.newAdjustmentLayer.levels", levels)?;
    let levels_id = s.active().and_then(|d| d.active_layer).map(|l| l.0);
    s.execute("layer.setProps", json!({ "name": "Auto Correct – Levels" }))?;

    let mut vibrance_id = None;
    if vibrance >= 3.0 {
        s.execute("layer.newAdjustmentLayer.vibrance", json!({ "vibrance": vibrance }))?;
        vibrance_id = s.active().and_then(|d| d.active_layer).map(|l| l.0);
        s.execute("layer.setProps", json!({ "name": "Auto Correct – Vibrance" }))?;
    }

    Ok(json!({
        "applied": true,
        "black": (lo * 255.0).round(),
        "white": (hi * 255.0).round(),
        "gamma": gammas.map(|g| (g * 1000.0).round() / 1000.0),
        "saturation": (saturation * 100.0).round() / 100.0,
        "vibrance": vibrance,
        "levelsLayer": levels_id,
        "vibranceLayer": vibrance_id,
    }))
}

fn has_pixel_layer(s: &Session) -> std::result::Result<(), String> {
    let st = s.active().ok_or("no document open")?;
    let id = st.active_layer.ok_or("no active layer")?;
    match st.doc.layer(id).and_then(|l| l.surface()) {
        Some(_) => Ok(()),
        None => Err("the active layer has no pixel surface".into()),
    }
}

/// Every command of this module.
pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "image.autoCorrect",
        label: "Auto Correct…",
        menu: &["Image"],
        shortcut: None,
        params: r#"{"strength":0..100=100} (blends every correction towards no change)"#,
        enabled: has_pixel_layer,
        run: auto_correct,
        journal: true,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with(px: impl Fn(i32, i32) -> [f32; 4], w: u32, h: u32) -> Session {
        let mut s = Session::new();
        s.execute("file.new", json!({ "width": w, "height": h })).unwrap();
        s.edit("paint", |doc, _| {
            let surf = doc.layers[0].surface_mut().unwrap();
            let r = Rect::new(0, 0, w as i32, h as i32);
            let mut data = Vec::with_capacity((w * h * 4) as usize);
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    data.extend_from_slice(&px(x, y));
                }
            }
            surf.write_region(r, &data);
            Ok(())
        })
        .unwrap();
        s
    }

    #[test]
    fn a_dull_dark_image_gets_levels_and_vibrance_layers() {
        // Low contrast, muted warm cast, dark.
        let s = doc_with(|x, y| {
            let v = 0.20 + 0.25 * (((x * 7 + y * 11) % 32) as f32 / 31.0);
            [v * 1.06, v * 0.98, v * 0.92, 1.0]
        }, 64, 64);
        let mut s = s;
        let layers = s.active().unwrap().doc.layers.len();
        let v = auto_correct(&mut s, &json!({})).expect("runs");
        assert_eq!(v["applied"], json!(true), "{v}");
        assert!(v["black"].as_f64().is_some_and(|b| b < 60.0), "black point raised: {v}");
        assert!(v["white"].as_f64().is_some_and(|w| w > 100.0), "white point pulled down: {v}");
        assert!(v["vibrance"].as_f64().is_some_and(|x| x > 0.0), "muted colours get vibrance: {v}");
        let st = s.active().unwrap();
        assert_eq!(st.doc.layers.len(), layers + 2, "two adjustment layers: {:?}", st.doc.layers.iter().map(|l| &l.name).collect::<Vec<_>>());
        assert_eq!(st.doc.layers[st.doc.layers.len() - 2].name, "Auto Correct – Levels");
        assert_eq!(st.doc.layers.last().unwrap().name, "Auto Correct – Vibrance");
    }

    #[test]
    fn a_neutral_mid_grey_gets_no_colour_or_vibrance_lift() {
        let mut s = doc_with(|_, _| [0.5, 0.5, 0.5, 1.0], 32, 32);
        let v = auto_correct(&mut s, &json!({})).expect("runs");
        assert_eq!(v["vibrance"], json!(0.0), "grey is not a muted colour: {v}");
        let g = v["gamma"].as_array().expect("gammas");
        for c in g {
            assert!((c.as_f64().unwrap_or(0.0) - 1.0).abs() < 0.02, "neutral stays neutral: {v}");
        }
        assert_eq!(s.active().unwrap().doc.layers.len(), 2, "only the Levels layer");
    }

    #[test]
    fn zero_strength_changes_nothing_visible() {
        let mut s = doc_with(|x, _| { let v = 0.2 + 0.2 * (x % 5) as f32 / 4.0; [v, v, v, 1.0] }, 32, 32);
        let v = auto_correct(&mut s, &json!({"strength": 0})).expect("runs");
        assert_eq!(v["black"], json!(0.0), "{v}");
        assert_eq!(v["white"], json!(255.0), "{v}");
        assert_eq!(v["vibrance"], json!(0.0), "{v}");
    }

    #[test]
    fn a_group_layer_is_refused() {
        let mut s = Session::new();
        s.execute("file.new", json!({ "width": 16, "height": 16 })).unwrap();
        s.execute("layer.new.group", json!({ "name": "Group" })).unwrap();
        assert!(auto_correct(&mut s, &json!({})).is_err());
    }
}
