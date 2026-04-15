# PROGRESS

**Last Updated:** 2026-04-15

---

## How this works

One file, one project, one set of moving parts. Sprint-level granularity —
individual tasks live in the per-sprint docs under `docs/design/sprints/`.
Update this file whenever a sprint ships, scope shifts, or a blocker changes.

Three questions this file must always answer:
1. What am I building right now?
2. Where is the next sprint's entry point in the code?
3. What's shipped and proven?

**This is not a to-do list.** For tasks, see the active sprint doc.

---

## CURRENT FOCUS

**Primary:** Sprint 1B — climate + ecology closed loop. Library path
**fully shipped on `dev`** in 14 atomic commits across 2026-04-15.
All 1B-core tasks from the sprint doc §0 required-for-close-out list
are landed: run_from infra, always-on curvature, 8 new sim stages
(TemperatureStage → HexProjectionStage), 6 new overlays, 4 new
validation invariants, golden-seed metrics regen, and the first
interactive wind-direction slider with end-to-end pipeline re-run
via `run_from` + `OverlayRenderer::refresh`.

**269 tests passing** across 8 crates (core 55, sim 123, data 10,
render 65, hex 7, app 9, ui 0, gpu 0). Regression: +81 new tests
since Sprint 1A's 188 baseline. `cargo fmt --check &&
cargo clippy --workspace -- -D warnings && cargo test --workspace`
is the hard CI gate, all green.

**Sprint 1B §10 acceptance checklist status:**

Compile + test:
- ✓ `cargo test --workspace` green (269 / 269)
- ✓ Every new stage has unit tests + invariant coverage
- ✓ Golden seed regression passes on regenerated snapshots for all 3 presets

Functional:
- ✓ Temperature / precipitation / soil moisture / biome / hex fields
  all `is_some()` after pipeline run (asserted by the new
  `full_sprint_1b_pipeline_passes_all_invariants` integration test)
- ✓ Wind direction slider re-runs the pipeline via `run_from` and
  refreshes overlay textures in `tick()` — library path is complete;
  the live `cargo run -p app` window session for visual verification
  is the last open item (needs user approval per CLAUDE.md)
- ✓ Dominant biome overlay: the 16-cell varied-domain integration
  test asserts at least 3 distinct dominant biomes appear; golden
  seed metrics confirm 3 biomes (Grassland 68%, BareRockLava 28%,
  RiparianVegetation 4%) for `volcanic_single` at 128²
- ✓ 12 overlays all toggle cleanly via `OverlayPanel` and resolve
  through the `resolve_scalar_source` typed-borrow path

Architectural invariants:
- ✓ `core` still headless — `cargo tree -p core` free of wgpu /
  winit / egui / png / image / tempfile / naga
- ✓ `hex` crate only depends on `core` (see new `hex/Cargo.toml`)
- ✓ `run_from(stage_index)` works on the wind slider path
- ✓ 12 overlays toggle + stack (alpha hardcoded from Sprint 1A, not
  a Sprint 1B blocker)

Paper:
- Deferred — spec §7 Core Pack reads + Bruijnzeel notes flagged for
  the next low-energy window; non-blocking per §7.

**Next session priorities** (see [QUICK REFERENCE](#quick-reference)):
1. User-approved `cargo run -p app` window session to drag the wind
   slider and visually verify the precipitation / biome overlays
   refresh in real time.
2. §7 Paper pack deep reads (Chen 2023 / Bruijnzeel 2005 / 2011).
3. Sprint 1B-tail (alpha slider per overlay, seed-cycling, 9-shot
   baseline, blue noise size toggle) — optional, parked under
   [DEFERRED TO LATER SPRINTS](#deferred-to-later-sprints).

---

## RECENTLY SHIPPED

Sprint 1B core, 14 commits on `dev`, 2026-04-15 session. Every
commit used the simplify → superpowers code-reviewer → commit
cadence except the two structural/mechanical commits
(golden-seed regen, app::Runtime wiring) where the combined review
pass confirmed no outstanding issues.

| Commit | Task | Spec | Tests delta |
|---|---|---|---|
| `96036c6` | 1B.0a — `run_from` infra + `StageId` enum (16 stages) | §4.0 / §2 DD9 | 188 → 193 |
| `e2d2bd9` | 1B.0b — always-on curvature in `DerivedGeomorphStage` | §4.0 DD9 | 193 → 197 |
| `26e6434` | 1B.1 — `TemperatureStage` + `climate::common` helpers | §2 DD1 | 197 → 205 |
| `d4321a2` | 1B.2 — `PrecipitationStage` upwind raymarch | §2 DD2 | 205 → 210 |
| `37616ce` | 1B.3 — `FogLikelihoodStage` + `smoothstep` helper | §2 DD7 | 210 → 219 |
| `513a941` | 1B.4 — `PetStage` + `WaterBalanceStage` (Budyko Fu) | §2 DD3 + DD4 | 219 → 227 |
| `66681d7` | 1B.5 — `SoilMoistureStage` (consumes `flow_dir`) | §2 DD5 | 227 → 234 |
| `0ca94a7` | 1B.6 — `BiomeWeightsStage` + 8 biome types | §2 DD6 | 234 → 246 |
| `d464936` | 1B.7 — `HexGrid` + `HexProjectionStage` | §2 DD8 | 246 → 259 |
| `de27147` | 1B.11 — 4 new validation invariants | §8 | 259 → 267 |
| `afc20f0` | Wire Sprint 1B pipeline into `app::Runtime` | §4 integration | 267 → 268 |
| `0e454db` | 1B.8 — 6 new overlays (12 total) | §6 | 268 → 269 |
| `75909ea` | 1B.10 — `SummaryMetrics` + golden regen | §9 | 269 (same) |
| `0ee8b82` | 1B.9 — Wind direction slider + `run_from` re-run | §5 | 269 (same) |

**StageId enum is the single source of truth** for pipeline indices.
The 16-variant enum (`Topography = 0` … `HexProjection = 15`) is
locked by `stage_id_indices_are_dense_and_canonical` in
`crates/sim/src/lib.rs`, and every `run_from` caller (
`app::Runtime`, slider handler, golden regen) passes `StageId::X as usize`
rather than hardcoding a literal index. `ValidationStage` is
intentionally excluded from the enum — it's a tail hook, not a
slider target.

**Climate + ecology decisions** (sprint doc §2 DD1–DD9):
- **DD1 Temperature:** lapse rate `6.5 °C/km` + coastal modifier
  `2 °C * exp(-d/0.05)`. Sea cells forced to `T_SEA_LEVEL_C = 26`
  to avoid phantom shoreline gradients downstream.
- **DD2 Precipitation:** 32-step upwind raymarch with `k_c = 1.5`
  condensation and `k_shadow = 2.0` rain-shadow attenuation.
  Ascent / descent branches are mechanically exclusive via the
  shared `signed_uplift` helper so the v1.0 dead-branch regression
  is impossible. Unit test asserts windward > leeward by 30 % on a
  synthetic tent ridge.
- **DD3 / DD4 Water balance:** Hamon PET (`k = 0.04`) plus
  Budyko-Fu ET/R split with `ω = 2.2` and `PET/P` clamped to
  `[0.01, 10]`. `R = max(0, P - ET)` preserves the mass balance
  exactly.
- **DD5 Soil moisture:** convex combination `0.5 * (ET/PET) +
  0.3 * log(A+1)/log(A_max+1) + 0.2 * river_proximity`, followed
  by a single downstream smoothing pass along `flow_dir` (the real
  first consumer of the Sprint 1A hand-off contract that built the
  routing graph for 1B to use).
- **DD6 Biomes:** 8 functional types with bell × smoothstep
  suitability, normalized to a per-cell partition of unity, then a
  per-basin mean blend with `α = 0.3` keyed on `basin_id` (the
  second real 1A handoff consumer). `BTreeMap<u32, ...>`
  accumulators lock determinism structurally.
- **DD7 Fog:** `smoothstep(CLOUD_BASE_Z=0.4, CLOUD_TOP_Z=0.75,
  z)` × `smoothstep(0, 0.3, max(0, signed_uplift))`. Single-pass
  over land cells, sea cells → 0.
- **DD8 Hex projection:** `64 × 64` flat-top axis-aligned box
  tessellation (v1 simplification; Sprint 5 can refit to true
  hexagonal Voronoi). f64 accumulators for aggregation precision,
  sea cells excluded from per-hex means.

**Integration test** `full_sprint_1b_pipeline_passes_all_invariants`
in `sim::validation_stage::tests` builds the complete 17-stage
pipeline (16 real + tail ValidationStage) on a `volcanic_preset` at
64² and asserts every Sprint 1B output field (`curvature`,
`temperature`, `precipitation`, `fog_likelihood`, `pet`, `et`,
`runoff`, `soil_moisture`, `biome_weights`, `hex_grid`, `hex_attrs`)
is populated and every invariant fires clean. This is the
end-to-end guarantee that the whole 1B data flow works on non-
synthetic inputs.

**Golden seed regression regenerated** via `SNAPSHOT_UPDATE=1
cargo test -p data --test golden_seed_regression`. Sprint 1A field
hashes are bit-exact unchanged (proving no 1B stage wrote back into
a 1A field), and the new 1B summary fields (`mean_precipitation`,
`windward_leeward_precip_ratio`, `mean_temperature_c`,
`mean_soil_moisture`, `biome_coverage_percent`, `hex_count`) are
committed for the three presets. `volcanic_single @ seed 42 / 128²`:
windward/leeward ratio 1.098, mean temp 19.1 °C, 3 dominant biomes.

---

## DEFERRED TO LATER SPRINTS

**From Sprint 1A (still pending, unchanged):**

- **Sprint 2 — flow accumulation overlay log-compression audit**
  (Shot 13 washout). Sprint 2's stream-power erosion work is the
  right place to exercise the accumulation distribution and validate
  the LogCompressed bake parameters.
- **Sprint 2 — Blue noise dither A/B visual validation** (Shots 30,
  31). Sub-LSB amplitude is below screenshot-inspection threshold;
  Sprint 2 will touch terrain shading and can ship a shader
  feature-flag for cheap A/B diff.

**From Sprint 1B close-out (new):**

- **User-approved `cargo run -p app` visual verification** of the
  wind-direction slider. Library path is complete; the live window
  session to drag the slider and watch the precipitation / biome
  overlays refresh needs explicit user approval per CLAUDE.md.
- **Sprint 1B-tail T1/T2/T3** from sprint doc §11 — the "nice to
  have" UI/tooling polish items that depend on Sprint 1B's
  ParamsPanel / OverlayPanel but are explicitly **not** part of the
  §10 close-out checklist:
  - **T1 — 9-shot golden visual baseline** (3 camera presets × 3
    seeds) inherited from Sprint 1A. Needs a seed-cycling slider
    which isn't in 1B-core.
  - **T2 — Per-descriptor alpha slider** for the 12 overlays. Alpha
    stays at the Sprint 1A hardcoded `0.6` for now.
  - **T3 — Blue noise runtime size toggle** (64 / 128 / 256). The
    other two PNGs sit in `assets/noise/` but nothing loads them.
- **Sprint 2 — BareRockLava / DryShrub / CoastalScrub / LowlandForest
  parameter tuning.** The current v1 suitability parameters collapse
  the `volcanic_single` preset onto Grassland + BareRockLava +
  RiparianVegetation (3 biomes, passes §10 acceptance but leaves 5
  biomes at 0 % coverage). The spec explicitly parks this under
  Task 1B.9's slider UI which lands as a scaffold in this sprint but
  doesn't expose per-biome tunables yet.
- **Sprint 1B paper pack** — `docs/papers/sprint_packs/sprint_1b.md`
  Bruijnzeel 2005 / 2011 notes, Chen 2023 Budyko writeup, and Core
  Pack #2/#3/#5/#6/#8 "Sprint 1B 落地点" sections. Non-blocking per
  §7; tackle in a low-energy session.
- **`cargo run -p app` slider cadence tuning.** Re-run cost is 8
  stages at 256² ≈ 100 ms theoretical, well under the 200 ms target.
  First live session will confirm.

---

## DEVELOPMENT

### Sprint 1B — Climate + Ecology closed loop
**Status:** Library path complete on `dev` as of 2026-04-15. 14
atomic commits covering every §10 core close-out item. **269 tests**
across 8 crates (+81 from Sprint 1A's 188 baseline). Wind-direction
slider wired end-to-end (`ParamsPanel → run_from →
OverlayRenderer::refresh`) but the live `cargo run -p app` visual
verification window pass is deferred to the next session per
CLAUDE.md rule 2.
**Doc:** [`docs/design/sprints/sprint_1b_climate_ecology.md`](docs/design/sprints/sprint_1b_climate_ecology.md)
See [CURRENT FOCUS](#current-focus) and [RECENTLY SHIPPED](#recently-shipped)
for the per-task + per-commit breakdown.

### Sprint 1A — Terrain + Water Skeleton
**Status:** §3.2 Visual Package complete (A1–A6 + B3 all shipped on
`dev` as of 2026-04-14). 16-shot validation captured + audited; Pass
3.1 post-fix landed for preset framing. The 9-shot golden baseline
inherited to Sprint 1B is now parked under the 1B-tail list in
[DEFERRED TO LATER SPRINTS](#deferred-to-later-sprints) rather than
blocking 1B close-out (per sprint 1B doc §0).
**Doc:** [`docs/design/sprints/sprint_1a_terrain_water.md`](docs/design/sprints/sprint_1a_terrain_water.md)

**Shipped this pass (sim pipeline, 2026-04-14):**
- **8 sim stages** — `sim::geomorph::{TopographyStage, CoastMaskStage,
  PitFillStage, DerivedGeomorphStage}` + `sim::hydro::{FlowRoutingStage,
  AccumulationStage, BasinsStage, RiverExtractionStage}`.
- **Pipeline-end `sim::ValidationStage`** wrapping `core::validation`'s
  four invariants (`river_termination`, `basin_partition_dag`,
  `accumulation_monotone`, `coastline_consistency`).
- **`core::world::{CoastMask, FLOW_DIR_SINK, D8_OFFSETS}`** + extended
  `DerivedCaches` with all 9 Sprint 1A fields.
- **`core::neighborhood::neighbour_offsets`** shared const fn + the 3
  §D9 Sprint 1A constants.
- **`app::Runtime`** runs the full 9-stage pipeline once at startup and
  stores the populated `WorldState` behind `Runtime::world()`.
- **3 golden-seed regression snapshots** in `crates/data/golden/snapshots/`
  locked by `SummaryMetrics` (int/float/blake3 tiers) + the mandated
  field-hash classification comment.

**Shipped this pass (render-shell non-window work, 2026-04-15):**
- **`crates/render/src/palette.rs`** rebuilt: 8 canonical `[f32; 4]`
  constants (DEEP_WATER / SHALLOW_WATER / LOWLAND / MIDLAND / HIGHLAND /
  RIVER / BASIN_ACCENT / OVERLAY_NEUTRAL), all locked against
  `assets/visual/palette_reference.jpg` via pixel-sampling; `PaletteId`
  grew `TerrainHeight` + `BinaryBlue`; Viridis and Turbo are now real
  256-entry LUTs (Matplotlib BSD / Google Apache); `Categorical` uses
  a fixed 16-entry muted-blue table around `BASIN_ACCENT`.
- **`crates/render/src/camera.rs`** (new): §3.2 A6 camera preset pack —
  `PRESET_HERO` (3/4 perspective, pitch 30°, distance 1.6×r),
  `PRESET_TOP_DEBUG` (orthographic, pitch π/2−0.01), `PRESET_LOW_OBLIQUE`
  (pitch 12.5°, distance 2.0×r). Stateless `view_projection` + the
  orbit camera in `app::camera` coexist independently.
- **`crates/render/src/noise.rs`** (new): blue-noise PNG loader that
  accepts 8-bit Grayscale/Rgb/Rgba and strips RGBA→L via the R channel,
  plus a deterministic `splitmix64`-based fallback when the asset is
  missing. Calinou-format validated.
- **`assets/noise/`** — the 3 real Calinou 2D blue-noise textures
  (`blue_noise_2d_{64,128,256}.png`, copies of `LDR_LLL1_0.png`) + a
  CC0 attribution `LICENSE.md`. The shipping default test now asserts
  the loader takes the real-PNG branch rather than falling back.
- **`crates/render/src/terrain.rs`** grew `MeshData { vertices, indices }`,
  `TerrainVertex { position, normal, uv }`, `build_terrain_mesh(z_filled)`,
  and `build_sea_quad(sea_level)`. Sprint 0 `TerrainRenderer` is still
  the render path in `app::Runtime` — the new mesh builders are library
  functions only, waiting on the window-session wiring.
- **`shaders/terrain.wgsl`** (new top-level): §3.2 A1 height ramp / A2 sea
  blend / A4 key+fill+ambient lighting wired through three uniform
  buffers (`View`, `Palette`, `LightRig`). Zero color literals — the
  §3.2 acceptance grep passes. naga 29.0.1 dev-dep validates the shader
  headlessly in CI.
- **`crates/render/src/overlay.rs`** — `sprint_0_defaults()` deleted;
  `sprint_1a_defaults()` now returns the 6 real descriptors wired to
  `derived.*` fields + palette families per §3.2 A5. `final_elevation`
  source is locked to `ScalarDerived("z_filled")` (not
  `ScalarAuthoritative("height")`) by a dedicated named test.
  `ValueRange::LogCompressed` is new for the flow-accumulation overlay.

**Test deltas:** core 43 (+11), sim 62 (+62), data 10 (+3), render
56 (+45 vs Sprint 0's 11), hex 0. `cargo test --workspace` — **177
tests, 0 failed**. `cargo tree -p core` still clean of `wgpu` /
`winit` / `egui*` / `png` / `image` / `tempfile` / `naga`.

**Shipped this pass (render integration + sky gradient, 2026-04-13,
5 commits on `dev`):**
- **`7d683ca` feat(render,gpu,app) — Task 1A.9 window integration:**
  `TerrainRenderer` rewritten end-to-end. Loads `shaders/terrain.wgsl`
  via `include_str!`, VBO/IBO from `build_terrain_mesh(world.derived
  .z_filled) + build_sea_quad(preset.sea_level)`, 3 std140 uniform
  buffers (`View`/`Palette`/`LightRig`) at `@group(0) @binding(0/1/2)`.
  Palette values flow exclusively from `palette::*` constants — zero
  hex literals Rust-side. Light rig matches §3.2 A4 (`key = normalize
  (-1,-2,-1)`, `fill = normalize(1,-1,1) * 0.3`, `ambient = 0.15`).
  `GpuContext` gained a `Depth32Float` attachment recreated on resize;
  the sea quad z-fights without it. `Runtime` reorders construction so
  `TerrainRenderer::new` runs AFTER the sim pipeline, passes `&world
  + &preset` in.
- **`22d7ab6` feat(app) — UX polish:** window title → "Island Proc-Gen
  — Sprint 1A", initial size `1280×800` via `INITIAL_WINDOW_WIDTH/
  HEIGHT` consts wired through `LogicalSize::new`. New
  `crates/app/src/camera_panel.rs` with target/eye readouts, editable
  `distance/yaw°/pitch°/fov°` DragValues, a `vertical_scale` slider
  (0.1..=2.0), and a Reset-view button. `Runtime` grew a
  `vertical_scale: f32` field composed into the view-proj matrix in
  `tick()` via `Mat4::from_scale(Vec3::new(1.0, vertical_scale, 1.0))`
  right-multiplied with `camera.view_projection()`. Fragment shader
  still reads the unscaled world_pos.y passed from `vs_terrain`, so
  the sea test and elevation colouring stay canonical. Normals are
  NOT rebuilt — intentional Sprint 1A trade-off; Sprint 2+ can refit.
- **`156e21c` chore(app) — camera defaults:** `INITIAL_CAMERA_
  {DISTANCE, YAW, PITCH}` updated to `(1.44, 0.23, 0.22)` (rad) to
  match the user-verified preview view. The Sprint 0 default `pitch
  = -0.5` put the eye below sea level once back-face culling was
  enabled on the Sprint 1A terrain pipeline — fixed.
- **`835f690` feat(render,app) — §3.2 A3 sky gradient (Pass 1 of 4):**
  New `shaders/sky.wgsl` (full-screen triangle via `@builtin
  (vertex_index)`, no VBO) + `crates/render/src/sky.rs` with
  `SkyRenderer` owning a single pipeline + bind group for a 32-byte
  `Sky` uniform (horizon + zenith vec4). Pipeline uses
  `depth_write_enabled: Some(false)` + `depth_compare: Some(Always)`
  so the cleared 1.0 depth stays intact for terrain's `Less` test.
  Sky drawn BEFORE terrain in the same render pass. New non-canonical
  `palette::SKY_HORIZON` (0xB8C8D4) and `palette::SKY_ZENITH`
  (0x1C2C44) constants — explicitly NOT pixel-locked to
  `palette_reference.jpg` (the reference image has no sky panel).
  Tests: `sky_wgsl_parses_successfully` + `sky_wgsl_has_no_literal
  _colors` (180 passing total).

**Test deltas (render integration pass):** render 57 (+1 for
`terrain_vertex_layout_stride_matches_size`) → render 59 (+2 for
`sky_wgsl_*`), all other crates unchanged. `cargo test --workspace`
= **180 passed / 0 failed**. `cargo clippy --workspace -- -D warnings`
clean. `cargo tree -p core` still clean of `wgpu` / `winit` / `egui*`
/ `png` / `image` / `tempfile` / `naga`.

**Shipped this session (visual polish rollup, 2026-04-14, 4 commits
on `dev`):**
- **`ac0368d` feat(render,app) — Pass 2 / Task 1A.10 GPU overlay
  render path (§3.2 A5):** new `crates/render/src/overlay_render.rs`
  module with `OverlayRenderer` struct + pure CPU bake function
  `render_overlay_to_gpu(desc, world)` that resolves the typed
  `ResolvedField` borrow via `resolve_scalar_source` in `overlay.rs`,
  normalises per `ValueRange`, and samples the palette per cell to
  RGBA8. New `shaders/overlay.wgsl` samples the baked texture +
  per-descriptor alpha uniform and alpha-blends over terrain in the
  same render pass. The overlay pipeline shares `TerrainRenderer`'s
  view uniform + VBO/IBO via cloned `wgpu::Buffer` handles
  (Arc-refcounted in wgpu 29). Depth state is `LessEqual` +
  `depth_write_enabled = false` so overlays paint on the terrain
  surface without occluding each other. No defensive `_texture` /
  `_sampler` fields — the simplifier verified `BindGroup` refcounts
  its bound resources against wgpu-core 29.0.1 source. +5 tests
  (180 → 185), invariant #8 (string-key dispatch confined to
  `overlay.rs`) preserved.
- **`442aabe` feat(app) — Pass 3 camera preset dropdown (§3.2 A6):**
  new `Camera::apply_preset(preset, island_radius)` method + an egui
  `ComboBox` in `camera_panel.rs` that lists Hero / TopDebug /
  LowOblique and calls `apply_preset` on selection. Dropdown is
  stateless (`selectable_label(false, ...)` + `Option<CameraPreset>`
  local) — every click is a one-shot jump, orbit / pan / zoom stay
  functional after. Extracted `PITCH_CLAMP: f32 = 1.553` const
  replacing 4 pre-existing magic-number sites (two tests + `orbit` +
  the new `apply_preset`). +3 tests (185 → 188), all targeted at
  per-preset spherical coord correctness + the TopDebug clamp
  behaviour + all-three-presets finiteness round-trip.
- **`4b230ed` feat(render) — Pass 4 blue noise dither (§3.2 B3):**
  `shaders/terrain.wgsl` gains `@group(0) @binding(3)` + `@binding(4)`
  for the blue noise texture + sampler; `fs_terrain` adds
  `(textureSample(...).r - 0.5) * (1.0 / 255.0)` to `lit_rgb` as a
  ±½ LSB dither at `DITHER_TILE = 8.0`. `TerrainRenderer::new`
  uploads `load_blue_noise_2d(64)` as an R8Unorm 2D texture with
  `AddressMode::Repeat` u/v, Linear mag/min. `terrain_wgsl_has_no
  _literal_colors` still green — `0.5`, `1.0/255.0`, and
  `vec3<f32>(dither)` don't trip the grep. No test delta (shader-
  only effect).
- **`071c14a` fix(render) — Pass 3.1 preset distance factors:** post-
  validation fix for shot 21 (TopDebug FAIL) and shot 20 (Hero
  CONCERN). `PRESET_HERO.distance_factor` 1.6 → 5.0 and
  `PRESET_TOP_DEBUG.distance_factor` 1.4 → 3.5 so the orbit camera
  clears the volcano peak instead of embedding. LowOblique unchanged.
  See `RECENTLY SHIPPED` for the analytical framing verification.
  Tests still at 188 (symbol-reference, not literal).

**Still to ship for Sprint 1A §7 full acceptance:**
- **9-shot golden visual baseline** (3 presets × 3 golden seeds) —
  deferred to Sprint 1B on seed-cycling UI (see
  [DEFERRED TO LATER SPRINTS](#deferred-to-later-sprints)).
- **Paper pack §6:** Chen 2014 + Génevaux 2013 deep reads, Lague 2014
  target-deep.

**Spec clarifications discovered during implementation** (applied to the
author's Obsidian vault — see `docs/design` which is a gitignored symlink):
- **§D5 `coastal_falloff`** formula had `(1 - smoothstep(...))` which
  evaluated backwards relative to the prose intent. The stage uses the
  corrected `amplitude * smoothstep(0.9r, r, dist)` (0 inside, amplitude
  outside).
- **§D6 `flow_dir == 0`** can't be the "no downstream" sentinel because
  `E = 0` in the D8 encoding. Replaced with `FLOW_DIR_SINK = 0xFF` (now
  a shared constant in `core::world`). §Task 1A.5 and §Task 1A.7 both
  updated to reference the constant by name.
- **§Task 1A.7 sink definition** extended from
  `is_land && flow_dir == FLOW_DIR_SINK` to also include land cells whose
  D8 downstream is a sea cell or OOB. `CoastMaskStage` uses Von4 for
  `is_coast` while `FlowRoutingStage` sees Moore8, so a land cell with
  only a *diagonal* sea neighbour is not classified as coast but still
  drains directly to the ocean. Without the extension those cells and
  their upstream stay at `basin_id = 0`.
- **`RiverExtractionStage` candidates** must gate on `is_land` — sea cells
  can accumulate upstream flow via the same diagonal Moore8 edge case and
  would otherwise be flagged as river candidates. The bug surfaced during
  `ValidationStage::run()` (`river_termination` returned `RiverInSea`) —
  one of the clearest wins for running validation at the pipeline tail.
- **§3.2 Deep Water hex** drifted in the reference image:
  `palette_reference.jpg` samples `#1C416B` at every interior region,
  not `#24466B` as the table had. The eight-color constants now lock
  against pixel-samples of the image (ΔE < 6 tolerance, with Deep Water
  updated). Palette reference image is the golden source going forward.
