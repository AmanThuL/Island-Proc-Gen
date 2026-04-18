# Sprint 1B Acceptance Baseline

This directory contains the `--headless` baseline for Sprint 1B visual acceptance testing.
The baseline is validated via `cargo run -p app --release -- --headless-validate` and serves
as the truth path for bit-exact regression detection across the pipeline.

## Shot count arithmetic

Original Sprint 1B visual acceptance = 16 shots in `docs/design/sprints/sprint_1b_visual_acceptance/`.
This baseline covers:

- **9 default-wind shots** from the original 1B baseline migration
  (seed 42, volcanic_single, 128×128):
  - `s00_baseline_default` (final_elevation, metrics)
  - `s40`–`s46` (temperature, precipitation, soil_moisture, curvature, dominant_biome,
    hex_aggregated, precipitation + rivers; no metrics)
  - `s70_all_overlays_on` (12 overlays, metrics)

- **6 wind-varying shots** added by Sprint 2.5.E using schema v2's `preset_override.prevailing_wind_dir`
  (seed 42, volcanic_single, 128×128):
  - `wind_0_volcanic_single_seed42` (0 rad: precipitation, soil_moisture, metrics)
  - `wind_pi_2_volcanic_single_seed42` (π/2 rad: precipitation, soil_moisture, metrics)
  - `wind_pi_volcanic_single_seed42` (π rad: precipitation, metrics)
  - `wind_3pi_2_volcanic_single_seed42` (3π/2 rad: precipitation, metrics)
  - `wind_0_soil_volcanic_single_seed42` (0 rad: soil_moisture, metrics)
  - `wind_pi_soil_volcanic_single_seed42` (π rad: soil_moisture, metrics)

**Total: 15 shots in this baseline.**

## Permanently excluded shot

Shot `01_baseline_camera_overlays_panels` is **permanently excluded** from this baseline.
It captures UI panel state and interactive camera position, which the `--headless` capture path
cannot serialise (no UI runtime exists to replay). The headless architecture (AD2, AD7) requires
pure pipeline-determinism with no interactive state.

This shot remains as a **manual visual reference** in `docs/design/sprints/sprint_1b_visual_acceptance/`
and is not re-runnable via `--headless`. Any future AI agent auditing the shot count should
refer to this note rather than re-opening the "why not 16?" question.
