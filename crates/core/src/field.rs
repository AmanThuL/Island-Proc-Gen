//! 2D scalar field container and semantic aliases.
//!
//! The single generic container is [`ScalarField2D<T>`].  Two type aliases
//! are provided for semantic clarity:
//!
//! * [`MaskField2D`] — `ScalarField2D<u8>` with the convention `0 = false`,
//!   `1 = true`.  Never use `Vec<bool>`.
//! * [`VectorField2D`] — `ScalarField2D<[f32; 2]>` for wind / flow-direction
//!   data stored as plain POD `[x, y]` pairs (not `glam::Vec2`).
//!
//! Path / PNG I/O intentionally lives in `app::save_io` / `data::export`,
//! not here.

// ─── public types ────────────────────────────────────────────────────────────

/// Plain-old-data 2D field.  Row-major: element `(x, y)` lives at index
/// `y * width + x`.
#[derive(Debug, Clone)]
pub struct ScalarField2D<T> {
    pub data: Vec<T>,
    pub width: u32,
    pub height: u32,
}

/// Boolean-semantic mask.  Convention: `0 = false`, `1 = true`.
/// **Never** use `Vec<bool>` — the u8 representation keeps the layout
/// compatible with GPU texture uploads.
pub type MaskField2D = ScalarField2D<u8>;

/// 2D vector field (wind, flow direction, etc.).  Stored as `[f32; 2]`
/// (not `glam::Vec2`) so the struct is POD and layout-stable.
pub type VectorField2D = ScalarField2D<[f32; 2]>;

// ─── core impl ───────────────────────────────────────────────────────────────

impl<T: Copy + Default> ScalarField2D<T> {
    /// Allocate a new field filled with `T::default()`.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            data: vec![T::default(); (width as usize) * (height as usize)],
            width,
            height,
        }
    }

    /// Row-major flat index for `(x, y)`.
    ///
    /// # Panics
    /// Panics in debug mode if `x >= self.width` or `y >= self.height`.
    #[inline]
    pub fn index(&self, x: u32, y: u32) -> usize {
        debug_assert!(x < self.width, "x={x} out of bounds (width={})", self.width);
        debug_assert!(
            y < self.height,
            "y={y} out of bounds (height={})",
            self.height
        );
        y as usize * self.width as usize + x as usize
    }

    /// Read the value at `(x, y)`.
    #[inline]
    pub fn get(&self, x: u32, y: u32) -> T {
        self.data[self.index(x, y)]
    }

    /// Write `v` at `(x, y)`.
    #[inline]
    pub fn set(&mut self, x: u32, y: u32, v: T) {
        let idx = self.index(x, y);
        self.data[idx] = v;
    }

    /// Resize to `(new_width, new_height)` using nearest-neighbour sampling.
    /// Pixels that map outside the original bounds are filled with
    /// `T::default()`.
    pub fn resize_to(&self, new_width: u32, new_height: u32) -> Self {
        let mut out = Self::new(new_width, new_height);
        for ny in 0..new_height {
            for nx in 0..new_width {
                // nearest-neighbour: scale coordinates
                let sx = (nx as f64 * self.width as f64 / new_width as f64) as u32;
                let sy = (ny as f64 * self.height as f64 / new_height as f64) as u32;
                if sx < self.width && sy < self.height {
                    out.set(nx, ny, self.get(sx, sy));
                }
                // else: default already in place
            }
        }
        out
    }
}

// ─── f32-specific ops ────────────────────────────────────────────────────────

/// Statistics for a `ScalarField2D<f32>`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldStats {
    pub min: f32,
    pub max: f32,
    pub mean: f32,
    pub std: f32,
}

impl ScalarField2D<f32> {
    /// Compute min / max / mean / std over all elements.
    ///
    /// Returns `None` if the field is empty.
    pub fn stats(&self) -> Option<FieldStats> {
        if self.data.is_empty() {
            return None;
        }
        let n = self.data.len() as f64;
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        let mut sum = 0.0_f64;
        for &v in &self.data {
            if v < min {
                min = v;
            }
            if v > max {
                max = v;
            }
            sum += v as f64;
        }
        let mean = (sum / n) as f32;
        let variance = self
            .data
            .iter()
            .map(|&v| {
                let d = (v as f64) - (mean as f64);
                d * d
            })
            .sum::<f64>()
            / n;
        Some(FieldStats {
            min,
            max,
            mean,
            std: variance.sqrt() as f32,
        })
    }