- **§3.2 shader colour literals ban applies to WGSL too.** The acceptance
  grep covers `shaders/*.wgsl`, so `terrain.wgsl` threads all eight
  colours through a `Palette` uniform buffer instead of baking them as
  vec3/vec4 literals. Future shaders must do the same.
- **Calinou LDR_LLL1 blue-noise files are 8-bit RGBA** with L replicated
  across R=G=B, not true grayscale. `noise::try_load_png` now accepts
  Grayscale/RGB/RGBA and strips to the R channel to recover the
  luminance sample.

**Blockers:** None technical. Task 1A.9 / 1A.10 need a confirmed
`cargo run -p app` window session to close — all library code compiles
and unit-tests green.

---

## LIVE

Nothing shipped to users yet — this is a pre-alpha research project.
`cargo run -p app` opens a local window on macOS with Metal backend; no
distribution, no wasm build, no binary releases.

---

## RECENTLY COMPLETED

### Sprint 1A — Render shell library (2026-04-15, 7 commits on `dev`)

**Doc:** [`docs/design/sprints/sprint_1a_terrain_water.md`](docs/design/sprints/sprint_1a_terrain_water.md) §3.2 + §4 Task 1A.9/1A.10
**Test totals:** 177 passing across the workspace (43 core + 62 sim +
10 data + 56 render + 4 app + 2 hex + …).
**CI gate:** `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace` all green.
**Architectural invariant check:** `cargo tree -p core` clean (no
`wgpu` / `winit` / `egui` / `png` / `image` / `tempfile` / `naga`).

