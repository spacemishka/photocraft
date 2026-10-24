//! Turn a decoded server image into a `Surface` covering the requested region.
//!
//! Servers are free to answer with a different resolution than the request (a 1024²
//! generation for a 700 × 420 region is normal for diffusion backends). Resampling here,
//! with bounds-checked indexing, keeps a mismatch from reaching `write_interleaved`,
//! whose length assertion would otherwise panic on a server-supplied size.

use photocraft_geom::Rect;
use photocraft_raster::Surface;

use crate::MlError;

/// The image as an RGBA8 surface filling `target` (nearest-neighbour when the sizes differ).
pub(crate) fn surface_from_image(img: &photocraft_codecs::Image, target: Rect) -> Result<Surface, MlError> {
    let (dw, dh) = (target.width(), target.height());
    if dw == 0 || dh == 0 {
        return Err(MlError::BadParam("empty result region".into()));
    }
    let (sw, sh) = img.dimensions();
    if sw == 0 || sh == 0 {
        return Err(MlError::InvalidOutput("empty image".into()));
    }
    let rgba = img.to_rgba8();
    let scaled = if sw == dw && sh == dh {
        rgba
    } else {
        resize_rgba8(&rgba, sw, sh, dw, dh)
    };
    let mut surf = Surface::new(photocraft_color::PixelFormat::RGBA8);
    surf.write_interleaved(target, &scaled);
    Ok(surf)
}

/// Nearest-neighbour RGBA8 rescale; missing pixels stay transparent black.
fn resize_rgba8(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<u8> {
    let (sw, sh, dw, dh) = (usize::try_from(sw).unwrap_or(0), usize::try_from(sh).unwrap_or(0), dw as usize, dh as usize);
    let mut out = vec![0u8; dw.saturating_mul(dh).saturating_mul(4)];
    if sw == 0 || sh == 0 {
        return out;
    }
    for y in 0..dh {
        let sy = y * sh / dh;
        for x in 0..dw {
            let sx = x * sw / dw;
            let si = (sy * sw + sx) * 4;
            let di = (y * dw + x) * 4;
            if let (Some(s), Some(d)) = (src.get(si..si + 4), out.get_mut(di..di + 4)) {
                d.copy_from_slice(s);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32, px: &[u8]) -> photocraft_codecs::Image {
        photocraft_codecs::Image::from_u8(w, h, photocraft_codecs::ChannelLayout::Rgba, px.to_vec()).expect("4 bytes per pixel")
    }

    #[test]
    fn a_different_server_resolution_is_resampled_not_panicked() {
        // 2 × 2 red image answered for a 3 × 5 region.
        let img = png(2, 2, &[255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255]);
        let target = Rect::new(10, 20, 13, 25);
        let surf = surface_from_image(&img, target).expect("resampled");
        assert_eq!(surf.read_region(target).len(), 3 * 5 * 4);
        assert!(surf.sample_channel(10, 20, 0) > 0.9, "red survives the scale");
    }

    #[test]
    fn a_matching_resolution_is_used_as_is() {
        let img = png(1, 1, &[0, 255, 0, 255]);
        let target = Rect::new(0, 0, 1, 1);
        let surf = surface_from_image(&img, target).expect("as is");
        assert!(surf.sample_channel(0, 0, 1) > 0.9, "green");
    }

    #[test]
    fn an_empty_region_is_rejected() {
        let img = png(1, 1, &[0, 0, 0, 0]);
        assert!(matches!(surface_from_image(&img, Rect::new(0, 0, 0, 0)), Err(MlError::BadParam(_))));
    }
}
