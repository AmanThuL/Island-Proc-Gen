# CLAUDE.md

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
8. **String field keys only in `crates/render/src/overlay/resolve.rs`.**
   `sim`, `core::save` (error payloads aside), `core::validation` access
   state via struct field paths like `world.authoritative.height`.
   (Sprint 3.4 directorised `overlay.rs` → `overlay/{mod,catalog,range,resolve}.rs`;
   raw field-key strings remain confined to a single file — `resolve.rs`.
   `catalog.rs` constructs `OverlaySource` values only via
   `resolve::source_for(SourceKey::…)`.)

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
  `"deposition_flux"`, `"fog_water_input"`) stay in
  `crates/render/src/overlay/resolve.rs` per invariant #8.
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

### Closed-sprint contracts (Sprint 2.5 → 3.5)

Rationale, calibration evidence, and implementation narrative for the
closed sprints below are archived at
[`docs/history/claude_md_gotchas_archive.md`](docs/history/claude_md_gotchas_archive.md).
The bullets that follow are load-bearing *today*; go to the archive when
you need the "why" behind a constant or the full story of a decision.

#### Sprint 2.5 (hex contract)

- **`HexAttributes` is the stable 8-field contract** (Sprint 5 S2
  depends on it); don't extend. Debug quantities (slope variance,
  accessibility cost, river crossing) live on `HexDebugAttributes` at
  `derived.hex_debug`. Sprint 3/4 do NOT read `hex_debug`.
- **`Runtime::saved_visibility` snapshots the Continuous baseline**,
  not the previous view. Round-trips through any ViewMode land on
  original per-overlay visibility; HexOverlay's forced
  `hex_aggregated=on` does NOT persist on return.

#### Sprint 2.6

- **Dither toggle removed 2026-04-19.** `shaders/terrain.wgsl` keeps
  unconditional Sprint 1A dither (tile 8, amplitude 1/255, from
  `blue_noise_2d_64.png`). `overlay_render.rs` dither path is an
  independent control group. Do NOT reintroduce without a fresh A/B.
- **`render::DEFAULT_WORLD_XZ_EXTENT = 5.0`** (Fuji-like aspect ≈ 0.17,
  frozen 2026-04-19). Every render fn takes `extent: f32` explicitly;
  `Runtime::world_xz_extent` A/B's via the World panel. **Headless
  always passes `DEFAULT_WORLD_XZ_EXTENT`** — truth is sim-invariant,
  only beauty `byte_hash` drifts with extent.

#### Sprint 3 (sediment + climate + coast-type v2)

- **`authoritative.sediment` init lives at end of
  `CoastMaskStage::run`** (DD1). Land = `hs_init = 0.1`, sea = `0.0`,
  using `derived.coast_mask.is_land` (NOT `height > sea_level` per
  the Moore8/Von4 diagonal gotcha). Size-match reuses, size-change
  reallocates. Invalidation flows through **Coastal arm**, cascaded
  from `invalidate_from(StageId::Topography)`. `TopographyStage`
  writes a zero placeholder (`SaveMode::Minimal` requires `Some(..)`)
  that CoastMaskStage overwrites microseconds later.
- **SPACE-lite variant dispatch on `preset.erosion.spim_variant`**
  (DD2). Default `SpaceLite` = bedrock incision + sediment entrainment
  + `exp(-hs/H_STAR)` shielding; `Plain` preserves Sprint 2 bit-exact
  for Task 3.10 regen. Both share `stream_power_kernel(k,a,s,m,n)`.
  `K_bed = 5e-3` is the grid-safe ceiling; see Sprint 3.1 for why
  any bump trips the 5% sea-crossing invariant on 40²/64² fixtures.