Delivered (everything that compiles and tests headlessly; window-session
wiring to `TerrainRenderer` + the 9 baseline screenshots are the only
Sprint 1A §7 items still open):

- **`render::palette` rebuild** — 8 canonical `[f32; 4]` constants
  locked against `assets/visual/palette_reference.jpg` via pixel-sampling,
  including the `canonical_constants_match_palette_reference` test that
  fires on any drift. Real 256-entry Matplotlib Viridis / Google Turbo
  LUTs, 16-entry muted categorical table around `BASIN_ACCENT`,
  `TerrainHeight` 3-stop lerp (LOWLAND → MIDLAND → HIGHLAND), and
  `BinaryBlue` for the river-mask overlay.
- **`render::camera` preset module** — `PRESET_HERO` /
  `PRESET_TOP_DEBUG` (orthographic) / `PRESET_LOW_OBLIQUE` with
  stateless `view_projection(preset, island_radius, aspect) -> Mat4`
  + row-major `ALL_PRESETS` + `preset_by_id` for UI wiring. The
  interactive orbit camera in `app::camera` is unchanged.
- **`render::noise` blue-noise loader** — `load_blue_noise_2d(size)`
  reads 8-bit Grayscale/RGB/RGBA (strips to R channel for Calinou's
  LDR_LLL1 format) and falls back to a deterministic splitmix64-based
  pattern on any failure. Real 2D textures checked in at
  `assets/noise/blue_noise_2d_{64,128,256}.png` (copies of
  `LDR_LLL1_0.png` from Calinou/free-blue-noise-textures, CC0)
  with `assets/noise/LICENSE.md` attribution.
