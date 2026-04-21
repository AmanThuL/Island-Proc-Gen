# AGENTS.md

Project-scoped context for Claude Code (and any other agent harness)
working in this repository. Read this before proposing or executing
changes.

A sibling [`CLAUDE.local.md`](CLAUDE.local.md) is **gitignored** and
carries per-user preferences. If both files exist, CLAUDE.md wins for
project-level rules; CLAUDE.local.md only shapes collaboration style.

---

## Role

Pair programmer on a single-developer Rust research project. Help build,
catch mistakes, push back when an idea drifts from the active sprint's
scope. Prefer small atomic commits. Ask before anything irreversible —
force push, dep downgrades, renaming a workspace crate, rewriting
`WorldState` layout, deleting generated artifacts. See `CLAUDE.local.md`
for the full consent-gate list.

---

## Key files

| File | Purpose |
|------|---------|
| [`PROGRESS.md`](PROGRESS.md) | Sprint-level dashboard |
| [`docs/design/island_generation_complete_roadmap.md`](docs/design/island_generation_complete_roadmap.md) | Authoritative roadmap + architectural rules |
| [`docs/design/sprints/sprint_N_*.md`](docs/design/sprints/) | Active sprint §6 acceptance + §7 invariants |
| [`docs/papers/README.md`](docs/papers/README.md) | Paper knowledge base layering |
| [`crates/data/golden/headless/README.md`](crates/data/golden/headless/README.md) | `--headless-validate` baselines |

**Read the active sprint doc before touching code for that sprint.** The
sprint's §6 acceptance checklist and §7 invariants are the done-definition
— not generic Rust best practices.

---

## AI-native validation workflow (Sprint 1C)

Validate the full pipeline + overlay + beauty stack headlessly:

```bash
# Run a CaptureRequest (runtime outputs → /captures/ gitignored;
# baselines under crates/data/golden/headless/ are tracked + idempotent
# via deterministic run_id).
cargo run -p app --release -- --headless <path/to/request.ron>

# Diff a runtime capture against a checked-in baseline (pure summary.ron
# comparison, no PNG reads).
cargo run -p app --release -- \
    --headless-validate /captures/headless/<run_id>/ \
    --against crates/data/golden/headless/sprint_1a_baseline/
```

**Exit-code contract (AD9, locked by `main_bin::tests::headless_exit_byte_maps_overall_status_to_ad9_code`):**

| Code | `OverallStatus` | Meaning |
|------|-----------------|---------|
| `0`  | `Passed` / `PassedWithBeautySkipped` | Truth path green |
| `2`  | `FailedTruthValidation` / `FailedMetricsValidation` | Pipeline regression |
| `3`  | `InternalError` | Tool-level error (IO, RON parse, shot-set mismatch) |

Always check exit code or `summary.ron.overall_status` — don't assume
success from command return.

Baselines under `crates/data/golden/headless/`:
- `sprint_1a_baseline/` — 3 presets × 3 seeds × Hero = 9 shots
- `sprint_1b_acceptance/` — default-wind 9-shot subset of the 1B 16-shot
  acceptance; wind-varying shots still live as manual PNGs in
  `docs/design/sprints/sprint_1b_visual_acceptance/`
- `sprint_2_erosion/` — 3 presets × pre/post erosion @ seed 42 = 6 shots;
  `pre_*` uses `preset_override.erosion.n_batch = 0`

---

## Architectural invariants (hard rules — tests + CI enforce)

1. **`core` stays headless.** `cargo tree -p core` must never list `wgpu`,
   `winit`, `egui*`, `png`, `image`, or `tempfile`. Enforced by
   `pipeline_runs_without_graphics` in `crates/core/src/pipeline.rs`.
2. **No `&Path` or `std::fs` in `core`.** Save codec is byte-level
   (`impl Write`/`impl Read`); `app::save_io` is the only ~5-line Path
   wrapper. Wasm target must work without touching `core`.