    /// Bilinearly-interpolated sample.
    ///
    /// `u` and `v` are continuous grid coordinates in the ranges
    /// `[0, width-1]` and `[0, height-1]` respectively.  Both are clamped
    /// to those ranges before sampling.
    pub fn sample_bilinear(&self, u: f32, v: f32) -> f32 {
        // clamp to valid grid range
        let u = u.clamp(0.0, (self.width.saturating_sub(1)) as f32);
        let v = v.clamp(0.0, (self.height.saturating_sub(1)) as f32);

        let x0 = u.floor() as u32;
        let y0 = v.floor() as u32;
        // saturate at the last valid pixel
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);

        let tx = u - u.floor(); // fractional part in [0,1)
        let ty = v - v.floor();

        let q00 = self.get(x0, y0);
        let q10 = self.get(x1, y0);
        let q01 = self.get(x0, y1);
        let q11 = self.get(x1, y1);

        let top = q00 + (q10 - q00) * tx;
        let bot = q01 + (q11 - q01) * tx;
        top + (bot - top) * ty
    }
}

// ─── binary serialisation ─────────────────────────────────────────────────────
//
// Wire format (all numbers little-endian):
//
//   offset  size  field
//   ──────  ────  ─────────────────────────────────────────────
//       0     4   magic: b"IPGF"
//       4     4   format_version: u32  (currently 1)
//       8     1   dtype_tag: u8  (0=u8, 1=u32, 2=f32, 3=[f32;2])
//       9     4   width: u32
//      13     4   height: u32
//      17     *   row-major element bytes
//
// Total header = 17 bytes.

const MAGIC: [u8; 4] = *b"IPGF";
const FORMAT_VERSION: u32 = 1;
const HEADER_LEN: usize = 17;

// ─── dtype trait (private) ────────────────────────────────────────────────────

/// Crate-internal trait sealing `to_bytes` / `from_bytes` to the four
/// supported element types without exposing a public `Field` trait.
pub(crate) trait FieldDtype: Copy + Default + Sized {
    const DTYPE_TAG: u8;
    const ELEM_SIZE: usize;

    fn write_elem(v: Self, buf: &mut Vec<u8>);
    fn read_elem(bytes: &[u8]) -> Self;
}

impl FieldDtype for u8 {
    const DTYPE_TAG: u8 = 0;
    const ELEM_SIZE: usize = 1;

    fn write_elem(v: Self, buf: &mut Vec<u8>) {
        buf.push(v);
    }
    fn read_elem(bytes: &[u8]) -> Self {
        bytes[0]
    }
}

impl FieldDtype for u32 {
    const DTYPE_TAG: u8 = 1;
    const ELEM_SIZE: usize = 4;

    fn write_elem(v: Self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    fn read_elem(bytes: &[u8]) -> Self {
        u32::from_le_bytes(bytes[..4].try_into().unwrap())
    }
}

impl FieldDtype for f32 {
    const DTYPE_TAG: u8 = 2;
    const ELEM_SIZE: usize = 4;

    fn write_elem(v: Self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    fn read_elem(bytes: &[u8]) -> Self {
        f32::from_le_bytes(bytes[..4].try_into().unwrap())
    }
}

impl FieldDtype for [f32; 2] {
    const DTYPE_TAG: u8 = 3;
    const ELEM_SIZE: usize = 8;

    fn write_elem(v: Self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&v[0].to_le_bytes());
        buf.extend_from_slice(&v[1].to_le_bytes());
    }
    fn read_elem(bytes: &[u8]) -> Self {
        let x = f32::from_le_bytes(bytes[..4].try_into().unwrap());
        let y = f32::from_le_bytes(bytes[4..8].try_into().unwrap());
        [x, y]
    }
}

// ─── errors ──────────────────────────────────────────────────────────────────

/// Error returned by [`ScalarField2D::from_bytes`].
#[derive(Debug, thiserror::Error)]
pub enum FieldDecodeError {
    #[error("too short: expected at least {expected} bytes, got {actual}")]
    TooShort { expected: usize, actual: usize },

    #[error("bad magic: expected IPGF, got {0:?}")]
    BadMagic([u8; 4]),

    #[error("unsupported format version {0}")]
    UnsupportedVersion(u32),

    #[error("dtype mismatch: expected {expected}, got {actual}")]
    DtypeMismatch { expected: u8, actual: u8 },

    #[error(
        "length mismatch: header says {expected_elems} elements, body has {actual_bytes} bytes"
    )]
    LengthMismatch {
        expected_elems: u64,
        actual_bytes: usize,
    },
}

// ─── to_bytes / from_bytes (generic over FieldDtype) ─────────────────────────