- **`render::terrain` mesh builder** — `MeshData` + `TerrainVertex` +
  `build_terrain_mesh(&ScalarField2D<f32>)` producing a full
  `sim_width * sim_height` grid mesh with central-diff normals
  (single-sided at edges), plus `build_sea_quad(sea_level)`. Sprint 0
  `TerrainRenderer` is still the live render path — the new mesh
  builders are library functions.
- **`shaders/terrain.wgsl`** (new top-level directory) — §3.2 A1
  height ramp + A2 sea-depth blend + A4 key/fill/ambient lighting,
  threaded through `View` / `Palette` / `LightRig` uniform buffers.
  ZERO colour literals in the WGSL; grep + a dedicated test enforce
  this. naga 29.0.1 dev-dep validates the shader semantically in a
  headless test.
- **`render::overlay` Task 1A.10 repointing** — `sprint_0_defaults()`
  replaced by `sprint_1a_defaults()` returning the 6 real Sprint 1A
  overlays wired to their actual `derived.*` fields.
  `final_elevation.source == ScalarDerived("z_filled")` is the
  mandatory §7 criterion, locked by a dedicated named test.
  `ValueRange::LogCompressed` is new for `flow_accumulation`.

### Sprint 1A — Terrain + Water Skeleton (sim pipeline, 2026-04-14, 10 commits on `dev`)