- **`ErosionOuterLoop` inner step order is locked** (Task 3.3):
  `[stream_power_incision, sediment_update, deposition,
  hillslope_diffusion]`. `SedimentUpdateStage::run` does full DD3
  Qs-routing + deposition math in one Kahn topo-sort, writing
  `derived.deposition_flux`. `DepositionStage::run` is a diagnostic
  `Ok(())` hook (splitting would double O(N)). `deposition_flux`
  invalidates under **Topography arm** (NOT Coastal) — matches the
  sticky `erosion_baseline` pattern.
- **`PrecipitationStage` branches on
  `preset.climate.precipitation_variant`** (DD4). Default `V3Lfpm` =
  sequential upwind sweep with stateful `q`; `V2Raymarch` preserves
  Sprint 1B for regen. Sweep order cached in
  `derived.precipitation_sweep_order: Option<Vec<usize>>`, cleared
  under **Precipitation arm** so wind-dir slider drags rebuild.
  `run_v3_sweep` preheats 2 throwaway passes to kill near-axis cold
  starts. `P = max(0, Δq)` floors negative precipitation from
  marine-recharge injection.
- **`ClimateParams` nested on `IslandArchetypePreset`** (Task 3.4)
  with `#[serde(default)]` on every field — Sprint-2-vintage RON
  without `climate:` still deserializes. `core::preset::default_*`
  mirrors constants in `sim::climate::precipitation_v3` because
  invariant #1 forbids `core → sim`.
- **FogLikelihood v2 + SoilMoisture coupling** (DD5). Fog likelihood
  = `elev_band(p) · (0.5 + 0.5·uplift)`; `elev_band` = Gaussian bell
  at `inversion_z = 0.65·max_relief`, width `0.15·max_relief`.
  `derived.fog_water_input` on land, cleared under **SoilMoisture
  arm**. CloudForest bell weighting retightened so fog feeds
  CloudForest via raised soil_moisture, not direct bell multiplier
  (avoids double-counting). Numeric values subsequently doubled in
  Sprint 3.1.C — see that section.
- **`CoastType` is 5 classes** (DD6): `Cliff=0, Beach=1, Estuary=2,
  RockyHeadland=3, LavaDelta=4, Unknown=0xFF`. `CoastTypeStage`
  dispatches on `preset.erosion.coast_type_variant`: `V2FetchIntegral`
  default (16-direction raycast fetch + 5-class first-match
  classifier), `V1Cheap` for regen. `derived.volcanic_centers:
  Option<Vec<[f32;2]>>` written by `TopographyStage`, consumed by
  LavaDelta proximity. `COAST_TYPE_TABLE` is `[[f32;4]; 5]`;
  `sample_f32` uses `t * 5.0`.
- **16-direction fetch integral uses `-cos(θ − wind_angle)`** for
  windward weight, NOT DD6's literal `cos(...)` (reviewer I1 sign
  flip). `wind_angle` in this codebase = direction wind *travels*
  (matches `climate::common::wind_unit`); without the flip, weight
  peaks downwind and §10 Cliff>5% fails on real archetypes.
- **`BasinsStage` post-BFS CC pass is currently vacuous** because
  `ErosionOuterLoop` ends with fresh `PitFill`. Infrastructure +
  `basin_partition_post_erosion_well_formed` + `MIN_INTERNAL_LAKE_CELLS
  = 8` + Von4 CC pass activate once sediment-aware SPACE-lite leaves
  intentional deposition lakes unfilled. Do NOT remove the
  defensive code.

#### Sprint 3.1 (calibration tail)

- **`HS_INIT_LAND: f32 = 0.10`** is a named const in
  `crates/sim/src/geomorph/coastal.rs`, value-locked by
  `hs_init_land_constant_matches_sprint_3_1_lock`. Cross-file doc
  comments in `invalidation.rs` / `stream_power.rs` reference the
  named const so renames are single-point.
- **3:1 `K_sed = 3 · K_bed` ratio is asserted** in both
  `sediment.rs::space_lite_constants_match_dd2_lock` and
  `preset.rs::erosion_params_defaults_match_locked_constants`. Silent
  drift would break SPACE-lite physics while passing point-wise
  value checks.
