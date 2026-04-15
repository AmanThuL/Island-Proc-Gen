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

**Primary:** Sprint 1A — all 4 §3.2 visual-polish passes shipped on
`dev` plus one post-validation fix. `cargo run -p app` now renders
terrain + sea + §3.2 A3 sky + §3.2 A5 GPU overlay render path +
§3.2 A6 camera preset dropdown + §3.2 B3 blue noise dither, all
against the user-approved orbit camera default. **188 tests passing**.
16 of 16 validation screenshots captured to
`docs/design/sprints/sprint_1a_visual_acceptance/`; final audit
verdict **16 / 16 PASS** after the Pass 3.1 preset distance-factor
fix (`PRESET_HERO` 1.6→5.0 and `PRESET_TOP_DEBUG` 1.4→3.5 — the
pre-fix values put the orbit camera embedded inside the volcano's
vertical extent) and a manual re-audit of shot 21 (the "diamond-
faceted pyramid" a subagent flagged is the `ridge_field` output
viewed from above — correct geometry, not a regression).

Sprint 1A §3.2 Visual Package checklist status: A1 ✓ A2 ✓ A3 ✓ A4 ✓
A5 ✓ A6 ✓ B3 ✓. See [RECENTLY SHIPPED](#recently-shipped) for the
per-commit breakdown and [DEFERRED TO LATER SPRINTS](#deferred-to-later-sprints)
for the 2 visual-audit items that are punted rather than force-fit
into 1A.

**Remaining for Sprint 1A §7 full acceptance:**
- **9 golden-baseline screenshots:** 3 camera presets × 3 golden seeds
  in `docs/design/sprints/sprint_1a_visual_acceptance/` as the Sprint
  1B regression baseline. Blocked on a seed-cycling runtime flag or UI
  that Sprint 1A doesn't ship — carried forward to Sprint 1B.
- **Paper pack (non-blocking per §7):** Chen 2014 / Génevaux 2013 deep
  reads; Lague 2014 target-deep; background papers can stay at
  `metadata_only`.

---

## RECENTLY SHIPPED

Sprint 1A §3.2 visual-polish rollup shipped in sequence on `dev`,
2026-04-14 session. All 4 passes used the simplifier → superpowers
code-reviewer → commit cadence (per auto-memory
`feedback_commit_review_workflow.md`) except Pass 3.1, which was a
2-constant post-validation fix that skipped the cadence with user
approval.

| Commit | Pass | Spec | Tests |
|---|---|---|---|
| `ac0368d` | Pass 2 — GPU overlay render path | Task 1A.10 / §3.2 A5 | 180 → 185 (+5) |
| `442aabe` | Pass 3 — Camera preset dropdown | §3.2 A6 | 185 → 188 (+3) |
| `4b230ed` | Pass 4 — Blue noise dither | §3.2 B3 | 188 (+0) |
| `071c14a` | Pass 3.1 — preset `distance_factor` fix | §3.2 A6 post-validation | 188 (+0) |

**Validation screenshot audit (16 shots captured to
`docs/design/sprints/sprint_1a_visual_acceptance/`):** 4 parallel
subagents inspected all 16 shots against the INDEX.md spec. Verdict:
12 PASS, 2 CONCERN, 1 FAIL, 2 UNVERIFIABLE.

Pass 3.1 was triggered by shot `21_preset_top_debug.png` (FAIL, showed
a mid-perspective "isometric cube" instead of near-overhead) and shot
`20_preset_hero.png` (CONCERN, volcano peak pushed off the top of the
frame). Root cause: the `volcanic_single` preset's normalized
heightfield reaches y ≈ 0.8–1.0, but the pre-fix `distance_factor`
values put the orbit camera's `eye.y` BELOW the peak:

| Preset | old factor | d = f × r | eye.y = d · sin(pitch) | Verdict |
|---|---|---|---|---|
| Hero | 1.6 | 0.8 | 0.4 | embedded; peak off-frame |
| TopDebug | 1.4 | 0.7 | ~0.699 | below peak; weird perspective |
| LowOblique | 2.0 | 1.0 | 0.217 | PASSED validation (intentional low-angle) |

Fix bumps Hero to 5.0× and TopDebug to 3.5× so `eye.y` clears the
peak with ~0.25 / ~0.75 headroom respectively. Framing verified
analytically for both presets against the orbit camera's 45° FOV —
peak stays within the 22.5° half-FOV with margin. LowOblique stays
at 2.0× unchanged (validation PASSED). `cargo test --workspace`
still reports 188 passed because the Pass 3 tests reference the
constant by symbol, not by literal.

**Resolved:** shots 20 (Hero) and 21 (TopDebug) were reshot after
`071c14a` and the framing is confirmed fixed — final 16-shot verdict
is **16 / 16 PASS**. Shot 22 (LowOblique), shot 23 (hero-then-orbit),
and shot 24 (reset view) did not need reshoots — 22 was PASS, 23
automatically picks up the new Hero distance via `apply_preset`, and
24 returns to `INITIAL_CAMERA_*` in `runtime.rs` which is unaffected.

---

## DEFERRED TO LATER SPRINTS

Two items from the Pass 4 / flow-accumulation validation audit are
punted rather than force-fit into Sprint 1A.

**Deferred to Sprint 2 — Geomorph credibility:**

- **Shot 13 — Flow accumulation overlay log-compression audit.**
  Subagent validation noted that secondary tributaries may be washed
  out in the Turbo + LogCompressed bake — only the dominant channel
  shows red. Could be correct for the `volcanic_single` preset (one
  volcano → one drainage spine) or could indicate the log compression
  constants are too aggressive. Sprint 2 (stream power erosion)
  exercises the accumulation distribution in anger and is the
  natural place to validate / tighten the bake.
- **Shots 30, 31 — Blue noise dither A/B visual validation.**
  ±½ LSB amplitude is below screenshot-inspection threshold. The
  reliable test is a pixel-diff between dither-ON / dither-OFF
  captures taken with the shader temporarily edited. Sprint 2 will
  touch terrain shading for erosion / sediment visualisation, at
  which point a shader feature-flag mechanism can ship and the A/B
  test becomes cheap.

**Deferred to Sprint 1B — Climate + Ecology (UI-dependent):**

- **9-shot golden visual baseline.** Sprint 1A doc §7 calls for 3
  camera presets × 3 golden seeds = 9 captures as the regression
  baseline. Blocked on a seed-cycling runtime flag or UI that Sprint
  1A doesn't ship (seed is a startup constant in `runtime.rs`).
  Sprint 1B adds climate parameter tweaking; a `--seed N` flag can
  land alongside, and the 9-shot set can be captured cheaply.
- **Per-descriptor alpha slider for overlays.** Pass 2 hardcodes
  `alpha = 0.6` across all 6 overlay bind groups. Sprint 1B's
  climate UI can surface a per-overlay alpha + visibility panel at
  near-zero cost (uniform update per frame).
- **Blue noise runtime size toggle (64 / 128 / 256).** Pass 4 hardcodes
  the 64×64 shipping default; the 128 / 256 PNGs are on disk
  (`assets/noise/blue_noise_2d_{128,256}.png`) but unused. Sprint 1B
  UI work can expose a 3-way size switcher via a texture swap.

---

## DEVELOPMENT

### Sprint 1A — Terrain + Water Skeleton
**Status:** §3.2 Visual Package complete (A1–A6 + B3 all shipped on
`dev` as of 2026-04-14). 16-shot validation captured + audited; Pass
3.1 post-fix landed for preset framing. Only the 9-shot golden
baseline remains for §7 full acceptance, and it's deferred to Sprint
1B because seed-cycling UI isn't a Sprint 1A deliverable.
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