3. **`WorldState` is three-layer.** Top-level is exactly
   `{ seed, preset, resolution, authoritative, baked, derived }`. Never add
   `Option<ScalarField2D<...>>` to the top level. `derived` is
   `#[serde(skip)]`.
4. **`Resolution` is simulation-only.** `sim_width`/`sim_height` on
   `WorldState`. Render LOD and hex columns/rows are NOT canonical state.
5. **No `Vec<bool>`.** Masks are `MaskField2D = ScalarField2D<u8>` with
   `0 = false / 1 = true` — contiguous bytes for GPU/PNG/serde.
6. **Field abstraction is not a trait.** `ScalarField2D<T>` + `MaskField2D` +
   `VectorField2D` aliases only. (Private `pub(crate) trait FieldDtype`
   sealing `to_bytes`/`from_bytes` over `u8|u32|f32|[f32;2]` is OK.)
7. **Overlays are descriptors, not closures.** `OverlayRegistry` stores
   `Vec<OverlayDescriptor>`. No render-closure patterns (would lock
   Sprint 4's CPU-side PNG export path).
8. **String field keys only in `crates/render/src/overlay.rs`.** `sim`,
   `core::save` (error payloads aside), `core::validation` access state
   via struct field paths like `world.authoritative.height`.

---

## Crate dependency direction

```
app ──▶ render ──▶ gpu ──┐
  │       │              │
  │       └──▶ core ◀────┘
  │              ▲
  ├──▶ ui ───────┘
  │              ▲
  └──▶ sim ──────┘
         ▲
  hex ───┘
  data ──▶ core
```

`core` is a sink. `app` is the only crate allowed to wire everything together.

---

## Gotchas (learned, not in the roadmap)

### Crate / build

- **`core` shadows stdlib `::core`.** Downstream crates import as
  `island_core = { path = "../core", package = "core" }`. Apply the same
  pattern for new crates depending on `core`. Also why
  `crates/core/Cargo.toml` sets `[lib] doctest = false` (rustdoc's
  `--crate-name core` collides with `thiserror`'s `::core::fmt` paths).
- **`cargo clippy --workspace -- -D warnings`** (no `--all-targets`) is
  the CI gate. `--all-targets` surfaces pre-existing `approx_constant`
  lints in `crates/data/src/presets.rs` tests that can't replace the RON
  `1.5708` literals one-line.
- **Bash doesn't auto-source cargo env.** Prefix commands with
  `. "$HOME/.cargo/env" && <command>`.
- **Version pins (Sprint 0).** `egui`/`egui-wgpu`/`egui-winit` `0.34.1`;
  `wgpu` `29.0.1`; `winit` `0.30.13` (uses `ApplicationHandler`, not
  legacy `EventLoop::run`). `egui_dock = "0.19"` locks lockstep with
  egui 0.34 (`egui_dock/serde` activates `egui/serde` transitively,
  required for `crates/app/src/dock.rs` persistence). `naga` is a
  dev-dep on `render` only; pin matches wgpu-transitive `29.0.1`.
- **`docs/design` is a gitignored symlink** into the author's Obsidian
  vault. Sprint docs are NOT tracked in git — spec clarifications must
  be mirrored in commit messages + CLAUDE.md/PROGRESS.md.

### `WorldState` + serde

- **`ScalarField2D<T>` payloads are NOT serde-serialized inside
  `WorldState`.** `authoritative.height` / `authoritative.sediment` are
  `#[serde(skip)]`; save codec writes them via
  `ScalarField2D::to_bytes()` to keep `IPGF` byte format under our control.

### Hydro / D8

- **`FLOW_DIR_SINK = 0xFF`, not `0`.** `0` is east
  (`D8_OFFSETS[0] = (1,0)`). Import from
  `island_core::world::{D8_OFFSETS, FLOW_DIR_SINK}` — never hardcode.
  Constants live in `core::world` so `core::validation` references them
  without a reverse dep edge.
- **Post-pit-fill sinks are NOT `{ p : flow_dir[p] == FLOW_DIR_SINK }`.**
  `CoastMaskStage` uses Von4 for `is_coast`; `FlowRoutingStage` picks
  Moore8 downstream. Land cells with only a *diagonal* sea neighbour
  aren't coast but their D8 downstream is still sea. For BasinsStage +
  river-termination validation, "sink" includes "land cell whose D8
  downstream is sea or out-of-bounds". Encoded in `sim::hydro::basins.rs`.
- **`RiverExtractionStage` must gate candidates on `is_land`.** Same
  diagonal Moore8 edge case: sea cells can cross the river threshold
  without a land gate, firing `RiverInSea`. Caught by the full Sprint 1A
  pipeline test in `sim::validation_stage::tests`.
- **§D5 `coastal_falloff` formula in the sprint doc is written backwards.**
  Implementation uses `amplitude * smoothstep(0.9r, r, dist)` (0 inside
  island, amplitude at rim), not the doc's inverted form. See inline
  comment in `sim::geomorph::topography::build_coastal_falloff`.

### Palette / shaders / assets

- **8-colour palette is pixel-locked to
  `assets/visual/palette_reference.jpg`,** NOT the sprint-doc hex table.
  `canonical_constants_match_palette_reference` fires on drift.
  Re-sample reference (ΔE < 6 sRGB tolerance) before changing constants.
- **Invariant #8 applies to `shaders/*.wgsl`.** `terrain.wgsl` threads
  all 8 canonical colours through a `Palette` uniform buffer at
  `@group(0) @binding(1)`. Colour literals forbidden; enforced by
  `terrain_wgsl_has_no_literal_colors`.
- **Calinou blue-noise files are 8-bit RGBA** (LDR_LLL1_* replicates L
  across R=G=B). `noise::try_load_png` accepts Grayscale/RGB/RGBA and
  strips to R channel for luminance.
- **`render::camera` preset module ≠ `app::camera` orbit camera.**
  Runtime owns interaction; render owns capture geometry. Don't unify.
- **Shader validation:** `terrain_wgsl_parses_successfully` uses
  `naga::front::wgsl::parse_str` + `naga::valid::Validator`. Runtime
  shader loading uses wgpu's internal naga. Keep the pinned `naga`
  dev-dep matched to wgpu-transitive so parsers agree.

### Pipeline / StageId

- **`StageId` is the single source of truth for pipeline indices.**
  18-variant enum locked by `stage_id_indices_are_dense_and_canonical`.
  `run_from` callers pass `StageId::X as usize` — never a literal.
  `ValidationStage` is a tail hook, NOT a StageId variant.
- **`sim::default_pipeline()`** is the single source of truth for the
  19-stage canonical pipeline (18 variants + terminal `ValidationStage`).
  Consumed by `crates/data/tests/golden_seed_regression.rs` and
  `app::headless::executor`. Adding a stage requires updating both
  `default_pipeline` and `StageId` in lockstep.
- **`Runtime` uses `sim::default_pipeline()`; no local pipeline builders
  in `app`/`ui`.** Test-only variants go `#[cfg(test)]` inside
  `crates/sim/` next to the test. Hand-rolled pipelines silently drift
  when `StageId` changes.
- **Slider re-run protocol: sync `world.preset` BEFORE `run_from`.**
  Stages read from `world.preset`; runtime sliders mutate `self.preset`.
  Miss the sync → silent "identical before/after" screenshots with no
  panic or test failure.

### Biome / overlays

- **`BiomeWeightsStage` writes two fields simultaneously.**
  `baked.biome_weights` (rich partition-of-unity) +
  `derived.dominant_biome_per_cell` (`ScalarField2D<u32>` argmax
  sidecar, rendered via the same `ScalarDerived` resolver as
  `basin_id`). Both update every run. Touch the stage → update both or
  the overlay silently renders stale data.
- **`dominant_biome` overlay is NOT a reliable wind-propagation probe.**
  At v1 params, only ~3% of land cells flip argmax under a 180° wind
  swing (`volcanic_single`, 256², max `soil_moisture` delta 0.23). The
  8-biome categorical argmax is dominated by wind-invariant inputs
  (z_norm, slope, river_mask). Contract test
  `wind_dir_rerun_propagates_through_biome_chain` asserts
  `precipitation / fog_likelihood / soil_moisture / biome_weights /
  dominant_biome_per_cell` all mutate on `run_from(Precipitation)`.
  Sprint 1B §10 Pass 3 uses `soil_moisture` (wind-sensitive) for the
  60↔61 pair. New wind-dependent overlays: pick a field with a wind-
  sensitive raw range, not a categorical argmax.
- **`OverlayRegistry::sprint_3_defaults()` returns 20 descriptors.**
  Sprint 2.5's 16 + Sprint 3's four new (`sediment_thickness` → Turbo +
  `ScalarAuthoritative("sediment")`; `deposition_flux` → Viridis +
  `LogCompressedClampPercentile(0.99)`; `fog_water_input` → Blues +
  `Auto`; `lava_delta_mask` → `PaletteId::LavaDeltaMask` sampling only
  discriminant 4 opaque). Atomic rename per sprint (no alias shims per
  CLAUDE.md rename rule). Don't hardcode counts in UI — `OverlayPanel`
  iterates `registry.entries_mut()`. String keys (`"sediment"`,
  `"deposition_flux"`, `"fog_water_input"`) stay in `overlay.rs` per
  invariant #8.
- **`coast_type` overlay uses `ValueRange::Fixed(0.0, 5.0)`** (post-3.6
  LavaDelta addition; was `(0.0, 4.0)`). Range must match
  `discriminant count + 1` so Unknown=0xFF clamps transparent.
  Regression guards in `crates/render/src/palette.rs` tests.
- **`flow_accumulation` uses
  `ValueRange::LogCompressedClampPercentile(0.99)`,** not plain
  `LogCompressed`. Long-tail (P90/max ≈ 0.02 on `volcanic_twin`)
  compresses into bottom ~20% of palette under `ln(1+max)` ceiling.
  Percentile variant uses p-quantile of `ln(1+value)` at bake time.
  New long-tail overlays should use it.
- **`OverlayDescriptor.alpha`** replaces the hardcoded `0.6`.
  Per-descriptor default 0.6; `OverlayRenderer::draw` writes uniforms
  per frame (`registry.len() × 4 bytes` — negligible at 25+ overlays).
  Don't reintroduce a single global alpha constant.

### Headless / AD8 / AD9

- **`GpuContext::surface: Option<wgpu::Surface<'static>>`** serves both
  windowed Runtime and `--headless`. Interactive paths call
  `gpu.surface_expect()` (panics descriptively);
  `GpuContext::new_headless((w,h))` sets `surface = None` and
  `surface_format = HEADLESS_COLOR_FORMAT = Rgba8Unorm`. Renderers key
  off `gpu.surface_format` / `gpu.depth_format` — same code both paths.
- **AD8 GPU bootstrap is top-level, NOT per-shot.**
  `app::headless::executor::run_request` calls
  `GpuContext::new_headless(...)` once. Failure → all
  `BeautySpec` shots `BeautyStatus::Skipped`, truth runs to completion,
  `OverallStatus::PassedWithBeautySkipped` keeps exit 0. Per-shot retry
  introduces non-determinism.
- **`CaptureRequest` vs `SaveMode::DebugCapture` are different
  abstractions.** `core::save` stays byte-level (wasm-safe);
  `app::headless` owns filesystem/PNG/RON. Keep separate to preserve
  invariants #1/#2.
- **Truth deterministic (AD7); beauty artifact-only (AD2+AD7).**
  `summary.ron` bit-exact modulo whitelist (`timestamp_utc`,
  `pipeline_ms`, `bake_ms`, `gpu_render_ms`, `warnings`). Beauty
  `byte_hash` is NEVER used for pass/fail — cross-GPU fp drift → only
  warnings on divergence/skip. `--headless-validate` compare tool reads
  `summary.ron` only.
- **`metrics_hash: Option<String>`.** `None` = shot opted out of
  `include_metrics`; two `None`s compare equal; mixed is a mismatch.
  `include_metrics: false` means no `metrics.ron` file on disk.
- **`AD9 OverallStatus` 5-variant set is locked.** Exit-code map frozen
  at 0/2/3. Sprint 4+ may add variants only additively per AD9 "扩展规则".
  `InternalErrorKind::Other` carries `#[serde(other)]` for forward-compat.
- **`RunSummary.schema_version` mirrors input
  `CaptureRequest.schema_version`,** not tool version. v1 requests
  under a v2 binary produce v1-stamped summaries — baselines keep
  validating under newer binaries.
- **Never commit overlay/beauty PNGs under
  `crates/data/golden/headless/`.** `.gitignore` excludes `**/*.png`,
  but re-running `--headless <baseline>/request.ron` regenerates them.
  Delete before `git add -A` (one-liner in baseline README).

### Erosion (Sprint 2+)

- **`authoritative.height` is mutable from Sprint 2 onward.**
  `StreamPowerIncisionStage` + `HillslopeDiffusionStage` rewrite in-place
  every inner iteration of `ErosionOuterLoop`. When mutating from any
  code path, follow default invalidation:
  `invalidate_from(world, StageId::Coastal)` — height mutation may
  cross sea_level and stale `coast_mask`.
- **`derived.erosion_baseline` is sticky across slider reruns.**
  `ErosionOuterLoop::run` snapshots `{max_height_pre,
  land_cell_count_pre}` on first invocation (gated by `is_none()`).
  Reruns via `run_from(ErosionOuterLoop)` do NOT re-snapshot.
  `invalidate_from(Topography)` is the only legitimate reset; the
  `ErosionOuterLoop` arm of `clear_stage_outputs` is intentionally
  noop so `invalidate_from(Coastal)` mid-batch can't clobber baseline.
- **SPIM `K` is grid-size-dependent. `K=1.5e-3` is locked default.**
  `K=2e-3` passes 128² but fails synthetic 64² (`volcanic_single`
  sea-crossing tips to 5.09%, above the 5%
  `erosion_no_excessive_sea_crossing` invariant). Any K bump must
  verify on 64²/128²/256². Spec DD1's "~18% max_z drop" is physically
  incompatible with the 5% invariant under uniform SPIM — requires
  `K·g(hs)` sediment-aware (a Sprint 3 problem).
- **CoastType v1 thresholds tuned from spec.** Spec's v1
  `0.30/0.18/0.05/0.30` puts 100% of cells in Beach because max coastal
  slope rarely exceeds 0.07 at safe K. Locked constants in
  `sim::geomorph::coast_type.rs` = `0.07/0.04/0.02/0.05` — populates
  Beach/Rocky/Estuary, still 0 Cliffs on stock presets. Cliff bins
  light up under Sprint 3's v2 classifier (fetch-integral wave exposure).

### Sprint 2.5 (hex)

- **`HexAttributes` is locked at 8 fields** (elevation, slope, rainfall,
  temperature, moisture, biome_weights, dominant_biome, has_river).
  Sprint 2.5 debug quantities (slope variance, accessibility cost,
  river crossing) live on sibling `HexDebugAttributes` at
  `derived.hex_debug`. Sprint 3/4 do NOT read `hex_debug`; Sprint 5 S2
  may redesign it freely. Don't extend `HexAttributes` — it's the
  stable contract Sprint 5 S2 depends on.
- **`HexRiverCrossing` uses 4 box edges** (0=top/1=right/2=bottom/3=left),
  not 6 hex edges. `crates/hex` tessellation is axis-aligned rectangles
  per Sprint 1B. Sprint 5 S1 real-hex rework expands to 6; keeping
  `HexRiverCrossing` inside `HexDebugAttributes` isolates that.
- **ViewMode snapshot is the Continuous baseline, not the previous
  view.** `Runtime::saved_visibility` populates on first departure from
  `Continuous`, cleared on return. Round-trips (incl.
  `Continuous → HexOverlay → HexOnly → Continuous`) land on original
  per-overlay visibility. HexOverlay's forced `hex_aggregated=on` does
  NOT persist after return to Continuous.

### Sprint 2.6

- **Dither toggle DROPPED (2026-04-19 in-window A/B).** `DITHER_ON`
  uniform + Camera-panel checkbox + `TerrainRenderer::update_dither`
  all removed in `d39e2f3`. `shaders/terrain.wgsl` keeps unconditional
  Sprint 1A dither (tile 8, amplitude 1/255, from
  `blue_noise_2d_64.png`). 2.6.E ComboBox closed as n/a.
  `assets/noise/blue_noise_2d_{128,256}.png` deleted.
  `crates/render/src/overlay_render.rs` dither path untouched (independent
  control group). Do NOT reintroduce a dither toggle without a fresh
  A/B session.
- **`render::DEFAULT_WORLD_XZ_EXTENT = 5.0`** (Fuji-like aspect ≈ 0.17,
  frozen 2026-04-19). Every render function
  (`build_terrain_mesh`, `build_sea_quad`,
  `render::camera::view_projection/eye_position`,
  `app::Camera::apply_preset`, `TerrainRenderer::new`) takes
  `extent: f32` explicitly so `Runtime::world_xz_extent` can A/B via
  World panel. **Headless always passes `DEFAULT_WORLD_XZ_EXTENT`** — 3
  baselines captured at that value must stay truth-identical (only
  beauty `byte_hash` drifts with extent; truth is sim-invariant). When
  a later sprint freezes the final value, update the const and regen
  the 3 beauty PNGs in the same commit.

### Sprint 3

- **`authoritative.sediment` init lives at end of `CoastMaskStage::run`**
  (DD1). Land = `hs_init = 0.1`, sea = `0.0`, using the just-written
  `derived.coast_mask.is_land` oracle (NOT `height > sea_level` per the
  Moore8/Von4 diagonal gotcha). Allocate-if-None-or-size-mismatch, else
  reuse; `sediment_reused_across_reruns_when_resolution_unchanged`
  locks pointer+capacity stability. `TopographyStage` writes a zero
  placeholder (`SaveMode::Minimal` demands `Some(..)` at the boundary);
  CoastMaskStage overwrites microseconds later. Invalidation flows
  through **Coastal arm** of `clear_stage_outputs`, cascaded from
  `invalidate_from(StageId::Topography)` via StageId ordering.
- **SPACE-lite variant dispatch reads
  `preset.erosion.spim_variant`** (DD2). Default `SpaceLite` runs the
  dual equation (bedrock incision + sediment entrainment +
  `exp(-hs/H_STAR)` shielding); `Plain` falls back to Sprint 2's
  single-equation SPIM, bit-exact with pre-3.2 behaviour for Task 3.10
  regen (`preset_override.erosion.spim_variant = Some(Plain)`). Both
  share `stream_power_kernel(k, a, s, m, n)` via `#[inline]`. Locked by
  `plain_branch_is_deterministic_across_repeated_runs`. `K_bed = 5e-3`
  tuned so effective K at coast ≈ 9e-5 (well below 5% sea-crossing
  invariant): `exp(-0.1/0.05) ≈ 0.14` shielding at `hs = 0.1`. K
  calibration sweeps belong to Task 3.10.
- **`ErosionOuterLoop` inner step is a 4-stage sequence** (Task 3.3):
  `[stream_power_incision, sediment_update, deposition,
  hillslope_diffusion]`. Locked by `erosion_inner_step_canonical_order`.
  `SedimentUpdateStage::run` does full DD3 Qs-routing + deposition
  math in one Kahn topo-sort (D compute, `hs += D·dt`, Qs_out
  propagate), writing `derived.deposition_flux[p] = D[p]`.
  `DepositionStage::run` is a diagnostic `Ok(())` finalization hook
  (splitting would force double O(N) traversal).
  `derived.deposition_flux` invalidates under **Topography arm** (NOT
  Coastal) — matches sticky `erosion_baseline` pattern. Counter-test:
  `invalidate_from_coastal_preserves_deposition_flux`.
- **`PrecipitationStage` branches on
  `preset.climate.precipitation_variant`** (DD4). Default `V3Lfpm` =
  sequential upwind sweep with stateful water-vapour `q`, orographic
  condensation + fallout, coast-proximity marine recharge.
  `V2Raymarch` preserves Sprint 1B per-cell raymarch for Task 3.10.
  Sweep order computed on first invoke via stable sort on
  `-wind · p_position`, cached in
  `derived.precipitation_sweep_order: Option<Vec<usize>>` — cleared
  under **Precipitation arm** so wind-dir slider drags rebuild.
  `run_v3_sweep` preheats 2 throwaway passes into `q_scratch` to kill
  near-axis-aligned cold-start transients. `P = max(0, Δq)` floors
  negative precipitation from marine_recharge injection (per-cell sign
  is not a v1 invariant; aggregate budget is
  `precipitation_mass_balance`'s job).
- **`ClimateParams` is a new nested struct on `IslandArchetypePreset`**
  (Task 3.4): `precipitation_variant / q_0 / tau_c / tau_f`.
  `prevailing_wind_dir` + `marine_moisture_strength` stay top-level
  (don't break existing RON). All `ClimateParams` fields are
  `#[serde(default = ..)]` → Sprint-2-vintage RON without `climate:`
  deserializes into V3Lfpm defaults.
  `core::preset::default_q_0/tau_c/tau_f` mirror `Q_0_DEFAULT /
  TAU_C_DEFAULT / TAU_F_DEFAULT` in `sim::climate::precipitation_v3`
  because invariant #1 forbids `core → sim`. Same structural issue as
  SPACE-lite defaults; out of scope to consolidate in Sprint 3.
- **FogLikelihood v2 + SoilMoisture fog coupling** (DD5). Fog likelihood
  = `elev_band(p) · (0.5 + 0.5·uplift)`; `elev_band` = Gaussian bell at
  `inversion_z = 0.65·max_relief`, width `0.15·max_relief` (trade-wind
  inversion-layer proxy). `SoilMoistureStage::run` writes
  `derived.fog_water_input[p] = FOG_WATER_GAIN · fog_likelihood[p]`
  (GAIN=0.15) on land; adds `fog_water_input · FOG_TO_SM_COUPLING`
  (COUPLING=0.40) to soil_moisture, clamped 1.0. `fog_water_input`
  cleared under **SoilMoisture arm**. CloudForest bell retightened
  simultaneously: `sigma_fog = 0.08` (was 0.12), direct fog weight 0.3
  (was 1.0) — fog now feeds CloudForest primarily via raised
  soil_moisture rather than as a direct bell multiplier (prev
  double-counting).
- **`CoastType` is now 5 classes** (DD6): `Cliff=0`, `Beach=1`,
  `Estuary=2`, `RockyHeadland=3`, **`LavaDelta=4`**, `Unknown=0xFF`.
  `CoastTypeStage::run` dispatches on
  `preset.erosion.coast_type_variant`: `V2FetchIntegral` (default) =
  16-direction raycast fetch with windward-peaks-at-1.0 weighting +
  5-class first-match classifier (`is_mouth → LavaDelta(Young ∧ …) →
  Cliff → RockyHeadland → Beach → RockyHeadland fallthrough`);
  `V1Cheap` preserves Sprint 2 bit-exact for Task 3.10 pre_* regen.
  `derived.volcanic_centers: Option<Vec<[f32; 2]>>` (NEW derived
  field, follows Sprint-2.5 `hex_debug` `Vec<[f32;2]>` precedent, not
  a ScalarField2D) written by `TopographyStage::run` in normalized
  `[0,1]²`, consumed by LavaDelta proximity. `COAST_TYPE_TABLE` palette
  is now `[[f32; 4]; 5]`; `sample_f32` uses `t * 5.0`.
  `core::validation::coast_type_well_formed` widened to 0..=4 in 3.6;
  additional `coast_type_v2_well_formed` (3.9) enforces
  LavaDelta-only-on-Young.
- **16-direction fetch integral uses `-cos(θ − wind_angle)`** for the
  windward weight, NOT DD6's literal `cos(...)` (reviewer I1).
  `wind_angle` in this codebase = direction wind *travels* (matches
  `climate::common::wind_unit`); `cos(θ - wind_angle)` peaks downwind,
  opposite DD6's own 「迎风 1.0, 背风 0.5」 Chinese comment. Sign flip
  restores windward-maximum. Future edits must use windward-max
  semantics or lose §10 Cliff>5% acceptance on real archetypes.
- **`BasinsStage` post-BFS CC pass is currently vacuous on real
  terrain** because `ErosionOuterLoop` ends with fresh `PitFill`
  eliminating interior depressions. Infrastructure +
  `basin_partition_post_erosion_well_formed` invariant +
  `MIN_INTERNAL_LAKE_CELLS = 8` + Von4 CC pass all activate once Sprint
  3 sediment-aware SPACE-lite leaves intentional deposition lakes
  unfilled. Do NOT remove the defensive code.

---

## Commit style

- **Conventional commits:** `feat(scope): ...`, `fix(scope): ...`,
  `refactor: ...`, `docs: ...`, `ci: ...`, `chore: ...`. Scope is crate
  name(s) or omitted for workspace-wide refactors.
- **One task per commit.** Bundle sprint work across commits, not one
  giant commit — keeps bisection/rollback tractable.
- **No `Co-Authored-By: Claude ...` footer** — attribution disabled
  globally at user level.
- Don't amend commits already on `dev`/`main`. Create a new commit.
- Don't bypass `--no-verify` / `--no-gpg-sign` without asking.

---

## Rules for this session

1. Active sprint doc's §6 acceptance checklist is the done-definition.
   Out-of-scope features need explicit user approval.
2. Never add a dep to `core` that breaks `cargo tree -p core` cleanliness
   (no `wgpu`, `winit`, `egui*`, `png`, `image`, `tempfile`, `naga`).
3. If a subagent's plan violates any architectural invariant, stop and
   flag it — don't let it slide.
4. Subagent workflow cadence + model selection + consent gates live in
   [`CLAUDE.local.md`](CLAUDE.local.md) (gitignored, per-user).

---

## Session start protocol

1. Read `PROGRESS.md` for current sprint state.
2. Read the active sprint file in `docs/design/sprints/`.
3. Run `git status` and `git log --oneline -10`.
4. If a Rust change is planned, verify `cargo check --workspace` is green:
   ```bash
   . "$HOME/.cargo/env" && cargo check --workspace
   ```
5. Surface any architectural invariant the planned work would touch;
   confirm the plan preserves it.

---

## Notes

- App is macOS-first (Metal), architecture stays platform-agnostic
  (Sprint 5 targets wasm). No `#[cfg(target_os = "macos")]` in `core`.
- `docs/papers/` knowledge base is intentionally shallow for most papers
  (frontmatter + abstract + one-sentence purpose). Chen 2014 and Temme
  2017 are the only ones with substantive 落地点 sections as of Sprint 0.