- **Fog coupling tuned to doubled values**: `FOG_WATER_GAIN = 0.30`,
  `FOG_TO_SM_COUPLING = 0.60`, `CLOUD_FOREST_SIGMA_FOG = 0.15`,
  `CLOUD_FOREST_FOG_PEAK_WEIGHT = 0.40`. Max SM boost from fog is
  0.18 (was 0.06).
- **SPIM K cannot be raised above 5.0e-3** without tripping the 5%
  `erosion_no_excessive_sea_crossing` invariant on 40²/64² synthetic
  fixtures. Binding constraint for any future K sweep is the
  smallest grid, not 128². See archive for Sprint 3.1.A candidate
  evidence.
- **G4/G5/G7 stayed red** after 3.1. Residuals forwarded to Sprint
  3.5.D (biome + coast rework) and Sprint 4 (physical-unit
  calibration); LFPM numerical collapse was the one real fix.

#### Sprint 3.4 (module boundaries + test topology)

- **Three large files directorised**: `runtime.rs` → `runtime/{mod,
  events,frame,regen,view_mode,tabs}.rs`; `validation.rs` →
  `validation/{mod,hydro,climate,erosion,biome,hex}.rs`; `overlay.rs`
  → `overlay/{mod,catalog,range,resolve}.rs`. Public APIs
  bit-identical; see invariant #8 for the `overlay/resolve.rs`
  file-scope rule.
- **Test topology policy** (repo-wide):
  - Default: inline `#[cfg(test)] mod tests` adjacent to code under
    test.
  - **Pattern A** (`src/test_support.rs` + `#[cfg(test)] pub(crate)
    mod test_support;`) shares fixtures across inline tests in the
    **same crate**; invisible to integration tests. Only instance:
    `crates/core/src/test_support.rs::test_preset()`.
  - **Pattern B** (`tests/common/mod.rs` — **subdirectory form**,
    NOT `tests/common.rs`) shares fixtures across integration
    tests in the same crate. Not yet introduced.
  - Patterns A and B are separate worlds: if a helper is needed on
    both sides, **duplicate** rather than promote `test_support` to
    the public API.
- **`sim::` has duplicate `test_preset()` fns** across
  invalidation/hydro modules — intentionally NOT deduped
  (per-scenario tuning); revisit only if a later sprint produces
  identical duplication.

#### Sprint 3.5 (hex surface readability) — just closed

- **Flat-top hex convention is load-bearing** (DD1).
  `crates/hex/src/geometry.rs` is the single source of truth: width
  = `sqrt(3) * hex_size`, height = `2 * hex_size`, row spacing
  `1.5 * hex_size`, odd rows shifted east by `hex_size * sqrt(3) / 2`.
  All callers go through `axial_to_pixel / pixel_to_axial /
  offset_to_pixel / pixel_to_offset` — never re-derive inline.
- **`HexEdge` 6-edge numbering** (DD1, `#[repr(u8)]`): `E=0, NE=1,
  NW=2, W=3, SW=4, SE=5` (CCW from east). Encoding used in
  `HexDebugAttributes.river_crossing` since 3.5.B c1. **No raw
  `0..=5` edge indices outside `crates/hex/src/geometry.rs`** — use
  variants by name. `HexEdge::from_u8` validates at deserialisation
  boundaries.
- **DD2 axial-offset Voronoi kernel** in `build_hex_grid`
  (`crates/hex/src/lib.rs`) is the sole source of truth for
  hex-centre placement; `offset_to_pixel` mirrors it exactly.
  Value-locked by `offset_to_pixel_matches_build_hex_grid_centres`.
