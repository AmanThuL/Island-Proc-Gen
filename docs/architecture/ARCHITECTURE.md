# Architecture

Preliminary architecture overview for Island-Proc-Gen. Intended as a
reference for humans reading the codebase cold — covers the crate
layout, how simulation data flows through the pipeline, how that data
reaches the screen, and the hard invariants that keep the system
extensible.

This document complements two other tracked files:

- [`CLAUDE.md`](../../CLAUDE.md) — operating notes for AI coding
  agents (gotchas, commit style, session protocol).
- [`PROGRESS.md`](../../PROGRESS.md) — sprint-level dashboard with
  commit-level history.

For the big-picture research roadmap (non-tracked, lives in the
author's Obsidian vault) see `docs/design/` locally.

---

## 1. What the project is

A deterministic procedural generator for volcanic islands, written in
Rust. A linear pipeline of simulation stages produces continuous 2D
fields — terrain height, slope, flow accumulation, temperature,
precipitation, soil moisture, biome weights, and a hex-aggregated
summary — over a square grid (currently 256×256). Those fields are
rendered live with `wgpu` + `egui` on macOS / Metal and will
eventually be exportable as CPU-side PNG galleries for headless
batch runs and as a wasm build for the semantic-web viewer.

Everything is driven by a single `(Seed, IslandArchetypePreset)`
pair. Same seed + same preset → bit-exact same output, regardless
of how many times the pipeline re-runs. Load-time rebuild from a
saved seed is the canonical pattern for small save files; baked
snapshot fields are written by the pipeline, not carried by serde.

---

## 2. Crate layout

The workspace has 8 crates. Dependencies flow strictly downward to
`core` — no back-edges, no cycles, no shortcuts.

```mermaid
flowchart LR
    app["<b>app</b><br/><i>winit event loop,<br/>orbit camera, save IO</i>"]
    render["<b>render</b><br/><i>wgpu pipelines,<br/>palette, overlays</i>"]
    gpu["<b>gpu</b><br/><i>wgpu device,<br/>surface, depth</i>"]
    ui["<b>ui</b><br/><i>egui panels</i>"]
    sim["<b>sim</b><br/><i>16 pipeline stages,<br/>ValidationStage</i>"]
    hex["<b>hex</b><br/><i>HexGrid,<br/>aggregation</i>"]
    data["<b>data</b><br/><i>presets,<br/>golden snapshots</i>"]
    core["<b>core</b><br/><i>WorldState,<br/>field types,<br/>SimulationPipeline,<br/>validation</i>"]

    app --> render
    app --> ui
    app --> sim
    app --> data
    render --> gpu
    render --> core
    gpu --> core
    ui --> core
    sim --> core
    sim --> hex
    hex --> core
    data --> core
```

`core` is a sink crate. `app` is the only crate allowed to wire
everything together — it imports the sim pipeline, the render
pipelines, the UI panels, and the data loaders, then runs the
event loop.

### Why this shape

- `core` has no graphics dependency. `cargo tree -p core` is
  free of `wgpu`, `winit`, `egui`, `png`, `image`, and `tempfile`
  — enforced by the `pipeline_runs_without_graphics` test. This
  is what makes the wasm target feasible without rewriting the
  simulation.
- `render` depends on `core` (reads `WorldState`) and `gpu` (needs
  a device) but not on `sim`. Overlays dispatch over
  `&'static str` field-keys inside `overlay.rs` and nowhere else.
- `data` depends on `core` but never the other way around. Golden
  snapshots and RON presets never pollute the core state layer.

### The `core` crate name shadows stdlib `::core`

Downstream crates import it as `island_core = { path = "../core", package = "core" }`
to avoid path clashes. `crates/core/Cargo.toml` has `[lib] doctest = false`
for the same reason (rustdoc runs `--crate-name core` and
`thiserror`'s derive expands `::core::fmt` paths that can't resolve
inside a user crate called `core`).

---

## 3. Data model: `WorldState`

Every simulation stage reads from and writes to a single
`WorldState` value. The top-level fields are frozen — adding a new
field never means adding a top-level `Option<...>`, only filling in
an inner struct.

```mermaid
classDiagram
    class WorldState {
        +Seed seed
        +IslandArchetypePreset preset
        +Resolution resolution
        +AuthoritativeFields authoritative
        +BakedSnapshot baked
        +DerivedCaches derived
    }
    class AuthoritativeFields {
        <<world-truth, serde-serialized>>
        +Option~ScalarField2D~f32~~ height
        +Option~ScalarField2D~f32~~ sediment
    }
    class BakedSnapshot {
        <<stable derived, save-visible>>
        +Option~ScalarField2D~f32~~ temperature
        +Option~ScalarField2D~f32~~ precipitation
        +Option~ScalarField2D~f32~~ soil_moisture
        +Option~BiomeWeights~ biome_weights
    }
    class DerivedCaches {
        <<runtime cache, #[serde(skip)]>>
        +Option~ScalarField2D~f32~~ z_filled
        +Option~ScalarField2D~f32~~ slope
        +Option~ScalarField2D~f32~~ curvature
        +Option~CoastMask~ coast_mask
        +Option~ScalarField2D~u8~~ flow_dir
        +Option~ScalarField2D~f32~~ accumulation
        +Option~ScalarField2D~u32~~ basin_id
        +Option~MaskField2D~ river_mask
        +Option~ScalarField2D~f32~~ fog_likelihood
        +Option~ScalarField2D~f32~~ pet
        +Option~ScalarField2D~f32~~ et
        +Option~ScalarField2D~f32~~ runoff
        +Option~ScalarField2D~u32~~ dominant_biome_per_cell
        +Option~HexGrid~ hex_grid
        +Option~HexAttrs~ hex_attrs
    }
    WorldState --> AuthoritativeFields
    WorldState --> BakedSnapshot
    WorldState --> DerivedCaches
```

The three layers answer three different questions:

| Layer | Question | Save behaviour |
|-------|----------|----------------|
| `authoritative` | "What did erosion / deposition do to the heightfield?" | Written to save files via `ScalarField2D::to_bytes` (the `IPGF` format) |
| `baked` | "What stable derived snapshots do we show to users and golden-test against?" | Written to `SaveMode::Full` (not yet implemented) |
| `derived` | "What runtime caches do we rebuild from `authoritative` on load?" | `#[serde(skip)]` — rebuilt via `run_from(StageId::Coastal)` |

### Field storage

All continuous 2D fields share three type aliases:

- `ScalarField2D<T>` — row-major `Vec<T>` plus width/height.
- `MaskField2D = ScalarField2D<u8>` — 0/1 masks, never `Vec<bool>`
  (so they upload to the GPU and serialize as contiguous bytes).
- `VectorField2D = ScalarField2D<[f32; 2]>` — 2D vectors per cell.

No `trait Field`. If a future stage needs a new dtype, add it to
the sealed `pub(crate) trait FieldDtype` in `core::field` (this is
what gates `to_bytes` / `from_bytes` polymorphism). Everything else
uses the concrete aliases above.

---

## 4. Simulation pipeline

The canonical pipeline is a **17-stage pipeline (16 `StageId` variants +
terminal `ValidationStage`)** that runs 8 invariants at the tail. `StageId` in
`crates/sim/src/lib.rs` is the single source of truth for pipeline
indices — every `SimulationPipeline::run_from` caller passes
`StageId::X as usize`, never a literal.

```mermaid
flowchart TB
    subgraph Geo["Geomorph + Hydro (indices 0–7)"]
        direction TB
        S0[0&nbsp;Topography] --> S1[1&nbsp;Coastal]
        S1 --> S2[2&nbsp;PitFill]
        S2 --> S3[3&nbsp;DerivedGeomorph]
        S3 --> S4[4&nbsp;FlowRouting]
        S4 --> S5[5&nbsp;Accumulation]
        S5 --> S6[6&nbsp;Basins]
        S6 --> S7[7&nbsp;RiverExtraction]
    end
    subgraph Clim["Climate + Ecology + Hex (indices 8–15)"]
        direction TB
        S8[8&nbsp;Temperature] --> S9[9&nbsp;Precipitation]
        S9 --> S10[10&nbsp;FogLikelihood]
        S10 --> S11[11&nbsp;Pet]
        S11 --> S12[12&nbsp;WaterBalance]
        S12 --> S13[13&nbsp;SoilMoisture]
        S13 --> S14[14&nbsp;BiomeWeights]
        S14 --> S15[15&nbsp;HexProjection]
    end
    S7 --> S8
    S15 --> V[ValidationStage&nbsp;tail<br/>8 invariants]

    Slider([Wind slider]) -. run_from&nbsp;StageId::Precipitation .-> S9
```

### `run_from` semantics

`SimulationPipeline::run_from(world, start_index)` runs stages
`[start_index..len())` in push order. Preconditions: every field
produced by the prefix `[0..start_index)` must already be populated
on the `WorldState`. The pipeline doesn't introspect stage outputs;
each stage is responsible for its own "missing precondition" error
when an input field is `None`.

Three canonical callers:

- `run_from(0)` — a fresh world or a `SaveMode::Minimal` load.
- `run_from(StageId::Precipitation as usize)` — slider handler for
  a climate parameter change.
- `run_from(StageId::Coastal as usize)` — `SaveMode::Full` load
  rebuilds every `derived` field from `authoritative.height`
  without re-running `TopographyStage`.

### Writing a new stage

1. Implement `SimulationStage` (trait in `crates/core/src/pipeline.rs`):
   ```rust
   pub trait SimulationStage {
       fn name(&self) -> &'static str;
       fn run(&self, world: &mut WorldState) -> anyhow::Result<()>;
   }
   ```
2. Decide which layer the output lives in (`authoritative`,
   `baked`, or `derived`) and add the field inside that struct
   in `core::world`.
3. Add a new variant to `StageId`; update `stage_id_indices_are_dense_and_canonical`.
4. Push the stage in the pipeline builder `sim::default_pipeline()` in
   `crates/sim/src/lib.rs` at the right index.
5. If the new stage produces a validation-checkable output, add
   an invariant to `core::validation` and wire it into
   `sim::ValidationStage`.
6. If the stage is a slider target, add a UI control in
   `crates/ui/src/params_panel.rs` and a `run_from(StageId::X)` branch
   in `Runtime::tick`.

---

## 5. Render + UI runtime

`Runtime::tick` runs once per frame. The simulation pipeline already
ran at boot, so the tick loop is render + UI + slider-driven re-runs.

```mermaid
sequenceDiagram
    participant W as winit
    participant R as Runtime::tick
    participant E as egui
    participant P as SimulationPipeline
    participant O as OverlayRenderer
    participant G as wgpu

    W->>R: RedrawRequested
    R->>G: update view uniform (camera)
    R->>G: sky + terrain + overlay passes
    R->>E: begin_pass, show 4 panels
    E-->>R: ParamsPanelResult
    alt slider moved
        R->>R: world.preset = self.preset.clone()
        R->>P: run_from(StageId::X)
        P->>P: re-run stages [X..=15] + ValidationStage
        R->>O: refresh(gpu, world, registry)
        O->>G: re-upload affected overlay textures
    end
    R->>G: egui render pass
    G->>W: present
```

### Overlay rendering path

Overlays are data descriptors, not render closures.
`OverlayRegistry` in `crates/render/src/overlay.rs` stores
`Vec<OverlayDescriptor>` where each descriptor names a source field
by `&'static str`, a palette, and a value range. The `draw` step
resolves the source to a typed field borrow via
`resolve_scalar_source` (which is the *only* place in the codebase
that string-key dispatches over `WorldState` layout), then samples
the palette per cell into an RGBA8 texture.

The overlay descriptor contract is what lets the same descriptor
drive both the real-time GPU render path (today) and the CPU-side
PNG batch export path (§6 below). Both paths share one bake
function — `render::bake_overlay_to_rgba8(desc, world) -> Option<(Vec<u8>, u32, u32)>`
— so the interactive renderer and the `--headless` harness are
byte-identical by construction. Any "render closure" shortcut
would break that sharing and must be rejected.

---

## 6. Headless harness

A second, windowless execution path drives the same pipeline and
the same renderers for batch capture, regression, and AI-agent
validation:

```
cargo run -p app -- --headless         <request.ron>
cargo run -p app -- --headless-validate <run_dir> --against <expected_dir>
```

Both sub-commands skip the winit event loop entirely, write
artifacts under a deterministic directory tree, and return an
AD9-locked `OverallStatus` that `main.rs` maps to a `0 / 2 / 3`
process exit byte.

### The two capture paths

```mermaid
flowchart LR
    subgraph Truth["Truth path — deterministic, authoritative"]
        direction TB
        W["WorldState<br/>(fully populated)"]
        D["OverlayDescriptor"]
        B["render::bake_overlay_to_rgba8"]
        P["overlays/&lt;id&gt;.png<br/>+ blake3(RGBA8 bytes)"]
        W --> B
        D --> B
        B --> P
    end
    subgraph Beauty["Beauty path — artifact-only"]
        direction TB
        W2["WorldState + CameraPreset<br/>+ overlay stack"]
        SR["SkyRenderer"]
        TR["TerrainRenderer"]
        OR["OverlayRenderer"]
        OFF["Offscreen texture<br/>RENDER_ATTACHMENT | COPY_SRC"]
        PB["beauty/scene.png<br/>(not used for pass/fail)"]
        W2 --> SR --> OFF
        W2 --> TR --> OFF
        W2 --> OR --> OFF
        OFF --> PB
    end
```

The **truth path** is the hash-backed contract. Same host + same
binary + same `CaptureRequest` → same overlay RGBA8 bytes, locked
by a determinism test that runs the harness twice and diffs
`summary.ron` modulo a timing whitelist (`timestamp_utc`,
`pipeline_ms`, `bake_ms`, `gpu_render_ms`, `warnings`).

The **beauty path** goes through wgpu rasterisation + depth test
+ alpha blend + lighting + blue-noise dither, so cross-GPU fp
drift means beauty bytes are **not** part of the pass/fail
contract — `--headless-validate` only ever writes warnings for
beauty divergence. This split is load-bearing for future
cross-platform CI and the wasm target.

### AD8 fallback: beauty-skipped, truth-green

`GpuContext::new_headless((w, h))` is attempted **exactly once**
at the top of `run_request`. On failure (adapter unavailable,
driver crash at init, etc.) every `CaptureShot.beauty` is marked
`BeautyStatus::Skipped { reason }`, the truth path still runs to
completion, and `overall_status` becomes
`PassedWithBeautySkipped { skipped_shot_ids, reason }` — exit
code `0`, not a failure. Retrying adapter construction per shot
is explicitly forbidden (it would introduce nondeterminism).

### `CaptureRequest` vs `SaveMode::DebugCapture`

These are two different abstractions and share no code path:

| Abstraction | Unit | Code location | Directory |
|---|---|---|---|
| `SaveMode::*` | One file (`.ipgs`) | `crates/core/src/save.rs` (byte-level codec) + `crates/app/src/save_io.rs` (~5-line `&Path` wrapper) | User-chosen |
| `CaptureRequest` run | One directory tree | `crates/app/src/headless/` (owns all `std::fs`, `png`, `ron`) | `/captures/headless/<run_id>/` or `/crates/data/golden/headless/<baseline_id>/` |

`SaveMode::DebugCapture` serialises one `WorldState` at a point
in time. A `CaptureRequest` run records the outputs of executing
many shots. The harness may in future call into `core::save` as
a one-way dependency, but `core::save` must remain byte-level +
Path-free so the wasm target keeps working.

### Directory layout (AD4)

```
/captures/headless/<run_id>/                  # runtime outputs, gitignored
├── request.ron                               # copy of input for audit
├── summary.ron                               # top-level digest (the compare contract)
└── shots/<shot_id>/
    ├── metrics.ron                           # SummaryMetrics when include_metrics=true
    ├── overlays/<overlay_id>.png             # per-overlay truth PNG
    └── beauty/scene.png                      # beauty PNG when Rendered

/crates/data/golden/headless/<baseline_id>/   # tracked baselines
├── request.ron
├── summary.ron                               # committed expected hashes
└── shots/<shot_id>/metrics.ron               # committed expected metrics
                                              # (PNGs intentionally NOT committed)
```

The `.gitignore` carries `crates/data/golden/headless/**/*.png`
so re-running the harness into a baseline dir doesn't
accidentally commit PNG bytes. `--headless-validate` compares
the two `summary.ron` files directly — it does not read any PNG.

### AD9 `OverallStatus` public contract

The 5-variant set is **locked**: downstream shell scripts
`case $?` on the exit byte and AI agents `match` on the variant.

| Variant | Producer | Exit |
|---|---|---|
| `Passed` | run / validate | `0` |
| `PassedWithBeautySkipped { skipped_shot_ids, reason }` | run / validate | `0` |
| `FailedTruthValidation { mismatches }` | validate only | `2` |
| `FailedMetricsValidation { mismatches }` | validate only | `2` |
| `InternalError { reason, kind }` | both | `3` |

`InternalErrorKind` carries `#[serde(other)]` on `Other` so a
Sprint 4 `summary.ron` that names a new kind still parses
cleanly on a Sprint 1C binary.

### Three-step compare semantics (AD5)

`--headless-validate` applies the steps in order and returns on
the first failure:

1. **Shape guards** (hard `InternalError`):
   - `schema_version` equal
   - Shot-id sets strictly equal (`missing`, `extra` → `ShotSetMismatch`)
   - Per-shot overlay-id sets strictly equal (`OverlaySetMismatch`)
   - `request_fingerprint` divergence is a **warning only**; falls through

2. **Truth diff** (hash-based, authoritative):
   - Any `overlay_hashes[shot_id][overlay_id]` mismatch → `FailedTruthValidation`
   - If all overlays match, any `metrics_hash` mismatch → `FailedMetricsValidation`

3. **Beauty** (artifact-only, never fails):
   - Beauty `Skipped` on either side → warning + `PassedWithBeautySkipped`
   - Beauty `byte_hash` differs but both `Rendered` → warning + `Passed`
   - Beauty-spec asymmetry → warning + `Passed`

Step 1 guards prevent the "quietly compared a subset" class of
false-positive when baselines diverge structurally. Step 2's
early return on `overlay_hashes` means pixel-level drift is
reported before numeric drift, matching author intuition that
bit-drift in the raster is the stronger signal.

### Checked-in baselines

- `crates/data/golden/headless/sprint_1a_baseline/` — 9 shots
  (3 presets × 3 seeds × Hero camera). Seeds `[42, 123, 777]`
  match the pre-existing `golden_seed_regression` triples so the
  numeric and visual regressions share one set of pairs.
- `crates/data/golden/headless/sprint_1b_acceptance/` — 9 shots,
  the default-wind subset of the 16-shot
  `docs/design/sprints/sprint_1b_visual_acceptance/` PNG
  archive. The 6 wind-varying shots and the panel smoke test
  stay as manual PNGs; migrating them needs either a schema-v2
  `preset_override` field or a different harness entirely.

See [`crates/data/golden/headless/README.md`](../../crates/data/golden/headless/README.md)
for the author workflow (regenerate / validate / prune PNGs).

---

## 7. Architectural invariants

These are enforced by tests and CI, not just convention. Breaking
any of them reverts to `dev` and re-opens the sprint that broke it.

1. **`core` stays headless.** `cargo tree -p core` must never list
   `wgpu`, `winit`, `egui*`, `png`, `image`, or `tempfile`. The
   `pipeline_runs_without_graphics` test in `core::pipeline`
   enforces this at the build level.
2. **No `&Path` or `std::fs` in `core`.** The save codec is byte-
   level (`impl Write` / `impl Read`); `app::save_io` is the only
   ~5-line Path wrapper. Wasm must work without touching `core`.
3. **`WorldState` is three-layer.** Top-level fields are exactly
   `{ seed, preset, resolution, authoritative, baked, derived }`.
   Never add `Option<ScalarField2D<...>>` to the top level — put
   it under `authoritative`, `baked`, or `derived`. `derived` is
   `#[serde(skip)]`.
4. **`Resolution` is simulation-only.** `sim_width` / `sim_height`
   live on `WorldState`. Render LOD and hex cell counts live in
   their own crates and are NOT part of canonical state.
5. **No `Vec<bool>`.** Masks are `MaskField2D = ScalarField2D<u8>`
   with the `0 = false / 1 = true` convention, so GPU upload / PNG
   export / serde are contiguous byte arrays.
6. **Field abstraction is not a trait.** `ScalarField2D<T>` +
   `MaskField2D` + `VectorField2D` aliases only. No `trait Field`.
   (The internal sealed `pub(crate) trait FieldDtype` over
   `u8|u32|f32|[f32; 2]` is a private implementation detail.)
7. **Overlays are descriptors, not closures.** `OverlayRegistry`
   stores `Vec<OverlayDescriptor>`. Any "render closure" pattern
   locks the CPU-side PNG export path and must be rejected.
8. **String field keys live only in `crates/render/src/overlay.rs`.**
   `crates/sim`, `crates/core::save`, and `crates/core::validation`
   access state via struct field paths like
   `world.authoritative.height` — not by stringly-typed dispatch.
   `app::headless::executor` looks up overlays by typed
   registry (`registry.by_id(id)`), never by matching literal
   field strings itself.

---

## 8. File layout

```
Island-Proc-Gen/
├── Cargo.toml                 # workspace root
├── rust-toolchain.toml        # pin stable Rust (edition 2024, rustc ≥ 1.85)
├── shaders/                   # WGSL shaders — read via include_str!
│   ├── terrain.wgsl           # height ramp + sea blend + key/fill/ambient
│   ├── overlay.wgsl           # per-descriptor texture sample + alpha blend
│   └── sky.wgsl               # full-screen gradient
├── assets/
│   ├── visual/
│   │   └── palette_reference.jpg  # canonical 8-colour palette (pixel-locked)
│   ├── noise/
│   │   ├── LICENSE.md             # Calinou CC0 attribution
│   │   └── blue_noise_2d_{64,128,256}.png
│   └── screenshots/
│       └── hero.png           # README preview
├── crates/
│   ├── core/                  # field types, WorldState, pipeline trait, validation
│   ├── sim/                   # 17-stage canonical pipeline (16 StageId variants + terminal ValidationStage)
│   │   ├── geomorph/
│   │   ├── hydro/
│   │   ├── climate/
│   │   ├── ecology/
│   │   └── hex_projection.rs
│   ├── hex/                   # HexGrid + axis-aligned box tessellation (v1)
│   ├── data/                  # presets (RON), golden snapshots, SummaryMetrics
│   │   ├── presets/           # volcanic_single, volcanic_twin, caldera
│   │   └── golden/snapshots/  # per-seed regression snapshots
│   ├── gpu/                   # wgpu device/surface/depth management
│   ├── render/                # terrain + overlay + sky pipelines, palette, noise
│   ├── ui/                    # egui panels (overlay, params, stats)
│   ├── app/                   # winit event loop, Runtime, save_io Path wrapper
│   │   └── src/headless/      # --headless harness: request, executor, output, compare
│   └── data/
│       └── golden/
│           ├── snapshots/     # seed-keyed SummaryMetrics RON (per-seed)
│           └── headless/      # --headless-validate baselines (request/summary/metrics)
│               ├── README.md
│               ├── sprint_1a_baseline/
│               └── sprint_1b_acceptance/
├── docs/
│   ├── architecture/          # this directory — tracked architecture docs
│   └── papers/                # tracked paper knowledge base (Core Pack + sprint packs)
├── CLAUDE.md                  # agent operating notes (project-scoped)
├── CLAUDE.local.md            # agent operating notes (per-user, gitignored)
├── PROGRESS.md                # sprint dashboard + history
└── README.md                  # public-facing project readme
```

---

## 9. Versioning + compatibility

- **Rust toolchain:** stable, edition 2024, `rustc >= 1.85`
  (pinned by `rust-toolchain.toml`).
- **Graphics stack pins:** `egui` / `egui-wgpu` / `egui-winit` at
  `0.34.1`, `wgpu` at `29.0.1`, `winit` at `0.30.13`. Don't mix
  versions without verifying the egui / wgpu compatibility matrix.
- **CI gate:**
  `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace`
  runs on macOS (Metal backend available). Two additional
  `continue-on-error: true` steps regenerate the Sprint 1A +
  Sprint 1B headless baselines and self-validate them — these
  are non-blocking on first landing; Sprint 2+ can promote them
  to hard-failing once Metal on the CI runner proves stable.
  `app` / `render` / `gpu` tests that need a GPU device are
  excluded from the default workspace test run.
- **Target platforms:** macOS-first development
  (AD10 baseline acceptance host = Apple Silicon + Metal); the
  architecture stays platform-agnostic so a future wasm build
  can reuse `core`, `sim`, `hex`, and `data` unchanged. Beauty
  capture on non-Metal hosts falls through the AD8 Skipped path
  rather than failing.

---

## 10. What this document deliberately omits

- Per-stage algorithmic details (lapse rates, Budyko ω, suitability
  bell curves). Those live in the sprint docs under `docs/design/`
  (gitignored Obsidian vault) and in inline module docs in
  `crates/sim/src/`.
- Paper references and the research framing. See
  [`docs/papers/README.md`](../papers/README.md) for the indexed
  knowledge base.
- Sprint timelines and acceptance-checklist status. See
  [`PROGRESS.md`](../../PROGRESS.md).
- Operating instructions for AI coding agents. See
  [`CLAUDE.md`](../../CLAUDE.md).
