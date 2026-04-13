# PROGRESS

**Last Updated:** 2026-04-13

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

**Primary:** Sprint 1A — Terrain + Water Skeleton. First real simulation
stages (volcanic topography, flow routing, accumulation, river extraction)
that replace the Sprint 0 rainbow placeholder quad with a genuine heightfield
mesh.

**Secondary:** Sprint 1A prep-reading. Chen 2014 (LEM equation framework) and
Temme 2017 (scope-vs-fidelity decision matrix) are the two mandatory reads
before committing Sprint 1A implementation. Both already have `对本项目的落地点`
sections in `docs/papers/core_pack/` pointing at the specific files Sprint 1A
will produce.

---

## DEVELOPMENT

### Sprint 1A — Terrain + Water Skeleton
**Status:** Not started (gated only on Chen/Temme read-through).
**Doc:** [`docs/design/sprints/sprint_1a_terrain_water.md`](docs/design/sprints/sprint_1a_terrain_water.md)

**Entry points already in the tree, waiting to be filled:**
- `core::world::AuthoritativeFields.height: Option<ScalarField2D<f32>>` — land
  the real heightfield here.
- `core::world::DerivedCaches` — add `flow_dir: Option<ScalarField2D<u8>>`,
  `accumulation: Option<ScalarField2D<f32>>`, `basin_id: Option<ScalarField2D<u32>>`,
  `river_mask: Option<MaskField2D>`, `coast_mask: Option<MaskField2D>`.
- `core::pipeline::SimulationStage` — implement it for each stage in
  `crates/sim/src/geomorph/` (`topography.rs`) and `crates/sim/src/hydro/`
  (`flow_routing.rs`, `accumulation.rs`, `river_extraction.rs`).
- `render::OverlayRegistry::sprint_0_defaults` — three placeholder descriptors
  already point at the field-key strings Sprint 1A needs (`height`,
  `initial_uplift`, `accumulation`). Keep those strings in sync when the real
  fields land.
- `core::save` Minimal mode already serializes `authoritative.height` +
  `authoritative.sediment` — once Sprint 1A populates `height`, that path is
  live without any codec change.

**Next concrete action:** Fill the `关键方程` and `对本项目的落地点` equation
dump in `docs/papers/core_pack/chen_2014_lem_review.md`, then create
`crates/sim/src/geomorph/topography.rs` with `TopographyStage` and its unit
test. Wire `TopographyStage` into the pipeline in `crates/app/src/runtime.rs`.

**Blockers:** None technical.

---

## LIVE

Nothing shipped to users yet — this is a pre-alpha research project.
`cargo run -p app` opens a local window on macOS with Metal backend; no
distribution, no wasm build, no binary releases.

---

## RECENTLY COMPLETED

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

**High energy?** → Read Chen 2014, then draft `TopographyStage` in
`crates/sim/src/geomorph/topography.rs`.
**Low energy?** → Fill in Sprint-1A-deferred `TODO` sections in
`docs/papers/core_pack/*.md` with real equation dumps for the papers that
currently only have frontmatter.
**Quick win?** → Download the four remaining `metadata_only` PDFs
(`chen_2014_lem_review`, `smith_barstad_2004_linear_orographic`,
`fisher_2018_vegetation_demographics_esm`, `hergarten_robl_2022_lfpm`) when
institutional access is available.

---

**Update this file whenever a sprint ships, scope shifts, or a blocker moves.
Weekly minimum during active sprints.**
