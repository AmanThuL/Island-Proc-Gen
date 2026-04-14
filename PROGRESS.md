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

**Primary:** Sprint 1A — Task 1A.9 window integration shipped. `cargo run
-p app` now renders the real Sprint 1A terrain mesh + sea quad + §3.2 A3
procedural sky gradient against the user-approved orbit camera default.
Three more visual-polish passes queued (overlay render path, camera
preset dropdown, blue noise dithering), then the 9-screenshot visual
baseline captures. See [NEXT SESSION PLAN](#next-session-plan) below.

All Sprint 1A code that can be unit-tested headlessly is in place:
8 sim stages → 4 validation invariants → `sim::ValidationStage` at the
pipeline tail → 3 golden-seed regression snapshots → canonical
8-colour palette locked against `assets/visual/palette_reference.jpg` +
non-canonical `SKY_HORIZON`/`SKY_ZENITH` → Viridis / Turbo / Categorical
/ TerrainHeight / BinaryBlue LUTs → Hero / TopDebug / LowOblique camera
presets (library only — dropdown still pending) → blue-noise loader +
3 Calinou CC0 textures (loaded but not sampled in shader yet) →
`build_terrain_mesh` / `build_sea_quad` mesh builders wired into
`TerrainRenderer` with 3-uniform-buffer pipeline, depth attachment,
sky-then-terrain draw order → `shaders/{terrain,sky}.wgsl` both naga-
validated headlessly → 6 real `OverlayDescriptor`s pointing at the
correct `derived.*` fields (registry only — render path still pending).

**Remaining for Sprint 1A §6 full acceptance:**
- **Pass 2 — Task 1A.10 overlay render path:** `render_overlay_to_gpu`
  helper per visible descriptor, CPU-side RGBA8 bake via `palette::sample`,
  upload to 2D texture, alpha-blend over terrain in the same pass.
- **Pass 3 — Camera preset dropdown (§3.2 A6):** wire `render::camera::
  {PRESET_HERO, PRESET_TOP_DEBUG, PRESET_LOW_OBLIQUE}` into `ParamsPanel`
  (or `CameraPanel`) as a one-shot apply-to-orbit-camera control.
- **Pass 4 — Blue noise dithering (§3.2 B3):** upload
  `assets/noise/blue_noise_2d_64.png` as a 2D texture in `TerrainRenderer`,
  add a sampler binding, modify `fs_terrain` to add ±1/255 dither from
  the blue-noise sample.
- **9 baseline screenshots:** 3 camera presets × 3 golden seeds in
  `docs/design/sprints/sprint_1a_visual_acceptance/` as the Sprint 1B
  regression baseline. Blocked on Pass 3 for the preset switching.
- **Paper pack (non-blocking per §6):** Chen 2014 / Génevaux 2013 deep
  reads; Lague 2014 target-deep; background papers can stay at
  `metadata_only`.

---

## NEXT SESSION PLAN

Three visual-polish passes queued, to execute in order with
**simplifier → superpowers code-reviewer → commit** cadence per pass
(per auto-memory `feedback_commit_review_workflow.md`). Test baseline
going into Pass 2 is **180 passed**; each pass should add 0–3 tests.

### Pass 2 — Overlay render path (Task 1A.10)

**Files to create / modify:**
- `crates/render/src/overlay.rs` — add `resolve_scalar_source(world, source)
  -> Option<ResolvedField>` (keeps string-key dispatch in this file per
  CLAUDE.md invariant #8). Do NOT break existing tests.
- `crates/render/src/overlay_render.rs` (new) — `render_overlay_to_gpu(desc,
  world) -> Option<(Vec<u8>, u32, u32)>` pure bake function (testable), +
  `OverlayRenderer` struct owning one pipeline + per-descriptor bind groups
  (texture view + sampler + alpha uniform). Reuse terrain VBO/IBO and
  share `TerrainRenderer`'s View uniform buffer via a new `pub fn view_buf
  (&self) -> &wgpu::Buffer` getter.
- `shaders/overlay.wgsl` (new) — vertex shader identical to `terrain.wgsl`
  `vs_terrain` minus the normal+uv routing pieces it doesn't need, fragment
  samples a 2D texture at uv + multiplies rgba by descriptor alpha uniform.
  Must not contain RGB literals.
- `crates/render/src/lib.rs` — `pub mod overlay_render;` + re-export.
- `crates/render/src/terrain.rs` — expose `pub fn view_buf(&self) ->
  &wgpu::Buffer` for the overlay pipeline to share.
- `crates/app/src/runtime.rs` — construct `OverlayRenderer::new(&gpu,
  &world, &overlay_registry, terrain.view_buf())` after terrain. In
  `tick()` after `self.terrain.draw`, call
  `self.overlay.draw(&mut rpass, &self.overlay_registry)` to iterate
  visible descriptors and draw each as a second pass over terrain geometry.

**Design notes:**
- Bake overlay RGBA8 CPU-side at `OverlayRenderer::new` time (Sprint 1A
  pipeline runs once at boot; overlays are static). Use `palette::sample
  (palette_id, t)` for the per-cell lookup.
- `ValueRange` has `Auto / Fixed / LogCompressed`. Call `range.resolve
  (field_min, field_max)` to get `(lo, hi)`, then `t = (v - lo) / (hi - lo)`
  clamped to `[0, 1]`. For LogCompressed take `ln(1 + v.max(0.0))` before
  normalising.
- Tests: add `render_overlay_to_gpu_elevation_matches_palette` that feeds
  a known z_filled field + TerrainHeight palette + Auto range and verifies
  specific pixels match `palette::sample(TerrainHeight, t)` outputs.
- Alpha-blend mode: `BlendState { color: src_alpha * src + (1 - src_alpha)
  * dst, alpha: additive }` — standard over-blend.

### Pass 3 — Camera preset dropdown (§3.2 A6)

**Files to modify:**
- `crates/app/src/camera_panel.rs` — add an egui ComboBox above the existing
  drag controls listing `PRESET_HERO / PRESET_TOP_DEBUG / PRESET_LOW_OBLIQUE`
  from `render::ALL_PRESETS`. On selection, compute the corresponding orbit
  camera state (`distance`, `yaw`, `pitch`) from the preset's `(eye_theta,
  eye_phi, distance_scale)` using `preset.island_radius` and apply it to
  the mutable `Camera` reference.
- `crates/app/src/camera.rs` — no changes (unless a helper like `Camera::
  apply_preset(preset, island_radius)` cleans up the panel code).
- `crates/app/src/runtime.rs` — pass `preset.island_radius` to the panel
  so the preset can compute distance.

**Design notes:**
- The existing orbit controls stay fully functional after preset
  selection; the preset is a one-shot state load, not a mode switch.
- TopDebug preset is orthographic (`pitch = π/2 - 0.01`). The orbit
  camera uses perspective — clamp pitch to the orbit range on apply.
- Add a `camera_preset_apply_round_trip` test that creates a `Camera`,
  applies each preset, and verifies distance/yaw/pitch are finite.

### Pass 4 — Blue noise dithering (§3.2 B3)

**Files to modify:**
- `shaders/terrain.wgsl` — add `@group(0) @binding(3) var blue_noise:
  texture_2d<f32>; @group(0) @binding(4) var blue_noise_sampler: sampler;`.
  In `fs_terrain`, replace the final `return vec4<f32>(lit_rgb, 1.0)` with:
  `let dither = (textureSample(blue_noise, blue_noise_sampler,
  input.uv * DITHER_TILE).r - 0.5) * (1.0 / 255.0);
  return vec4<f32>(lit_rgb + vec3<f32>(dither), 1.0);`
  Where `DITHER_TILE` is a constant like `8.0`. Must not add RGB literals
  — the `0.5` and `1.0/255.0` are math constants, not colours.
- `crates/render/src/terrain.rs` — upload
  `render::noise::load_blue_noise_2d(64)` as an R8Unorm 2D texture inside
  `TerrainRenderer::new`. Create a sampler with `AddressMode::Repeat` on
  both axes. Extend the bind group layout and bind group to add the two
  new entries (texture view at 3, sampler at 4). Update the existing
  `terrain_wgsl_has_no_literal_colors` test if the new constants trip the
  grep (they shouldn't — they're `0.5` and `1.0`, wrapped in math, not
  colour constructors).
- Tests: `terrain_wgsl_parses_successfully` must still pass after the
  new bindings land (naga will validate the texture/sampler types).

**Design notes:**
- Blue-noise texture is 64×64 R8 (via `BlueNoiseTexture.data: Vec<u8>`).
  Upload with `device.create_texture_with_data` (wgpu-util crate) or
  `queue.write_texture` after `create_texture`.
- Repeat address mode is what makes a 64×64 tile dither a 256×256 mesh.
  `DITHER_TILE = 8.0` means the 64×64 noise tile repeats across each
  ~1/8 of the terrain UV space.
- Sample in the fragment shader, subtract 0.5 to centre around 0, scale
  by `1.0/255.0` so the dither is ±½ LSB — enough to break banding but
  invisible as noise at sRGB output.

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

**Still to ship for Sprint 1A §6 full acceptance:**
- **Pass 2 — Task 1A.10 overlay render path:** `render_overlay_to_gpu
  (desc, world)` pure bake function + `OverlayRenderer` + new
  `shaders/overlay.wgsl`. CPU-side RGBA8 bake via `palette::sample`,
  alpha-blend over terrain in same pass. See `NEXT SESSION PLAN` for
  the full breakdown.
- **Pass 3 — Camera preset dropdown (§3.2 A6):** wire `render::camera
  ::{PRESET_HERO, PRESET_TOP_DEBUG, PRESET_LOW_OBLIQUE}` into
  `CameraPanel` as a one-shot "apply preset to orbit camera" control.
- **Pass 4 — Blue noise dithering (§3.2 B3):** upload
  `assets/noise/blue_noise_2d_64.png` as a 2D texture in
  `TerrainRenderer::new`, add sampler binding to `shaders/terrain.wgsl`,
  add `±1/255` dither in `fs_terrain` to break gradient banding.
- **9-screenshot visual baseline** in
  `docs/design/sprints/sprint_1a_visual_acceptance/` (3 camera presets
  × 3 golden seeds) — blocked on Pass 3 for preset switching.
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
