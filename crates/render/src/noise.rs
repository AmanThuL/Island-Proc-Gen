//! Blue-noise texture loader (§3.2 B3).
//!
//! Loads `assets/noise/blue_noise_2d_{size}.png`; falls back to a deterministic
//! hash-derived pattern when the file is absent so the pipeline keeps running.

use std::path::{Path, PathBuf};

use tracing::warn;

/// CPU-side 8-bit grayscale blue-noise texture.
#[derive(Debug, Clone)]
pub struct BlueNoiseTexture {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum NoiseLoadError {
    #[error("blue noise PNG at {path} failed to decode: {source}")]
    DecodeFailed {
        path: PathBuf,
        source: png::DecodingError,
    },

    #[error("blue noise PNG at {path} has dimensions {got:?}; expected {expected}x{expected}")]
    WrongSize {
        path: PathBuf,
        got: (u32, u32),
        expected: u32,
    },

    #[error("blue noise PNG at {path} is not 8-bit grayscale (color_type={color_type:?}, bit_depth={bit_depth:?})")]
    WrongFormat {
        path: PathBuf,
        color_type: png::ColorType,
        bit_depth: png::BitDepth,
    },
}

/// Load (or synthesize) a blue noise 2D texture of the given size.
///
/// Resolution order:
/// 1. Try `<repo>/assets/noise/blue_noise_2d_{size}.png` via `png::Decoder`.
/// 2. On any failure (missing file, decode error, size/format mismatch),
///    synthesize a deterministic hash-derived fallback and `tracing::warn!`.
///
/// The returned buffer is always `size * size` bytes, row-major.
pub fn load_blue_noise_2d(size: u32) -> BlueNoiseTexture {
    let path = repo_relative_path(size);
    match try_load_png(&path, size) {
        Ok(tex) => tex,
        Err(e) => {
            warn!(
                "blue noise load fell back to synthesized pattern: {e}. \
                 Download the real texture to {} for production use.",
                path.display()
            );
            synthesize_fallback(size)
        }
    }
}

fn repo_relative_path(size: u32) -> PathBuf {
    // CARGO_MANIFEST_DIR points at crates/render; go up two levels and into
    // assets/noise.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates
    p.pop(); // repo root
    p.push("assets");
    p.push("noise");
    p.push(format!("blue_noise_2d_{size}.png"));
    p
}

pub(crate) fn try_load_png(path: &Path, size: u32) -> Result<BlueNoiseTexture, NoiseLoadError> {
    let file = std::fs::File::open(path).map_err(|e| NoiseLoadError::DecodeFailed {
        path: path.to_path_buf(),
        source: png::DecodingError::IoError(e),
    })?;
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info().map_err(|e| NoiseLoadError::DecodeFailed {
        path: path.to_path_buf(),
        source: e,
    })?;

    let info = reader.info();
    if info.color_type != png::ColorType::Grayscale || info.bit_depth != png::BitDepth::Eight {
        return Err(NoiseLoadError::WrongFormat {
            path: path.to_path_buf(),
            color_type: info.color_type,
            bit_depth: info.bit_depth,
        });
    }
    if info.width != size || info.height != size {
        return Err(NoiseLoadError::WrongSize {
            path: path.to_path_buf(),
            got: (info.width, info.height),
            expected: size,
        });
    }

    let mut buf = vec![0_u8; reader.output_buffer_size()];
    reader
        .next_frame(&mut buf)
        .map_err(|e| NoiseLoadError::DecodeFailed {
            path: path.to_path_buf(),
            source: e,
        })?;

    Ok(BlueNoiseTexture {
        width: size,
        height: size,
        data: buf,
    })
}

fn synthesize_fallback(size: u32) -> BlueNoiseTexture {
    // Deterministic hash-derived pattern: splitmix-style mix of (x, y) to a
    // u64, take the top byte. Cheap, zero-correlation at the pixel scale,
    // reproducible across runs.
    let n = (size as usize) * (size as usize);
    let mut data = Vec::with_capacity(n);
    for y in 0..size {
        for x in 0..size {
            let key = ((x as u64) << 32) | (y as u64);
            let h = splitmix64(key);
            data.push((h >> 56) as u8);
        }
    }
    BlueNoiseTexture {
        width: size,
        height: size,
        data,
    }
}

// Vigna's splitmix64 mixing constants.
fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use tempfile::NamedTempFile;

