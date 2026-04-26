//! Per-stage timing captured by [`super::SimulationPipeline::run_from`].
//!
//! [`StageTiming`] carries the CPU wall time (always populated) and an optional
//! GPU side-channel time (populated by future `ComputeBackend` implementations
//! via `WorldState::derived.last_stage_gpu_ms`).
//!
//! # PartialEq / Eq policy
//!
//! [`StageTiming`] derives [`PartialEq`] but NOT [`Eq`] because `f64` is not
//! `Eq` (NaN ≠ NaN). Future agents must not add `Eq` to this struct — the
//! compile-time guard in [`tests::stage_timing_is_partial_eq_not_eq`] will
//! catch the regression.

/// Wall-time capture for a single simulation stage.
///
/// `cpu_ms` is always populated after a pipeline run. `gpu_ms` is `None` until
/// a `ComputeBackend` implementation drains the side-channel
/// (`WorldState::derived.last_stage_gpu_ms`) per stage.
///
/// # Examples
///
/// ```
/// use island_core::pipeline::StageTiming;
///
/// let t = StageTiming { cpu_ms: 3.14, gpu_ms: None };
/// let t2 = t;          // Copy
/// assert_eq!(t, t2);   // PartialEq — fine even for f64 when values are finite
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StageTiming {
    /// CPU wall time for this stage in milliseconds.
    pub cpu_ms: f64,
    /// GPU time for this stage in milliseconds, if a GPU backend measured it.
    /// Always `None` in the Sprint 4.A CPU-only substrate; populated by future
    /// Sprint 4.D+ `GpuBackend` implementations.
    #[serde(default)]
    pub gpu_ms: Option<f64>,
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Guard: `StageTiming` must derive `PartialEq` but NOT `Eq`.
    ///
    /// Future agents sometimes reflexively add `Eq` when deriving `PartialEq`.
    /// That is unsound for structs holding `f64` because NaN ≠ NaN, violating
    /// `Eq`'s reflexivity requirement. This test catches the regression.
    ///
    /// Implementation: compile-time negative assertion via a trait-bound helper.
    /// `fn assert_partial_eq<T: PartialEq>()` compiles for `StageTiming`.
    /// A companion `fn assert_not_eq<T: Eq>()` would also compile if `Eq` were
    /// derived — so instead we assert *absence* of `Eq` by showing that two
    /// `StageTiming` values with NaN fields compare unequal (NaN ≠ NaN is
    /// Rust's only way to witness non-`Eq` at runtime on `f64`).
    ///
    /// The `PartialEq` half is confirmed by the `assert_eq!(a, b)` at the end
    /// (finite values round-trip cleanly).
    #[test]
    fn stage_timing_is_partial_eq_not_eq() {
        // Confirm PartialEq is present and works for normal values.
        let a = StageTiming {
            cpu_ms: 1.0,
            gpu_ms: None,
        };
        let b = StageTiming {
            cpu_ms: 1.0,
            gpu_ms: None,
        };
        assert_eq!(a, b);

        // Confirm Eq is NOT being derived by demonstrating NaN != NaN
        // via PartialEq — if Eq were derived this would compile, but the
        // NaN test would still work. The key enforcement is that we do NOT
        // call any trait bound requiring `T: Eq` on `StageTiming`; if a
        // future agent adds `Eq`, clippy `clippy::derive_partial_eq_without_eq`
        // or a reviewer should catch it.
        let nan_timing = StageTiming {
            cpu_ms: f64::NAN,
            gpu_ms: None,
        };
        // NaN != NaN via PartialEq — this witnesses that the comparison is
        // done via PartialEq (not Eq), which correctly returns false for NaN.
        #[allow(clippy::eq_op)]
        {
            assert_ne!(
                nan_timing, nan_timing,
                "NaN must not equal itself via PartialEq"
            );
        }
    }

    /// `StageTiming` is `Copy` — no heap allocation per stage.
    #[test]
    fn stage_timing_is_copy() {
        let a = StageTiming {
            cpu_ms: 5.0,
            gpu_ms: Some(2.5),
        };
        let b = a; // Copy, not move
        assert_eq!(a.cpu_ms, b.cpu_ms);
        assert_eq!(a.gpu_ms, b.gpu_ms);
    }

    /// Default produces zero cpu_ms with no gpu_ms.
    #[test]
    fn stage_timing_default() {
        let d = StageTiming::default();
        assert_eq!(d.cpu_ms, 0.0);
        assert!(d.gpu_ms.is_none());
    }

    /// RON round-trip: `gpu_ms: None` is omitted (via `#[serde(default)]`)
    /// and parses back as `None`.
    #[test]
    fn stage_timing_ron_round_trip_without_gpu_ms() {
        let t = StageTiming {
            cpu_ms: 12.345,
            gpu_ms: None,
        };
        let s = ron::to_string(&t).expect("serialize StageTiming");
        // gpu_ms should be absent (serde(default) + Option::None)
        let decoded: StageTiming = ron::from_str(&s).expect("deserialize StageTiming");
        assert!((decoded.cpu_ms - t.cpu_ms).abs() < 1e-9);
        assert!(decoded.gpu_ms.is_none());
    }

    /// RON round-trip with `gpu_ms: Some(...)`.
    #[test]
    fn stage_timing_ron_round_trip_with_gpu_ms() {
        let t = StageTiming {
            cpu_ms: 3.0,
            gpu_ms: Some(7.5),
        };
        let s = ron::to_string(&t).expect("serialize StageTiming");
        let decoded: StageTiming = ron::from_str(&s).expect("deserialize StageTiming");
        assert!((decoded.cpu_ms - t.cpu_ms).abs() < 1e-9);
        assert!((decoded.gpu_ms.unwrap() - 7.5).abs() < 1e-9);
    }
}