- **`HexCoastClass` lives in `core::world`, not `sim`** (DD4,
  invariant #1 forbids `core → sim`). Classifier impl in
  `crates/sim/src/hex_coast_class.rs`. 7 variants: `Inland=0,
  OpenOcean=1, Beach=2, RockyHeadland=3, Estuary=4, Cliff=5,
  LavaDelta=6`. Discriminants map 1:1 to
  `HexInstance.coast_class_bits` (render) and are hashed by
  `SummaryMetrics.hex_coast_class_hash`.
- **`coast_fetch_integral` → `hex_coast_class` data flow** (DD4).
  `CoastTypeStage::run` persists `derived.coast_fetch_integral:
  Option<ScalarField2D<f32>>`; `HexProjectionStage` reads it back
  without re-raycasting. Invariant
  `hex_coast_class_requires_fetch_integral` enforces the implication.
  Invalidation: `coast_fetch_integral` under **CoastType arm**,
  `hex_coast_class` under **HexProjection arm**.
- **DD6 SM floor + CloudForest envelope**: `COASTAL_MARGIN_MAX_DIST
  = 3` (Von4 land cells within 3 of sea get `soil_moisture ≥ 0.25`,
  never exceeds 1.0 clamp). CloudForest `f_t`: `T_PEAK = 18.0,
  T_SIGMA = 6.0` (widened from Sprint 3's 15.0/4.0; value-locked).
- **Dominant surface contract (DD5)**: base read = {biome,
  elevation, coast, river}; overlays are explicit augmentations,
  not base reads. New base reads need documented rationale in the
  sprint doc §2 DD5, not bolted on.
- **DD7 `HexInspectPanel` is strictly read-only.** Two-column egui
  grid, 11 attributes. No buttons, sliders, clear widget, or
  mutability. Reviewer gate catches interactive widgets at commit
  time.
- **Off-grid click = no-op** (DD7). `runtime/events.rs` on left-click
  release: ray → sea plane → `pixel_to_offset`; any `None` keeps
  `picked_hex` unchanged. A miss does NOT clear a previous pick
  (would silently blank the inspect panel).
- **Click-vs-drag threshold: `CLICK_DRAG_THRESHOLD_PHYS_PX = 3.0`**
  (Manhattan). Only clicks below threshold trigger hex pick; drags
  never touch `picked_hex`, so orbit-drag doesn't clobber the
  selection.
- **Dock layout compat is forward-only.** Pre-3.5 layouts fall
  through `dock.rs:122`'s `failed to parse` arm onto
  `default_layout()`. No migration promised.
- **`pixel_to_axial` cube-rounding is implementation-defined at
  3-way vertices**; tests assert "one of the three neighbours" to
  avoid coupling to the rounder's tie-break. Note: NE neighbour of
  `(0,0)` is axial `(1,-1)`, NOT `(0,-1)` which is NW.
- **DD8 schema: `CaptureRequest.schema_version = 3`.** Adds
  `CaptureShot.view_mode: Option<ViewMode>` via `#[serde(default)]`;
  v1/v2 files still parse. `SummaryMetrics` gains `hex_attrs_hash /
  hex_debug_river_crossing_hash / hex_coast_class_hash`, all rolled
  up by `TruthSummary.metrics_hash`.
- **Render-path parity** is locked by `render_stack_for(ViewMode) ->
  &'static [RenderLayer]` in `crates/app/src/runtime/view_mode.rs`.
  Both `frame.rs::tick` and `headless/executor.rs` beauty pass call
  it. `view_mode_dispatches_identically_in_frame_and_executor`
  (tier-1 gate) asserts both paths emit the same descriptor per
  ViewMode; tier-2 `IPG_RUN_VISUAL_PARITY=1` integration is opt-in.
- **Truth invariance across view_modes** (3.5.F): shots differing
  only in `view_mode` produce bit-identical `overlay_hashes` and
  `metrics_hash`. View_mode only affects beauty; a truth-affecting
  view_mode change would be an architectural violation.

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