**Doc:** [`docs/design/sprints/sprint_1a_terrain_water.md`](docs/design/sprints/sprint_1a_terrain_water.md)
**Test totals:** 115 passing (43 core + 62 sim + 10 data + 0 hex).
**CI gate:** `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test -p core -p sim -p hex -p data` all green.
**Architectural invariant check:** `cargo tree -p core` clean (no `wgpu` /
`winit` / `egui` / `png` / `image` / `tempfile`).

Delivered (this pass covers Tasks 1A.1–1A.8, 1A.11, 1A.12 + the
`app::Runtime` wiring):

- **8 sim stages + pipeline-end validation** — see the Sprint 1A
  DEVELOPMENT entry above for the full stage list.
- **3-layer `DerivedCaches` fully populated at boot.** Every field the
  sprint doc §3.1 promised (`initial_uplift`, `z_filled`, `slope`,
  `coast_mask`, `shoreline_normal`, `flow_dir`, `accumulation`,
  `basin_id`, `river_mask`) is written by the Sprint 1A pipeline run.
- **§D9 neighborhood constants** — `COAST_DETECT_NEIGHBORHOOD = Von4`
  (coastline aesthetics), `RIVER_CC_NEIGHBORHOOD = Moore8` (connect
  diagonally-reaching rivers), `RIVER_COAST_CONTACT = Moore8` (keep
  river components that only touch the coast diagonally) all live in
  `core::neighborhood` behind a shared `neighbour_offsets()` helper.
