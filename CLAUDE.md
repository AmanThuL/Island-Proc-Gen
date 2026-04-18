# CLAUDE.md

Project-scoped context for Claude Code (and any other agent harness)
working in this repository. Read this before proposing or executing
changes.

A sibling [`CLAUDE.local.md`](CLAUDE.local.md) is **gitignored** and
carries per-user preferences (subagent workflow cadence, model
selection for subagents, personal consent gates). If both files
exist on a given checkout, CLAUDE.md wins for project-level rules;
CLAUDE.local.md only shapes _how the user wants to collaborate_ on
top of those rules.

---

## Role

Pair programmer on a single-developer Rust research project. Default stance:
help build, catch mistakes, push back when an idea drifts from the active
sprint's stated scope. Prefer small atomic commits over big bundled ones.
Ask before anything irreversible — force push, dep downgrades, renaming a
workspace crate, rewriting `WorldState` layout, deleting generated
artifacts. See `CLAUDE.local.md` for the full consent-gate list.

---

## Key files

| File | Purpose |
|------|---------|
| [`PROGRESS.md`](PROGRESS.md) | Sprint-level dashboard — what's shipped, what's next, what's blocked |
| [`docs/design/island_generation_complete_roadmap.md`](docs/design/island_generation_complete_roadmap.md) | Authoritative roadmap and architectural rules |
| [`docs/design/sprints/sprint_N_*.md`](docs/design/sprints/) | The active sprint's implementation plan and §6 acceptance checklist |
| [`docs/papers/README.md`](docs/papers/README.md) | Paper knowledge base layering (A Core Pack / B Sprint Packs / C Case Studies / D Parking Lot) |
| [`crates/data/golden/headless/README.md`](crates/data/golden/headless/README.md) | `--headless-validate` baselines (1A 9-shot + 1B 9-shot + Sprint 2 erosion 6-shot) |

**Read the active sprint doc before touching code for that sprint.** The
sprint's §6 acceptance checklist and §7 risks/invariants are the done-definition
— not generic Rust best practices.

---

## AI-native validation workflow (Sprint 1C)

You can validate the full pipeline + overlay + beauty-render stack without
opening a window. Two entry points on the main `app` binary:

```bash
# Run a CaptureRequest — writes the artifact tree (runtime outputs go under
# /captures/ which is gitignored; baselines under crates/data/golden/headless/
# are tracked and re-run idempotently thanks to deterministic run_id).
cargo run -p app --release -- --headless <path/to/request.ron>

# Diff a runtime capture against a checked-in baseline — pure summary.ron
# comparison, no PNG reads required.
cargo run -p app --release -- \
    --headless-validate /captures/headless/<run_id>/ \
    --against crates/data/golden/headless/sprint_1a_baseline/
```

**Exit-code contract (AD9, locked by `main_bin::tests::headless_exit_byte_maps_overall_status_to_ad9_code`):**

| Code | `OverallStatus` variant | Meaning |
|------|-------------------------|---------|
| `0`  | `Passed` / `PassedWithBeautySkipped` | Truth path green; beauty may be Rendered or legitimately skipped |
| `2`  | `FailedTruthValidation` / `FailedMetricsValidation` | Pipeline regression — overlay bytes or SummaryMetrics drifted |
| `3`  | `InternalError` | Tool-level error (IO, RON parse, shot-set mismatch) — fix the harness, not the pipeline |

Shell scripts `case $?` this directly; AI agents `match` on
`summary.ron.overall_status` without string scraping. **Always check exit
code or read `summary.ron.overall_status` — don't assume success just
because the command returned.**

Three checked-in baselines live under `crates/data/golden/headless/`:
- `sprint_1a_baseline/` — 3 presets × 3 golden seeds × Hero camera = 9 shots
- `sprint_1b_acceptance/` — migration of the default-wind subset of the
  Sprint 1B 16-shot visual acceptance (9 shots). The wind-varying shots
  remain as manual PNGs in `docs/design/sprints/sprint_1b_visual_acceptance/`;
  Sprint 2.5 is the natural slot to migrate them (schema-v2 `preset_override`
  from Sprint 2 now unblocks it, but the Sprint 2.5 scope hasn't absorbed
  them yet).
