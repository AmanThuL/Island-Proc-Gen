/// Governs how to map a field's raw value range to the `[0, 1]` palette input.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValueRange {
    /// Derive the mapping from the actual field min/max at render time.
    Auto,
    /// Fixed `[lo, hi]` mapping regardless of the field's actual range.
    Fixed(f32, f32),
    /// Auto-ranged on `log(value + 1)`. Used for flow accumulation where
    /// the raw distribution spans several decades.
    LogCompressed,
    /// Like `LogCompressed`, but the upper range is clamped to the given
    /// percentile (e.g. `0.99` for P99) of the field distribution before
    /// applying the log transform. Values above the clamp percentile are
    /// mapped to `t = 1.0`. Prevents extreme outliers from washing out the
    /// majority of the colour range into a single palette band.
    ///
    /// The percentile `q` must be in `(0.0, 1.0)`. Values outside this range
    /// are clamped by the caller before use.
    LogCompressedClampPercentile(f32),
}

impl ValueRange {
    /// Resolve this range to a concrete `(lo, hi)` pair.
    ///
    /// * `Auto` → returns `(field_min, field_max)` from the supplied values.
    /// * `Fixed(lo, hi)` → returns `(lo, hi)` unchanged.
    /// * `LogCompressed` → returns `(ln(1+field_min), ln(1+field_max))`.
    /// * `LogCompressedClampPercentile(_)` → same as `LogCompressed`.
    ///   The caller is responsible for passing an already-clamped `field_max`
    ///   (i.e. the raw percentile value rather than the absolute maximum).
    ///   See [`crate::overlay_export::bake_overlay_to_rgba8`] for how the
    ///   percentile is computed and passed in.
    pub fn resolve(self, field_min: f32, field_max: f32) -> (f32, f32) {
        match self {
            ValueRange::Auto => (field_min, field_max),
            ValueRange::Fixed(lo, hi) => (lo, hi),
            ValueRange::LogCompressed | ValueRange::LogCompressedClampPercentile(_) => (
                (1.0 + field_min.max(0.0)).ln(),
                (1.0 + field_max.max(0.0)).ln(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_compressed_resolve() {
        let (lo, hi) = ValueRange::LogCompressed.resolve(0.0, std::f32::consts::E - 1.0);
        assert!((lo - 0.0).abs() < 1e-5, "lo={lo}");
        assert!((hi - 1.0).abs() < 1e-4, "hi={hi}");
    }

    #[test]
    fn log_compressed_clamps_negative_min() {
        // Negative field_min treated as 0 before ln.
        let (lo, _hi) = ValueRange::LogCompressed.resolve(-5.0, 10.0);
        assert!((lo - 0.0).abs() < 1e-5, "lo={lo}");
    }
}