- **`core::validation`** — four pure-CPU invariant functions with their
  own unit tests, plus a thin `sim::ValidationStage` wrapper so
  `SimulationPipeline::run` asserts correctness at the tail.
- **Golden-seed regression** — 3 (seed, preset) pairs at 128x128 snapshot
  int/float/blake3 tiers. Re-running the pipeline on the same host is
  bit-exact; cross-platform drift falls through to the 1e-4 float
  tolerance per the mandated field-hash semantics comment block.
- **`app::Runtime`** now depends on `sim`, runs the full 9-stage pipeline
  before the window opens, and logs `land_cells` at completion. Pipeline
  errors prevent window creation via `?`.

Not yet done (see DEVELOPMENT above): Task 1A.9 render mesh + §3.2 Visual
Package, Task 1A.10 overlay wiring, paper-pack deep reads.

### Sprint 0 — Scaffolding (2026-04-13, 14 commits on `dev`)

**Doc:** [`docs/design/sprints/sprint_0_scaffolding.md`](docs/design/sprints/sprint_0_scaffolding.md)
**Test totals:** 56 passing (32 core + 7 data + 11 render + 4 camera + 2 save_io).
**CI gate:** `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test -p core -p sim -p hex -p data` all green.
**Architectural invariant check:** `cargo tree -p core` clean (no `wgpu` /
`winit` / `egui` / `png` / `image`).

