//! Filter › Noise › Extract Grain: turn a photo's grain into a reusable overlay layer.
//!
//! The layer is the high-frequency residual (`pixel − local blur`, edges suppressed by the
//! blur's gradient), centred on 0.5 and blended in **Overlay**, so it is neutral where it is
//! added and can be copied into another document, scaled or masked. The numbers reported
//! (`sigma8`) are the same estimate `analysis.noise` uses.

use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};
use photocraft_algo::retouch::local_blur;
use photocraft_doc::{Layer, LayerContent};
use photocraft_raster::Surface;

fn num(p: &Value, k: &str, d: f64) -> f64 {
    p.get(k).and_then(Value::as_f64).unwrap_or(d)
}

fn flag(p: &Value, k: &str, d: bool) -> bool {
    p.get(k).and_then(Value::as_bool).unwrap_or(d)
}

fn bad(cmd: &str, msg: impl Into<String>) -> EngineError {
    EngineError::Other(format!("{cmd}: {}", msg.into()))
}

fn extract_grain(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "image.extractGrain";
    let radius = num(p, "radius", 4.0).clamp(1.0, 20.0) as f32;
    let strength = num(p, "strength", 100.0).clamp(0.0, 200.0) as f32 / 100.0;
    let monochrome = flag(p, "monochrome", true);

    let st = s.active().ok_or(EngineError::NoDocument)?;
    let id = match p.get("layer").and_then(Value::as_u64) {
        Some(id) => photocraft_doc::LayerId(id),
        None => st.active_layer.ok_or_else(|| bad(CMD, "no active layer"))?,
    };
    let layer = st.doc.layer(id).ok_or_else(|| bad(CMD, "the active layer is missing"))?;
    let surf = layer.surface().ok_or_else(|| bad(CMD, "the active layer has no pixel surface"))?;
    let area = st
        .doc
        .selection
        .as_ref()
        .map_or(st.doc.bounds(), |m| m.content_bounds())
        .intersect(&st.doc.bounds())
        .intersect(&surf.content_bounds());
    if area.is_empty() {
        return Err(bad(CMD, "the layer has no pixels in the area"));
    }
    let (w, h) = (area.width() as usize, area.height() as usize);
    if w < 3 || h < 3 {
        return Err(bad(CMD, "grain extraction needs at least 3 × 3 pixels"));
    }

    let fmt = surf.format();
    let ch = fmt.channels();
    let alpha = if fmt.alpha { Some(ch - 1) } else { None };
    let data = surf.read_region(area);
    let base = local_blur(&data, w, h, ch, alpha, (0, 0, w, h), radius);

    let luma = |src: &[f32], x: usize, y: usize| -> f32 {
        let i = (y * w + x) * ch;
        if ch >= 3 {
            0.2126 * src[i] + 0.7152 * src[i + 1] + 0.0722 * src[i + 2]
        } else {
            src[i]
        }
    };
    let mut sum_abs = 0.0f64;
    let mut counted = 0u64;
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * ch;
            // Edges in the blur are detail, not grain: fade the residual out there.
            let weight = if x > 0 && y > 0 && x + 1 < w && y + 1 < h {
                let gx = (luma(&base, x + 1, y) - luma(&base, x - 1, y)).abs();
                let gy = (luma(&base, x, y + 1) - luma(&base, x, y - 1)).abs();
                (1.0 - (gx + gy - 0.01) / 0.06).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let r = if monochrome && ch >= 3 {
                luma(&data, x, y) - luma(&base, x, y)
            } else {
                data[i] - base[i]
            };
            if weight > 0.5 {
                sum_abs += f64::from(r.abs());
                counted += 1;
            }
        }
    }
    // `p − blur` is a conservative grain estimate; 0.6 brings it near the 4-neighbour figure.
    let sigma = if counted > 0 { (sum_abs / counted as f64) as f32 * 0.6 } else { 0.0 };
    let sigma8 = (sigma * 255.0 * 10.0).round() / 10.0;
    // Normalise the extracted field to ±0.5 at 3σ, then let strength scale it.
    let scale = if sigma > 1e-5 { 0.5 / (3.0 * sigma) * strength } else { 0.0 };
    let mut out = vec![0.0f32; w * h * ch];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * ch;
            let weight = if x > 0 && y > 0 && x + 1 < w && y + 1 < h {
                let gx = (luma(&base, x + 1, y) - luma(&base, x - 1, y)).abs();
                let gy = (luma(&base, x, y + 1) - luma(&base, x, y - 1)).abs();
                (1.0 - (gx + gy - 0.01) / 0.06).clamp(0.0, 1.0)
            } else {
                0.0
            };
            for c in 0..ch {
                if Some(c) == alpha {
                    out[i + c] = 1.0;
                    continue;
                }
                let value = if monochrome && ch >= 3 {
                    luma(&data, x, y) - luma(&base, x, y)
                } else {
                    data[i + c] - base[i + c]
                };
                out[i + c] = (0.5 + value * weight * scale).clamp(0.0, 1.0);
            }
        }
    }
    let mut grain = Surface::new(fmt);
    grain.write_region(area, &out);

    let layer_id = s.edit("Extract Grain", |doc, active| {
        let name = doc.next_layer_name("Film Grain");
        let l = Layer::new(name, LayerContent::Raster(grain));
        let new_id = doc.insert_above(*active, l);
        *active = Some(new_id);
        Ok(new_id)
    })?;
    s.edit("Extract Grain", |doc, _| {
        let l = doc.layer_mut(layer_id).ok_or(EngineError::NoLayer(layer_id))?;
        l.blend = photocraft_doc::BlendMode::Overlay;
        Ok(())
    })?;

    Ok(json!({
        "layer": layer_id.0,
        "sigma8": sigma8,
        "scale": (scale * 1000.0).round() / 1000.0,
        "bounds": [area.x0, area.y0, area.width(), area.height()],
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
        id: "image.extractGrain",
        label: "Extract Grain…",
        menu: &["Filter", "Noise"],
        shortcut: None,
        params: r#"{"layer":u32?,"radius":1..20=4,"strength":0..200=100,"monochrome":bool=true} (creates a Film Grain layer with Overlay)"#,
        enabled: has_pixel_layer,
        run: extract_grain,
        journal: true,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_geom::Rect;

    fn doc_with(mut px: impl FnMut(i32, i32) -> [f32; 4], w: u32, h: u32) -> Session {
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
    fn a_grainy_image_yields_a_neutral_overlay_layer() {
        let mut seed = 99u32;
        let mut rnd = move || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 8) as f32 / 16_777_216.0 - 0.5
        };
        let mut s = doc_with(|x, y| {
            let v = 0.3 + 0.4 * (x + y) as f32 / 127.0 + 0.03 * rnd();
            [v, v * 0.9, v * 0.8, 1.0]
        }, 96, 96);
        let v = extract_grain(&mut s, &json!({})).expect("runs");
        assert!(v["sigma8"].as_f64().is_some_and(|s| s > 0.5), "grain measured: {v}");
        let st = s.active().unwrap();
        let l = st.doc.layers.last().expect("the grain layer");
        assert!(l.name.starts_with("Film Grain"), "{}", l.name);
        assert_eq!(l.blend, photocraft_doc::BlendMode::Overlay);
        // Neutral: the extracted field sits around 0.5.
        let px = l.surface().unwrap().read_region(st.doc.bounds());
        let mean = px.iter().step_by(4).sum::<f32>() / (px.len() / 4) as f32;
        assert!((mean - 0.5).abs() < 0.06, "neutral grain, mean {mean}");
    }

    #[test]
    fn a_clean_gradient_produces_no_grain() {
        let mut s = doc_with(|x, _| { let v = 0.2 + 0.6 * x as f32 / 95.0; [v, v, v, 1.0] }, 96, 96);
        let v = extract_grain(&mut s, &json!({})).expect("runs");
        assert!(v["sigma8"].as_f64().is_some_and(|s| s < 0.5), "no grain to extract: {v}");
    }

    #[test]
    fn a_group_layer_is_refused() {
        let mut s = Session::new();
        s.execute("file.new", json!({ "width": 16, "height": 16 })).unwrap();
        s.execute("layer.new.group", json!({ "name": "Group" })).unwrap();
        assert!(extract_grain(&mut s, &json!({})).is_err());
    }
}
