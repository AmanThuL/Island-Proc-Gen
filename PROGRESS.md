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

**Primary:** Sprint 1A — simulation pipeline **and** non-window-dependent
render shell both shipped on `dev`. What's left is a single window-
session pass to actually wire the new `MeshData` + `shaders/terrain.wgsl`
+ 6 overlays into `TerrainRenderer`, swap the Sprint 0 rainbow quad for
the real mesh, and capture the 9 baseline screenshots §7 requires.

All Sprint 1A code that can be unit-tested headlessly is in place:
8 sim stages → 4 validation invariants → `sim::ValidationStage` at the
pipeline tail → 3 golden-seed regression snapshots → canonical
8-colour palette locked against `assets/visual/palette_reference.jpg` →
Viridis / Turbo / Categorical / TerrainHeight / BinaryBlue LUTs →
Hero / TopDebug / LowOblique camera presets → blue-noise loader with
the 3 Calinou 2D textures checked in + a deterministic fallback →
`build_terrain_mesh` / `build_sea_quad` mesh builders →
`shaders/terrain.wgsl` combining §3.2 A1 / A2 / A4 via a uniform-buffer
palette (naga-validated headlessly) → 6 real `OverlayDescriptor`s
pointing at the correct `derived.*` fields.

**Remaining for Sprint 1A §6 full acceptance:**
- **Window session (Task 1A.9 integration):** refactor `TerrainRenderer`
  to consume `MeshData` + `shaders/terrain.wgsl`, populate the `View`
  / `Palette` / `LightRig` uniform buffers, wire the camera-preset
  dropdown into `ParamsPanel`, and hand off `world.derived.z_filled`
  + `preset.sea_level` at boot. Needs `cargo run -p app`.
- **Overlay render path (Task 1A.10 integration):** CPU-side texture
  upload from the 6 `OverlayDescriptor` sources via a single
  `render_overlay_to_gpu(desc, world)` helper, with alpha blending
  over the terrain pass. Needs the window session too.
- **9 baseline screenshots:** 3 camera presets × 3 golden seeds in
  `docs/design/sprints/sprint_1a_visual_acceptance/` as the Sprint 1B
  regression baseline.
- **Paper pack (non-blocking per §6):** Chen 2014 / Génevaux 2013 deep
  reads; Lague 2014 target-deep; background papers can stay at
  `metadata_only`.

---

## DEVELOPMENT

### Sprint 1A — Terrain + Water Skeleton
**Status:** Sim pipeline and render-shell library pieces all shipped on
`dev` as of 2026-04-15. Only the actual render-path integration + the
9-screenshot visual baseline remain, both of which need a window session.
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

**Still to ship for Sprint 1A §6 full acceptance:**
- **Window-session integration (Task 1A.9):** swap Sprint 0's `TerrainRenderer`
  for a new pipeline consuming `MeshData` + `shaders/terrain.wgsl`,
  populate `View` / `Palette` / `LightRig` uniforms from
  `palette::*` + `preset.sea_level` + a light rig const, wire the
  camera preset dropdown into `ParamsPanel`, add a depth buffer (the
  Sprint 0 path has no depth attachment and the sea quad will z-fight
  the terrain mesh).
- **Overlay render path (Task 1A.10):** one `render_overlay_to_gpu(desc,
  world)` per visible descriptor — CPU-side texture upload keyed by
  `OverlaySource`, alpha-blended over the terrain. Shader can reuse
  `palette::sample_f32`.
- **9-screenshot visual baseline** in
  `docs/design/sprints/sprint_1a_visual_acceptance/` (3 camera presets
  × 3 golden seeds).
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
Sprint 1A §6 items still open):

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

**High energy?** → Close Task 1A.9: wire `derived.z_filled` into a real
triangle mesh in `crates/render/src/terrain.rs`, land the §3.2 canonical
palette in `crates/render/src/palette.rs`, and a first cut of
`shaders/terrain.wgsl` combining A1–A4. Requires a confirmed
`cargo run -p app` window pass with me before shipping.
**Medium energy?** → Close Task 1A.10: repoint the 6 `OverlayDescriptor`
`source` fields at the real `derived.*` field names. No draw closures
(§CLAUDE.md invariant #7). Test that each overlay toggles cleanly.
**Low energy?** → Fill the `关键方程` and `对本项目的落地点` sections in
`docs/papers/core_pack/chen_2014_lem_review.md` and
`genevaux_2013_hydrology_terrain.md` — both are mandatory deep reads per
sprint doc §6, currently only frontmatter. Also Kwang & Parker's
`m/n = 0.5` warning still needs to land in
`sprint_2_geomorph_credibility.md` open questions.
**Quick win?** → Download the four remaining `metadata_only` PDFs
(`chen_2014_lem_review`, `smith_barstad_2004_linear_orographic`,
`fisher_2018_vegetation_demographics_esm`, `hergarten_robl_2022_lfpm`) when
institutional access is available.

---

**Update this file whenever a sprint ships, scope shifts, or a blocker moves.
Weekly minimum during active sprints.**