// The `FieldDtype` bound is intentionally not public — it seals to_bytes /
// from_bytes to the four concrete types this crate controls.
#[allow(private_bounds)]
impl<T: FieldDtype> ScalarField2D<T> {
    /// Serialise to the IPGF binary format (little-endian, hand-rolled).
    pub fn to_bytes(&self) -> Vec<u8> {
        let n_elems = self.data.len();
        let body_len = n_elems * T::ELEM_SIZE;
        let mut buf = Vec::with_capacity(HEADER_LEN + body_len);

        // header
        buf.extend_from_slice(&MAGIC);
        buf.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        buf.push(T::DTYPE_TAG);
        buf.extend_from_slice(&self.width.to_le_bytes());
        buf.extend_from_slice(&self.height.to_le_bytes());

        // body
        for &elem in &self.data {
            T::write_elem(elem, &mut buf);
        }
        buf
    }

    /// Deserialise from the IPGF binary format.
    ///
    /// Returns `Err` (never panics) on any header / length mismatch.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, FieldDecodeError> {
        // 1. minimum header length
        if bytes.len() < HEADER_LEN {
            return Err(FieldDecodeError::TooShort {
                expected: HEADER_LEN,
                actual: bytes.len(),
            });
        }

        // 2. magic
        let magic: [u8; 4] = bytes[0..4]
            .try_into()
            .expect("HEADER_LEN guard above ensures ≥17 bytes");
        if magic != MAGIC {
            return Err(FieldDecodeError::BadMagic(magic));
        }

        // 3. format version
        let version = u32::from_le_bytes(
            bytes[4..8]
                .try_into()
                .expect("HEADER_LEN guard above ensures ≥17 bytes"),
        );
        if version != FORMAT_VERSION {
            return Err(FieldDecodeError::UnsupportedVersion(version));
        }

        // 4. dtype tag
        let tag = bytes[8];
        if tag != T::DTYPE_TAG {
            return Err(FieldDecodeError::DtypeMismatch {
                expected: T::DTYPE_TAG,
                actual: tag,
            });
        }

        // 5. dimensions
        let width = u32::from_le_bytes(
            bytes[9..13]
                .try_into()
                .expect("HEADER_LEN guard above ensures ≥17 bytes"),
        );
        let height = u32::from_le_bytes(
            bytes[13..17]
                .try_into()
                .expect("HEADER_LEN guard above ensures ≥17 bytes"),
        );
        let expected_elems = width as u64 * height as u64;
        let body = &bytes[HEADER_LEN..];

        // 6. body length
        let expected_body_bytes = expected_elems
            .checked_mul(T::ELEM_SIZE as u64)
            .expect("overflow computing expected body size")
            as usize;
        if body.len() != expected_body_bytes {
            return Err(FieldDecodeError::LengthMismatch {
                expected_elems,
                actual_bytes: body.len(),
            });
        }

        // 7. decode elements
        let mut data = Vec::with_capacity(expected_elems as usize);
        for chunk in body.chunks_exact(T::ELEM_SIZE) {
            data.push(T::read_elem(chunk));
        }

        Ok(Self {
            data,
            width,
            height,
        })
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // 1. set / get roundtrip
    #[test]
    fn set_then_get_roundtrip() {
        let mut f = ScalarField2D::<f32>::new(8, 4);
        f.set(3, 2, 1.5);
        assert_eq!(f.get(3, 2), 1.5);
    }

    // 2. stats on a linear gradient
    #[test]
    fn stats_linear_gradient() {
        const W: u32 = 8;
        const H: u32 = 4;
        let mut f = ScalarField2D::<f32>::new(W, H);
        for y in 0..H {
            for x in 0..W {
                f.set(x, y, x as f32);
            }
        }
        let s = f.stats().expect("non-empty field should have stats");
        let eps = 1e-4_f32;
        assert!((s.min - 0.0).abs() < eps, "min={}", s.min);
        assert!((s.max - (W - 1) as f32).abs() < eps, "max={}", s.max);
        assert!(
            (s.mean - (W - 1) as f32 / 2.0).abs() < eps,
            "mean={}",
            s.mean
        );
    }

    // 3. bilinear at integer grid points equals get()
    #[test]
    fn bilinear_at_grid_points_equals_get() {
        let mut f = ScalarField2D::<f32>::new(6, 5);
        // fill with recognisable values
        for y in 0..5_u32 {
            for x in 0..6_u32 {
                f.set(x, y, (x * 7 + y * 13) as f32);
            }
        }
        for y in 0..5_u32 {
            for x in 0..6_u32 {
                let expected = f.get(x, y);
                let sampled = f.sample_bilinear(x as f32, y as f32);
                assert!(
                    (sampled - expected).abs() < 1e-5,
                    "at ({x},{y}): expected {expected}, got {sampled}"
                );
            }
        }
    }