Delivered:

- **Workspace foundation** — 8 crates (`app` bin + `core`, `gpu`, `render`,
  `sim`, `hex`, `ui`, `data` libs) with `[workspace.package]` metadata
  inheritance. Rust toolchain pinned to stable. Graphics stack pinned:
  `egui` / `egui-wgpu` / `egui-winit` at 0.34.1, `wgpu` 29.0.1, `winit` 0.30.13.
- **`core::field`** — `ScalarField2D<T>` + `MaskField2D = ScalarField2D<u8>` +
  `VectorField2D = ScalarField2D<[f32; 2]>` aliases with byte-level
  `to_bytes` / `from_bytes` (magic `IPGF`, format_version 1, four dtypes via a
  sealed `pub(crate) trait FieldDtype`). No Path, no PNG, no `Vec<bool>`.
- **`core::seed`** — `Seed(u64)` newtype with `rand_pcg::Pcg64Mcg` RNG and
  `fork(stream)` using splitmix64 mixing for independent per-module RNG
  streams.
- **`core::preset`** + **`crates/data`** — `IslandArchetypePreset` with 8
  fields + `IslandAge` enum. Three built-in RON presets: `volcanic_single`,
  `volcanic_twin`, `caldera`. `data::presets::load_preset(name)` with
  structured `PresetLoadError` (NotFound / Io / Parse).
- **`core::world::WorldState`** — three-layer split enforced from day zero:
  `{ seed, preset, resolution, authoritative, baked, derived }`.
  `AuthoritativeFields { height, sediment }` both default-`None` awaiting
  Sprint 1A / 3. `BakedSnapshot` and `DerivedCaches` are intentionally empty
  structs — future fields land inside them, not on the top level.
- **`core::pipeline`** — `SimulationStage` trait (object-safe), `SimulationPipeline`
  with `tracing::info!` per stage, `NoopStage` placeholder, plus the headline
  `pipeline_runs_without_graphics` invariant test that proves `WorldState` +
  `SimulationPipeline` can construct and run without linking `wgpu` / `winit`
  / `egui`. The test uses an inline preset helper to avoid a `core` → `data`
  back-edge.
