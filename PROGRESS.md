# PROGRESS

**Last Updated:** 2026-04-14

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

**Primary:** Sprint 1A — Terrain + Water Skeleton **simulation pipeline
shipped**. All 8 sim stages plus validation + golden-seed regression land
on `dev`; `cargo run -p app` still renders the Sprint 0 placeholder quad
but the full `TopographyStage → CoastMaskStage → PitFillStage →
DerivedGeomorphStage → FlowRoutingStage → AccumulationStage → BasinsStage
→ RiverExtractionStage → ValidationStage` chain runs once at startup and
produces a fully-populated `WorldState` before the window opens.

**Deferred from Sprint 1A (not yet shipped):**
- **Task 1A.9** — real terrain mesh + §3.2 Visual Package (palette, camera
  presets, lighting rig, blue-noise download, `shaders/terrain.wgsl`). Needs
  a confirmed `cargo run -p app` window pass.
- **Task 1A.10** — 6 descriptor-based overlays wired to the new
  `derived.*` fields.
- **Paper pack (non-blocking per §6)** — Chen 2014 / Génevaux 2013 deep
  reads plus the 4 Core-Pack / Sprint-Pack notes still sitting at
  `metadata_only`.

---

## DEVELOPMENT

### Sprint 1A — Terrain + Water Skeleton (simulation portion shipped)
**Status:** Sim pipeline shipped on `dev` as of 2026-04-14. Render shell
(Task 1A.9) and overlay wiring (Task 1A.10) still pending.
**Doc:** [`docs/design/sprints/sprint_1a_terrain_water.md`](docs/design/sprints/sprint_1a_terrain_water.md)

**Shipped this pass:**
- **8 sim stages** — `sim::geomorph::{TopographyStage, CoastMaskStage,
  PitFillStage, DerivedGeomorphStage}` + `sim::hydro::{FlowRoutingStage,
  AccumulationStage, BasinsStage, RiverExtractionStage}`.
- **Pipeline-end `sim::ValidationStage`** wrapping `core::validation`'s
  four invariants (`river_termination`, `basin_partition_dag`,
  `accumulation_monotone`, `coastline_consistency`).
- **`core::world::{CoastMask, FLOW_DIR_SINK, D8_OFFSETS}`** + extended
  `DerivedCaches` with all 9 Sprint 1A fields (`initial_uplift`, `z_filled`,
  `slope`, `coast_mask`, `shoreline_normal`, `flow_dir`, `accumulation`,
  `basin_id`, `river_mask`).
- **`core::neighborhood::neighbour_offsets`** shared const fn + the 3
  §D9 Sprint 1A constants (`COAST_DETECT_NEIGHBORHOOD = Von4`,
  `RIVER_CC_NEIGHBORHOOD = Moore8`, `RIVER_COAST_CONTACT = Moore8`).
- **`app::Runtime`** runs the full 9-stage pipeline once at startup and
  stores the populated `WorldState` behind `Runtime::world()` for Sprint 1B+
  overlay bindings.
- **3 golden-seed regression snapshots** in `crates/data/golden/snapshots/`:
  `seed_42_volcanic_single.ron`, `seed_123_volcanic_twin.ron`,
  `seed_777_caldera.ron`. `SummaryMetrics` covers 5 integer counters, 5
  float aggregates, and 6 blake3 field hashes with the mandated
  `// Field hash vs. abs-tolerance semantics` classification comment.

**Test deltas:** core 43 (+11), sim 62 (+62), data 10 (+3), hex 0 —
**total 115 passing** (was 56 at end of Sprint 0). `cargo tree -p core`
still clean of `wgpu` / `winit` / `egui*` / `png` / `image` / `tempfile`.

**Still to ship for Sprint 1A §6 full acceptance:**
- **Task 1A.9:** replace the Sprint 0 rainbow quad with a real
  `sim_width * sim_height` triangle mesh driven by `derived.z_filled`,
  plus the §3.2 Visual Package — canonical 8-colour palette in
  `crates/render/src/palette.rs`, `shaders/terrain.wgsl` combining A1
  terrain / A2 sea / A3 sky / A4 lighting, `crates/render/src/camera.rs`
  preset pack (Hero / Top Debug / Low Oblique), and blue-noise
  download + runtime loader.
- **Task 1A.10:** repoint the 6 `OverlayDescriptor` sources at the real
  `derived.*` fields (descriptor-based, no draw closures) and lock the
  overlay palettes per §3.2 A5.
- **Paper pack §6:** Chen 2014 + Génevaux 2013 are the mandatory deep
  reads before closing Sprint 1A; Lague 2014 is target-deep; the
  three background-organization papers can stay at
  `status: metadata_only`.

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

**Blockers:** None technical. Task 1A.9 / 1A.10 need a confirmed
`cargo run -p app` window session to close.

---

## LIVE

Nothing shipped to users yet — this is a pre-alpha research project.
`cargo run -p app` opens a local window on macOS with Metal backend; no
distribution, no wasm build, no binary releases.

---

## RECENTLY COMPLETED

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