    use super::*;

    fn write_grayscale_png(path: &std::path::Path, width: u32, height: u32, data: &[u8]) {
        let file = std::fs::File::create(path).unwrap();
        let mut enc = png::Encoder::new(file, width, height);
        enc.set_color(png::ColorType::Grayscale);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().unwrap();
        writer.write_image_data(data).unwrap();
    }

    fn write_rgb_png(path: &std::path::Path, width: u32, height: u32) {
        let file = std::fs::File::create(path).unwrap();
        let mut enc = png::Encoder::new(file, width, height);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().unwrap();
        let data = vec![0_u8; (width * height * 3) as usize];
        writer.write_image_data(&data).unwrap();
    }

    #[test]
    fn fallback_is_deterministic() {
        let a = synthesize_fallback(64);
        let b = synthesize_fallback(64);
        assert_eq!(a.data, b.data);
    }

    #[test]
    fn fallback_length_matches_size() {
        let tex = synthesize_fallback(64);
        assert_eq!(tex.data.len(), 64 * 64);
        assert_eq!(tex.width, 64);
        assert_eq!(tex.height, 64);
    }

    #[test]
    fn fallback_is_not_constant() {
        let tex = synthesize_fallback(32);
        let unique: HashSet<u8> = tex.data.iter().copied().collect();
        assert!(
            unique.len() >= 16,
            "expected at least 16 distinct values, got {}",
            unique.len()
        );
    }

    #[test]
    fn fallback_mean_is_reasonable() {
        let tex = synthesize_fallback(64);
        let sum: u64 = tex.data.iter().map(|&b| b as u64).sum();
        let mean = sum as f64 / tex.data.len() as f64;
        assert!(
            (96.0..=160.0).contains(&mean),
            "expected mean in [96, 160], got {mean:.2}"
        );
    }

    #[test]
    fn png_round_trip() {
        let original = synthesize_fallback(64);
        let tmp = NamedTempFile::new().unwrap();
        write_grayscale_png(tmp.path(), 64, 64, &original.data);
        let loaded = try_load_png(tmp.path(), 64).unwrap();
        assert_eq!(loaded.data, original.data);
    }

    #[test]
    fn wrong_dimensions_is_err() {
        let tmp = NamedTempFile::new().unwrap();
        let data = vec![0_u8; 32 * 32];
        write_grayscale_png(tmp.path(), 32, 32, &data);
        let err = try_load_png(tmp.path(), 64).unwrap_err();
        assert!(
            matches!(err, NoiseLoadError::WrongSize { got: (32, 32), expected: 64, .. }),
            "unexpected error variant: {err}"
        );
    }

    #[test]
    fn missing_file_returns_decode_failed() {
        let err = try_load_png(
            Path::new("/tmp/definitely_not_a_blue_noise_file.png"),
            64,
        )
        .unwrap_err();
        assert!(
            matches!(err, NoiseLoadError::DecodeFailed { .. }),
            "expected DecodeFailed, got: {err}"
        );
    }

    #[test]
    fn load_blue_noise_2d_returns_correct_dimensions() {
        let tex = load_blue_noise_2d(64);
        assert_eq!(tex.width, 64);
        assert_eq!(tex.height, 64);
        assert_eq!(tex.data.len(), 64 * 64);
    }

    #[test]
    fn wrong_format_rejected() {
        let tmp = NamedTempFile::new().unwrap();
        write_rgb_png(tmp.path(), 64, 64);
        let err = try_load_png(tmp.path(), 64).unwrap_err();
        assert!(
            matches!(err, NoiseLoadError::WrongFormat { .. }),
            "expected WrongFormat, got: {err}"
        );
    }
}