- `sprint_2_erosion/` — 3 presets × pre/post erosion at seed 42 = 6 shots.
  `pre_*` uses `preset_override.erosion.n_batch = 0` → `ErosionOuterLoop`
  noop; `post_*` runs the locked 10×10 outer loop. Sprint 2 Task 2.6.

---

## Architectural invariants (hard rules — do not weaken without flagging)

These are enforced by tests and CI, not just convention. Breaking any of them
reverts to `dev` and re-opens the sprint.

1. **`core` stays headless.** `cargo tree -p core` must never list `wgpu`,
   `winit`, `egui*`, `png`, `image`, or `tempfile`. The
   `pipeline_runs_without_graphics` test in `crates/core/src/pipeline.rs`
   enforces this at the test level.
2. **No `&Path` or `std::fs` in `core`.** The save codec is byte-level
   (`impl Write` / `impl Read`); `app::save_io` is the only ~5-line Path
   wrapper. Wasm target must work without touching `core`.
3. **`WorldState` is three-layer.** Top-level fields are exactly
   `{ seed, preset, resolution, authoritative, baked, derived }`. Never add
   `Option<ScalarField2D<...>>` to the top level — put it under `authoritative`
   / `baked` / `derived`. `derived` is `#[serde(skip)]`.
4. **`Resolution` is simulation-only.** `sim_width` / `sim_height` live on
   `WorldState`. Render LOD and hex columns/rows live in their own crates and
   are NOT part of canonical state.
5. **No `Vec<bool>`.** Masks are `MaskField2D = ScalarField2D<u8>` with the
   `0 = false / 1 = true` convention, so GPU upload / PNG export / serde are
   contiguous byte arrays.