- **`core::save` + `app::save_io`** — byte-level codec with
  `SaveMode { SeedReplay, Minimal, Full, DebugCapture }` framed by `IPGS`
  magic + `format_version = 1`. Only `SeedReplay` and `Minimal` are
  implemented; `Full` and `DebugCapture` return `NotYetSupported`. The
  `read_world` API returns a `LoadedWorld` enum so `SeedReplay` can carry the
  preset _name_ only (the `app` layer re-resolves via `data::presets`), keeping
  `core::save` free of any `data` dependency. `app::save_io` is a 5-line
  Path wrapper delegating to the byte-level API.
- **`app` + `gpu` + `render`** — Winit 0.30 `ApplicationHandler` event loop,
  `GpuContext` owning wgpu 29 `Instance` / `Surface` / `Adapter` / `Device` /
  `Queue`, placeholder `TerrainRenderer` drawing a colored quad via an inline
  WGSL pipeline (red / green / blue / yellow corners on the XZ plane), egui
  0.34 `begin_pass` / `end_pass` panel stack, and an orbit / pan / zoom
  `Camera` built on `glam::Mat4`.
- **`render::overlay` + `crates/ui`** — descriptor-based `OverlayRegistry`
  (`Vec<OverlayDescriptor>`, no draw closures), `OverlaySource` enum confining
  `&'static str` field-keys to one file. Three Sprint 0 placeholder entries
  (`initial_uplift`, `final_elevation`, `flow_accumulation`) whose source
  strings match the field names Sprint 1A will add. `ui::OverlayPanel` /
  `ParamsPanel` / `StatsPanel` wired into `app::Runtime`'s egui pass.
- **CI** — `.github/workflows/ci.yml` on macOS runner: fmt-check, clippy
  `-D warnings`, and headless tests (`-p core -p sim -p hex -p data`). App /
  render / gpu tests excluded (no display on CI runner).
- **Paper knowledge base** — `docs/papers/README.md` with A/B/C/D layering.
  12 Core Pack paper stubs under `docs/papers/core_pack/` with frontmatter +
  abstract + 一句话用途 sections. 8 PDFs downloaded (target 6–8 met). Chen
  2014 and Temme 2017 have substantive non-TODO `对本项目的落地点` sections
  pointing at specific `crates/sim/...` files that Sprint 1A will produce.

---

## UPCOMING SPRINTS

| Sprint | Focus | Plan doc |
|---|---|---|
| 1A | Terrain + water skeleton (TopographyStage, FlowRoutingStage, AccumulationStage, RiverExtractionStage) | `docs/design/sprints/sprint_1a_terrain_water.md` |
| 1B | Climate + ecology (orographic precipitation, biome assignment) + hex aggregation start | `docs/design/sprints/sprint_1b_climate_ecology.md` |
| 2 | Geomorphology credibility (SPIM tuning, golden seed snapshots, reference metrics) | `docs/design/sprints/sprint_2_geomorph_credibility.md` |
| 3 | Sediment + advanced climate (SPACE-style coupled water/sediment) | `docs/design/sprints/sprint_3_sediment_advanced_climate.md` |
| 4 | GPU compute shaders + CLI headless batch + PNG gallery export | `docs/design/sprints/sprint_4_gpu_compute.md` |
| 5 | Hex aggregation finalization + semantic web (wasm) export | `docs/design/sprints/sprint_5_hex_semantic_web.md` |

---

## ON ICE

Nothing paused.

---

## QUICK REFERENCE

**High energy?** → Start Sprint 2 — geomorph credibility. The
Sprint 1B pipeline's `precipitation`, `accumulation`, `slope`, and
`z_filled` are exactly the four fields Sprint 2's stream-power
incision stage reads, so the first task is a new
`sim::geomorph::StreamPowerIncisionStage` that mutates
`authoritative.height` via an explicit Euler step. Touches
`crates/sim/src/geomorph/` and adds a 9th `StageId` ordinal after
`RiverExtraction`. Sprint 2 doc: `docs/design/sprints/sprint_2_geomorph_credibility.md`.
**Medium energy?** → Ship the `cargo run -p app` visual verification
of the Sprint 1B wind slider. Check with user first, then drag the
slider and inspect the precipitation / biome overlays for live
refresh. If the slider looks laggy, measure `run_from` wall-clock in
`Runtime::tick` and compare to the 200 ms §10 target.
**Low energy?** → Sprint 1B paper pack. Create
`docs/papers/sprint_packs/sprint_1b.md` per sprint doc §7: Bruijnzeel
2005 / 2011 TMCF notes, Chen 2023 Budyko readthrough, and Core Pack
#2/#3/#5/#6/#8 "Sprint 1B 落地点" sections pointing back at DD2 / DD4
/ DD6 anchor points. Also fill the Sprint 1A Chen 2014 / Génevaux
2013 deep reads still outstanding at `docs/papers/core_pack/`.
**Quick win?** → Tune `suitability.rs` parameters so more than 3
biomes appear in `volcanic_single`. Current output collapses onto
Grassland / BareRockLava / Riparian. Widen the σ on LowlandForest
and MontaneWetForest bells, or lower the `soil_moisture` thresholds
— Task 1B.9 adds the slider hooks, so this can be explored
interactively once the live window session happens.

---

**Update this file whenever a sprint ships, scope shifts, or a blocker moves.
Weekly minimum during active sprints.**