    // helpers for round-trip tests
    fn make_f32_field() -> ScalarField2D<f32> {
        let mut f = ScalarField2D::<f32>::new(3, 2);
        let vals = [1.0_f32, 2.5, -3.0, 0.0, 100.0, f32::NAN];
        for (i, &v) in vals.iter().enumerate() {
            f.data[i] = v;
        }
        f
    }

    fn f32_eq_or_nan(a: f32, b: f32) -> bool {
        (a == b) || (a.is_nan() && b.is_nan())
    }

    // 4. f32 round-trip
    #[test]
    fn to_from_bytes_roundtrip_f32() {
        let orig = make_f32_field();
        let bytes = orig.to_bytes();
        let decoded = ScalarField2D::<f32>::from_bytes(&bytes).expect("should decode");
        assert_eq!(decoded.width, orig.width);
        assert_eq!(decoded.height, orig.height);
        for (a, b) in orig.data.iter().zip(decoded.data.iter()) {
            assert!(f32_eq_or_nan(*a, *b), "mismatch: {a} vs {b}");
        }
    }

    // 5. u8 / MaskField2D round-trip
    #[test]
    fn to_from_bytes_roundtrip_u8() {
        let mut orig = MaskField2D::new(4, 4);
        for i in 0..16_u32 {
            orig.data[i as usize] = (i % 3) as u8;
        }
        let bytes = orig.to_bytes();
        let decoded = MaskField2D::from_bytes(&bytes).expect("should decode");
        assert_eq!(decoded.width, orig.width);
        assert_eq!(decoded.height, orig.height);
        assert_eq!(decoded.data, orig.data);
    }

    // 6. u32 round-trip
    #[test]
    fn to_from_bytes_roundtrip_u32() {
        let mut orig = ScalarField2D::<u32>::new(3, 3);
        for i in 0..9_u32 {
            orig.data[i as usize] = i * 1_000_000;
        }
        let bytes = orig.to_bytes();
        let decoded = ScalarField2D::<u32>::from_bytes(&bytes).expect("should decode");
        assert_eq!(decoded.data, orig.data);
    }

    // 7. [f32;2] / VectorField2D round-trip
    #[test]
    fn to_from_bytes_roundtrip_vec2() {
        let mut orig = VectorField2D::new(2, 2);
        orig.data[0] = [1.0, -1.0];
        orig.data[1] = [0.5, 0.5];
        orig.data[2] = [-99.0, 3.14];
        orig.data[3] = [0.0, 0.0];
        let bytes = orig.to_bytes();
        let decoded = VectorField2D::from_bytes(&bytes).expect("should decode");
        assert_eq!(decoded.data, orig.data);
    }

    // 8. bad magic
    #[test]
    fn from_bytes_rejects_bad_magic() {
        let mut bytes = ScalarField2D::<f32>::new(1, 1).to_bytes();
        bytes[0] = b'X'; // corrupt magic
        match ScalarField2D::<f32>::from_bytes(&bytes) {
            Err(FieldDecodeError::BadMagic(_)) => {}
            other => panic!("expected BadMagic, got {other:?}"),
        }
    }

    // 9. bad format version
    #[test]
    fn from_bytes_rejects_bad_version() {
        let mut bytes = ScalarField2D::<f32>::new(1, 1).to_bytes();
        // bytes[4..8] = format_version; set to 999
        bytes[4..8].copy_from_slice(&999_u32.to_le_bytes());
        match ScalarField2D::<f32>::from_bytes(&bytes) {
            Err(FieldDecodeError::UnsupportedVersion(999)) => {}
            other => panic!("expected UnsupportedVersion(999), got {other:?}"),
        }
    }

    // 10. bad dtype tag
    #[test]
    fn from_bytes_rejects_bad_dtype() {
        let mut bytes = ScalarField2D::<f32>::new(1, 1).to_bytes();
        bytes[8] = 99; // dtype_tag
        match ScalarField2D::<f32>::from_bytes(&bytes) {
            Err(FieldDecodeError::DtypeMismatch { .. }) => {}
            other => panic!("expected DtypeMismatch, got {other:?}"),
        }
    }

    // 11. truncated body
    #[test]
    fn from_bytes_rejects_bad_length() {
        let mut bytes = ScalarField2D::<f32>::new(2, 2).to_bytes();
        bytes.pop(); // remove one byte from body
        let res = ScalarField2D::<f32>::from_bytes(&bytes);
        assert!(
            matches!(
                res,
                Err(FieldDecodeError::LengthMismatch { .. })
                    | Err(FieldDecodeError::TooShort { .. })
            ),
            "expected LengthMismatch or TooShort, got {res:?}"
        );
    }
}