6. **Field abstraction is not a trait.** `ScalarField2D<T>` + `MaskField2D` +
   `VectorField2D` aliases only. If you catch yourself writing `trait Field`,
   stop. (The `pub(crate) trait FieldDtype` used internally to seal
   `to_bytes` / `from_bytes` over `u8|u32|f32|[f32; 2]` is OK — it's private.)
7. **Overlays are descriptors, not closures.** `OverlayRegistry` stores
   `Vec<OverlayDescriptor>`. Any "render closure" pattern locks Sprint 4's
   CPU-side PNG export path and must be rejected.
8. **String field keys only in `crates/render/src/overlay.rs`.** `crates/sim`,
   `crates/core::save` (error-message payloads aside), and
   `crates/core::validation` access state via struct field paths like
   `world.authoritative.height` — not by stringly-typed dispatch.

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

- **The `core` crate name shadows stdlib `::core`.** Downstream crates (`app`,
  `data`, `ui`) import it as
  `island_core = { path = "../core", package = "core" }`. Apply the same
  pattern when adding a new crate that depends on `core`.
- **`crates/core/Cargo.toml` has `[lib] doctest = false`.** Same shadowing
  issue: rustdoc runs `--crate-name core`, and `thiserror`'s derive expands
  `::core::fmt` paths that can't resolve inside the user crate. Don't remove
  it until a future sprint decides whether to rename `core` → `ipg-core`.
- **`ScalarField2D<T>` field payloads are NOT serde-serialized inside
  `WorldState`.** `authoritative.height` and `authoritative.sediment` are
  individually `#[serde(skip)]`; the save codec writes them via
  `ScalarField2D::to_bytes()` rather than serde, to keep the `IPGF` byte format
  under our control and avoid double-encoding.
- **Bash invocations don't auto-source cargo env on this machine.** Prefix
  commands with `. "$HOME/.cargo/env" && <command>`. `~/.bashrc` has the
  source line, but non-interactive bash skips it.
- **Version pins (locked in Sprint 0).** `egui` / `egui-wgpu` / `egui-winit` at
  `0.34.1`; `wgpu` `29.0.1`; `winit` `0.30.13`. Winit 0.30 uses the
  `ApplicationHandler` trait pattern, not the legacy `EventLoop::run` closure.
  Don't mix versions without verifying the egui / wgpu compatibility matrix.
- **`FLOW_DIR_SINK` is `0xFF`, not `0`.** `0` is already the `E` direction in
  the D8 encoding (`D8_OFFSETS[0] = (1, 0)`). The sprint doc originally wrote
  the sink sentinel as `0`, but that collides with east-flowing cells. Every
  Sprint 1A hydro stage reads the sentinel via
  `use island_core::world::{D8_OFFSETS, FLOW_DIR_SINK}` — never hardcode
  either. The constants live in `core::world` (not `sim::hydro`) so
  `core::validation` can reference them without a reverse dep edge.
- **Post-pit-fill sinks are NOT exactly `{ p : flow_dir[p] == FLOW_DIR_SINK }`.**
  `CoastMaskStage` uses Von4 for `is_coast`, while `FlowRoutingStage` picks
  downstream neighbours from the Moore8 set. A land cell with only a
  *diagonal* sea neighbour is therefore not classified as coast, yet its D8
  downstream is still that sea cell. For BasinsStage and river termination
  validation, "sink" must include "land cell whose D8 downstream is sea or
  out-of-bounds". `sim::hydro::basins.rs` encodes this as the extended sink
  definition.
- **`RiverExtractionStage` must gate candidates on `is_land`.** Because of
  the same diagonal Moore8 edge case above, sea cells can legitimately
  accumulate upstream flow (via `AccumulationStage` propagation from land to
  the diagonal sea neighbour) and cross the river threshold. Without the
  land gate, those sea cells get flagged as "rivers" and `ValidationStage`
  fires `RiverInSea`. The full Sprint 1A pipeline test in
  `sim::validation_stage::tests` catches this regression immediately.
- **§D5 `coastal_falloff` formula in the sprint doc is written backwards.**
  The prose says "让 z 在 island_radius 以外平滑跌到 sea_level 以下" but
  the literal formula `amplitude * (1 - smoothstep(0.9r, r, dist))` evaluates
  to `amplitude` *inside* the island and `0` *outside*, which is the opposite
  direction. The implementation uses the corrected
  `amplitude * smoothstep(0.9r, r, dist)` (0 inside the island, amplitude at
  the rim) — see the inline comment in
  `crates/sim/src/geomorph/topography.rs::build_coastal_falloff`.
- **`cargo clippy --workspace -- -D warnings`** (no `--all-targets`) is the
  hard CI gate — matches Sprint 0 CI config. `--all-targets` surfaces
  pre-existing `approx_constant` lints in `crates/data/src/presets.rs` unit
  tests (`1.5708` literals) that can't be replaced with `FRAC_PI_2` as a
  one-liner because the RON presets use `1.5708` and `assert_eq!` needs bit
  equality. Tracked as a Sprint 2+ cleanup task.
- **`docs/design` is a gitignored symlink** into the author's Obsidian vault.
  The sprint doc at `docs/design/sprints/sprint_1a_terrain_water.md` is
  therefore NOT tracked in git — local edits to it persist on disk but do
  not land in commits. Spec clarifications discovered during implementation
  (e.g. the §D6 `FLOW_DIR_SINK` sentinel) must be mirrored in the commit
  message and in CLAUDE.md / PROGRESS.md so they survive outside the
  author's machine.
- **The canonical 8-colour palette is pixel-locked to
  `assets/visual/palette_reference.jpg`,** not to the hex table in the
  sprint doc. `canonical_constants_match_palette_reference` in
  `crates/render/src/palette.rs` fires on any drift. Change a constant
  only after re-sampling the reference image with a ΔE < 6 tolerance
  in sRGB; smaller drifts are JPEG noise, larger drifts mean the
  reference image was intentionally re-generated and the code constant
  should update to match.
- **CLAUDE.md invariant #8 (no hardcoded colours outside `palette.rs`)
  applies to `shaders/*.wgsl` as well.** Sprint 1A's `terrain.wgsl`
  threads all 8 canonical colours through a `Palette` uniform buffer
  at `@group(0) @binding(1)`; WGSL `vec3<f32>(0.xx, 0.xx, 0.xx)` /
  `vec4<f32>(0.xx, ...)` style colour literals are forbidden. The
  `terrain_wgsl_has_no_literal_colors` test enforces this mechanically.
- **Calinou blue-noise files are 8-bit RGBA,** not grayscale, with L
  replicated across R=G=B in the `LDR_LLL1_*` variant. `noise::try_load_png`
  accepts Grayscale/RGB/RGBA and strips to the R channel to recover the
  luminance sample — keep that behaviour when porting the loader.
- **`render::camera` preset module ≠ `app::camera` orbit camera.** They
  coexist: the orbit camera is the interactive winit-event consumer in
  `Runtime`; the preset module is stateless LUT math for the
  Hero/TopDebug/LowOblique dropdown. Don't try to unify them —
  Runtime owns interaction, render owns capture geometry.
- **`naga` is a dev-dep on `render`, never a runtime dep.** The
  `terrain_wgsl_parses_successfully` test uses
  `naga::front::wgsl::parse_str` + `naga::valid::Validator` for
  headless shader validation, but the shader module is loaded at
  runtime via wgpu's internal naga. Keep naga in
  `[dev-dependencies]` and pin the version to the one wgpu pulls
  transitively (currently `29.0.1`) so the two parsers never disagree.
- **`StageId` is the single source of truth for pipeline indices.**
  Every `run_from` caller (app::Runtime slider handler, golden
  regen, tests) passes `StageId::X as usize` — never a literal
  index. The 18-variant enum in `crates/sim/src/lib.rs` is locked by
  `stage_id_indices_are_dense_and_canonical`; reordering it requires
  auditing every consumer in lockstep. `ValidationStage` is
  intentionally NOT a variant (it's a tail hook, not a slider
  target).
- **Slider re-run protocol: sync `world.preset` BEFORE `run_from`.**
  Sprint 1B sliders mutate the runtime's `self.preset`; stages read
  parameters from `world.preset`. `Runtime::tick` must
  `self.world.preset = self.preset.clone();` before calling
  `pipeline.run_from(&mut self.world, StageId::X as usize)`.
  Forgetting the sync means the slider changes are silently
  ignored by every stage — a class of bug that produces
  "identical before/after" screenshots without any panic or test
  failure. New sliders follow the same pattern.
- **`BiomeWeightsStage` writes two fields simultaneously.**
  `baked.biome_weights` holds the rich per-biome partition-of-unity
  weights; `derived.dominant_biome_per_cell` is a `ScalarField2D<u32>`
  argmax sidecar that the overlay path renders through the same
  `ScalarDerived` resolver as `basin_id`. Both are written on every
  run; the sidecar exists so the overlay doesn't recompute argmax
  every frame. If you touch the biome stage, update both — or the
  overlay will silently render stale data.
- **The `dominant_biome` overlay is not a reliable probe for wind-
  propagation.** At v1 params, only ~3 % of land cells flip biome
  argmax under a 180° wind swing (`volcanic_single`, 256², max
  soil_moisture delta 0.23). The 8-biome categorical argmax is
  dominated by wind-invariant inputs (z_norm, slope, river_mask).
  The pipeline-level guard is
  `sim::validation_stage::tests::wind_dir_rerun_propagates_through_biome_chain`
  — it asserts `precipitation / fog_likelihood / soil_moisture /
  biome_weights / dominant_biome_per_cell` all mutate on
  `run_from(Precipitation)`, which is the real contract. The Sprint
  1B §10 visual acceptance Pass 3 uses `soil_moisture` (wind-
  sensitive by construction), not `dominant_biome`, for the 60↔61
  screenshot pair. When adding future wind-dependent overlays, pick
  a field whose raw value range is wind-sensitive rather than a
  categorical argmax.
- **`GpuContext::surface` is `Option<wgpu::Surface<'static>>`** so the
  same type serves both the windowed Runtime and the `--headless`
  offscreen harness. Interactive code paths call
  `gpu.surface_expect()` which panics with a descriptive message
  rather than unwrapping; headless construction via
  `GpuContext::new_headless((w, h))` sets `surface = None` and picks
  `surface_format = HEADLESS_COLOR_FORMAT = Rgba8Unorm`. Renderers
  key off `gpu.surface_format` / `gpu.depth_format` — unchanged
  fields — so the same `TerrainRenderer` / `SkyRenderer` /
  `OverlayRenderer` code plugs into both paths.
- **AD8 GPU bootstrap is top-level, NOT per-shot.**
  `app::headless::executor::run_request` calls
  `GpuContext::new_headless(...)` exactly once at the top. On
  failure all `BeautySpec` shots are marked
  `BeautyStatus::Skipped`, truth path runs to completion, and
  `OverallStatus::PassedWithBeautySkipped` keeps exit code 0.
  Re-trying adapter construction per shot would introduce
  non-determinism; don't do it.
- **`CaptureRequest` vs `SaveMode::DebugCapture` are different
  abstractions.** `core::save` stays byte-level (no Path, no PNG, no
  `std::fs`) so the wasm target still works; `app::headless` owns
  all the filesystem + PNG + RON write code. The harness can in
  principle call `core::save` one-way, but the two must never
  share code paths — keeping them separate preserves invariants #1
  and #2.
- **Truth path is deterministic (AD7); beauty path is artifact-only
  (AD2 + AD7).** Same host + same binary + same `CaptureRequest`
  → `summary.ron` is bit-exact modulo the explicit whitelist
  (`timestamp_utc`, `pipeline_ms`, `bake_ms`, `gpu_render_ms`,
  `warnings`). Beauty `byte_hash` is bit-exact on the same host
  per AD7, but cross-GPU fp drift means it's NEVER used for
  pass/fail — `--headless-validate` Step 3 only ever writes
  warnings for beauty divergence / skip. The compare tool works
  off two `summary.ron` files and does not read any PNG.
- **`metrics_hash` is `Option<String>`.** `None` means the shot
  explicitly opted out of `include_metrics`; two `None`s compare
  equal, a mixed `Some`/`None` is a mismatch. If you add a shot
  with `include_metrics: false` to a baseline, expect no
  `metrics.ron` file on disk for that shot.
- **`AD9 OverallStatus` 5-variant set is locked.** Adding a new
  variant breaks downstream shell scripts and CI expectations.
  The exit-code map (`main_bin::tests::headless_exit_byte_maps_
  overall_status_to_ad9_code`) is frozen at 0 / 2 / 3. Sprint 4
  may add variants only additively per AD9 "扩展规则".
  `InternalErrorKind::Other` carries `#[serde(other)]` so
  Sprint 4's new kinds parse cleanly on a 1C binary.
- **`sim::default_pipeline()`** is the single source of truth for
  the 19-stage canonical pipeline (18 `StageId` variants + terminal
  `ValidationStage`). Both `crates/data/tests/golden_seed_regression.rs` and
  `app::headless::executor` consume it. If you add a stage,
  update `default_pipeline` and bump the `StageId` enum in
  lockstep (the `stage_id_indices_are_dense_and_canonical` test
  fires on drift).
- **Never commit overlay / beauty PNGs under
  `crates/data/golden/headless/`.** The `.gitignore` carries
  `crates/data/golden/headless/**/*.png`, but re-running
  `cargo run -p app -- --headless <baseline>/request.ron`
  produces them in place. Delete before `git add -A` or use the
  one-liner in `crates/data/golden/headless/README.md`.
- **`authoritative.height` is mutable from Sprint 2 onward.** Sprint
  1A / 1B wrote it once at `TopographyStage` and treated it as
  read-only thereafter; Sprint 2's `StreamPowerIncisionStage` +
  `HillslopeDiffusionStage` (called inside `ErosionOuterLoop`)
  rewrite it in place every inner iteration. When you mutate
  `authoritative.height` from ANY code path, follow the default
  invalidation protocol: `invalidate_from(world, StageId::Coastal)`
  before re-running downstream, because height mutation may cross
  the sea_level threshold and make `coast_mask` stale.
- **`derived.erosion_baseline` is sticky across slider reruns.**
  `ErosionOuterLoop::run` snapshots `{max_height_pre,
  land_cell_count_pre}` on its first invocation (gated by
  `is_none()`). Slider-triggered reruns via
  `run_from(StageId::ErosionOuterLoop)` do NOT re-snapshot — the
  baseline stays the pre-first-erosion reference so
  `erosion_no_explosion` / `erosion_no_excessive_sea_crossing`
  compare against the true pre-erosion state, not a
  moving-post-erosion state. `invalidate_from(Topography)` is the
  only legitimate reset; the `ErosionOuterLoop` arm of
  `clear_stage_outputs` is intentionally a noop so
  `invalidate_from(Coastal)` mid-batch can't clobber the baseline.
- **SPIM `K` is grid-size-dependent. `K=1.5e-3` is the locked
  default.** Sprint 2.6 empirical Pareto probe showed `K=2e-3`
  passes on 128² but fails synthetic 64² tests (volcanic_single
  sea-crossing tips to 5.09 %, above the 5 % invariant). Any K
  bump must verify on ALL grid sizes used by the test suite
  (64² / 128² / 256²) before landing. Sprint doc DD1's "~18 %
  max_z drop" projection is physically incompatible with the
  `erosion_no_excessive_sea_crossing` 5 % invariant under uniform
  SPIM: reaching 18 % peak drop (A≈1, S≈0.01) requires K ≈ 0.18,
  which scales coastal erosion ~180× and shatters the invariant
  by orders of magnitude. Larger peak erosion is a Sprint 3
  sediment-aware (`K·g(hs)`) problem, not a K-tuning problem.
- **CoastType thresholds tuned from spec's v1 values.** Sprint 2.6
  observed that at the safe K calibration, max coastal slope
  rarely exceeds 0.07 on the three stock presets, so the spec's
  v1 `S_CLIFF_HIGH=0.30` / `S_CLIFF_MID=0.18` / `S_BEACH_LOW=0.05`
  / `EXPOSURE_HIGH=0.30` put 100 % of cells in the Beach bin. The
  locked constants in `crates/sim/src/geomorph/coast_type.rs` are
  now `0.07 / 0.04 / 0.02 / 0.05`, which populates Beach + Rocky +
  Estuary but still gives 0 Cliffs across all three presets. Cliff
  bins lighting up is a Sprint 3 terrain-geometry problem (or a
  coast_type v2 classifier with fetch-integral wave exposure).
- **`Runtime` uses `sim::default_pipeline()`; no local pipeline
  builders in `app` or `ui`.** Sprint 1A / 1B had a hand-rolled
  `build_sprint_1b_pipeline()` in `crates/app/src/runtime.rs` that
  silently drifted out of lockstep with `StageId` when Sprint 2.3 /
  2.4 inserted two new variants — every slider run_from resolved
  to the wrong stage. `sim::default_pipeline()` is the single
  source of truth; if you need a pipeline variant for a specific
  test (e.g. non-eroding for bit-exact invalidation round-trips),
  define it `#[cfg(test)]` inside `crates/sim/` next to the test
  that needs it, not in a downstream crate.
- **`OverlayRegistry::sprint_2_defaults()` returns 13 descriptors
  (1B 12 + Sprint 2's `coast_type`).** Do NOT add
  `sprint_Na_defaults()` alias methods per sprint — CLAUDE.md
  forbids backwards-compat shims. When a new sprint adds an
  overlay, rename `sprint_N_defaults` to the latest sprint's
  number and update every call site in the same commit.
- **`coast_type` overlay normalises via `ValueRange::Fixed(0.0,
  4.0)`, not `(0.0, 3.0)`.** With `Fixed(0.0, 3.0)` the
  RockyHeadland discriminant (3) would map to `t = 1.0`,
  `idx = 4`, out of range → transparent — silently hiding the
  majority of coast cells. `Fixed(0.0, 4.0)` gives `t = disc/4`
  and `idx = disc` exactly for 0..=3; the 0xFF Unknown sentinel
  clamps to `idx = 4` → transparent as intended. Regression
  guards in `crates/render/src/palette.rs` tests lock this.
- **`RunSummary.schema_version` mirrors the input
  `CaptureRequest.schema_version`, not the tool version.** v1
  request files (Sprint 1C baselines) running under a Sprint 2+
  v2 binary produce v1-stamped summaries that match the
  checked-in baseline summary — the v1 baselines continue to
  `--headless-validate` exit 0 under v2 and beyond. Do NOT stamp
  a "current tool version" into `RunSummary.schema_version`; that
  breaks the forward-compat contract.

---

## Commit style

- **Conventional commits:** `feat(scope): ...`, `fix(scope): ...`, `refactor: ...`,
  `docs: ...`, `ci: ...`, `chore: ...`. Scope is crate name(s) for code changes
  (`feat(core,app): ...`) or omitted for workspace-wide refactors.
- **One task per commit.** Sprint-level work is bundled across multiple commits,
  not one giant commit — makes bisection and rollback tractable.
- **No `Co-Authored-By: Claude ...` footer** — attribution is disabled globally
  at the user level.
- Don't amend commits that are already on `dev` or `main`. Create a new commit.
- Don't bypass `--no-verify` or `--no-gpg-sign` without asking.

---

## Rules for this session

1. The active sprint doc's §6 acceptance checklist is the done-definition.
   Features beyond it are out of scope unless the user explicitly asks.
2. Never add a dep to `core` that breaks `cargo tree -p core` cleanliness
   (no `wgpu`, `winit`, `egui*`, `png`, `image`, `tempfile`, `naga` — ever).
3. If a subagent's plan would violate any architectural invariant above,
   stop and flag it — don't let it slide.
4. Subagent workflow cadence + model selection + consent gates live in
   [`CLAUDE.local.md`](CLAUDE.local.md) (gitignored, per-user).

---

## Session start protocol

1. Read `PROGRESS.md` for current sprint state.
2. Read the active sprint file in `docs/design/sprints/` for acceptance criteria.
3. Run `git status` and `git log --oneline -10` to see where the branch is.
4. If a Rust change is planned, verify `cargo check --workspace` is green
   before starting:
   ```bash
   . "$HOME/.cargo/env" && cargo check --workspace
   ```
5. Surface any architectural invariant the planned work would touch, and
   confirm the plan preserves it.

---

## Notes

- The app is macOS-first (Metal backend), but the architecture stays
  platform-agnostic — Sprint 5 targets wasm. Don't pull in
  `#[cfg(target_os = "macos")]` in `core`, ever.
- The paper knowledge base under `docs/papers/` is intentionally shallow for
  most papers (frontmatter + abstract + one-sentence purpose). Chen 2014 and
  Temme 2017 are the only ones with substantive 落地点 sections as of Sprint 0.
  Sprint 1A will fill more during its first-read phase.
