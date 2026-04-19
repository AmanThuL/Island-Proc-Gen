# PROGRESS

**Last Updated:** 2026-04-19 (Sprint 2.6 shipped — editor layout + `DEFAULT_WORLD_XZ_EXTENT` at Fuji-like 5.0 + dither A/B decided DROP; Sprint 2 residuals still pending Sprint 3)

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

**Primary:** Sprint 2.6 — Editor Layout, World Proportions & Visual Tail.
**Closed on `dev` 2026-04-19** with 13 atomic commits (`32ed155 →
f35941e`) across the 5 planned tasks + 1 user-requested follow-up
(aspect ratio ComboBox). Every feature commit used the CLAUDE.local.md
implementer → `code-simplifier` → `superpowers:code-reviewer` cadence;
the reviewer is fixed to Opus per an updated user preference. Test
delta: 405 → 424 passing (+19), 5 → 8 ignored (+3 new `#[ignore]`'d
GPU tests for offscreen viewport + sea-quad Y refresh). Hard CI gate
(`cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo
test --workspace`) all green. 3 `--headless` baselines exit 0; beauty
PNGs regenerated twice as cascade follow-ups (`090337c` at
`WORLD_XZ_EXTENT = 3.0`, then `f35941e` at the user-frozen Fuji-like
`DEFAULT_WORLD_XZ_EXTENT = 5.0`).

Sprint 2.6 shipped three user-visible changes out of the box:

1. **Engine-editor dock layout.** `egui_dock = "0.19"` drives an
   `Overlays (left) | Viewport (centre, non-closeable) | World +
   Camera + Params + Stats (tabbed right)` layout. 3D renders into
   an offscreen `ViewportTextureSet`; the image sits inside an
   `egui::Image` widget inside the `Viewport` tab. Camera input
   (orbit / pan / zoom) only fires when the cursor is inside the
   viewport rect; mouse delta fraction + camera aspect both
   normalise against the viewport rect, not the window. Layout
   persists to `~/.island_proc_gen/dock_layout.ron` on close; load
   gracefully falls back to default on missing / corrupt file.
2. **World proportions from the source.** The Sprint 1A `vertical_
   scale` Y-axis slider (a debug-era workaround for the 1:0.85
   aspect) is fully deleted; `render::DEFAULT_WORLD_XZ_EXTENT = 5.0`
   (Fuji-like aspect ≈ 0.17) is the baseline-capture const, and
   `Runtime::world_xz_extent` is a runtime-mutable field with a
   World-panel ComboBox (Pico-like 15.0 / Fuji-like 5.0 (default) /
   Moderate 3.0 / Steep 2.0). All render functions take `extent: f32`
   as an explicit parameter; headless passes the default, live app
   lets the user A/B. Live + headless render identical by construction.
3. **Runtime preset + seed switching.** New World panel (preset
   ComboBox from `data::presets::list_builtin()` + seed DragValue<u64>
   + `island_radius / max_relief / sea_level` sliders + Regenerate
   button). Regenerate runs the 7-step full rebuild (new preset →
   new WorldState → full pipeline → TerrainRenderer rebuild →
   OverlayRenderer rebuild → camera recentre → panel state reset).
   `sea_level` drag-release takes a 5-step fast path (invalidate +
   `run_from(Coastal)` + `TerrainRenderer::update_sea_level` +
   overlay refresh + camera Y sync) that avoids the full pipeline
   rerun. No async — all synchronous ~300 ms for the full rebuild.

Sprint 2.5's two display-gated deferrals closed in this sprint:

- **2.6.D — dither A/B**: decided **DROP**. In-window session
  2026-04-19 (`volcanic_single` seed 42 @ 128² Hero) reported no
  perceptible difference between dither ON and OFF at the project's
  render scale. `DITHER_ON` uniform / Camera-panel checkbox /
  `TerrainRenderer::update_dither` all removed; `shaders/terrain.wgsl`
  reverts to the Sprint 1A unconditional dither (still sampled from
  `blue_noise_2d_64.png`). `assets/noise/blue_noise_2d_{128,256}.png`
  deleted (64 tile kept for overlay_render). No "deferred toggle"
  tail.
- **2.6.E — blue-noise size**: closed `n/a via upstream 2.6.D drop`,
  no commit. The decision tree gated E on D keeping dither.

One B.3 regression fix (`966e545`) mid-sprint: egui's
`response.consumed` flag was swallowing mouse events landing on the
viewport image, so dragging inside the Viewport tab didn't move the
camera. Fix was to let mouse events fall through our handler
regardless of consumed (the viewport-rect gate already routes
correctly), while keeping the early-return for non-mouse events.

**Secondary:** Sprint 2 Geomorph Credibility — **still closed on
`dev` 2026-04-18** (close-out commits ab7d5b5 ← 8145b38). Two §10
acceptance residuals remain handed off to Sprint 3; see
"Sprint 2 acceptance status" below for details. Close-out chain
surfaced 3 Critical bugs + 7 Important items during the retroactive
simplifier + `superpowers:code-reviewer` pass:

1. **CoastType overlay transparent** — `ValueRange::Fixed(0.0, 3.0)`
   mapped discriminant 3 (RockyHeadland, ~50 % of coast cells) to
   idx 4 → transparent. Fixed to `Fixed(0.0, 4.0)` + 3 palette
   regression tests.
2. **Runtime pipeline drift** — `crates/app/src/runtime.rs` kept a
   local `build_sprint_1b_pipeline()` that didn't track StageId
   shifts from 2.3 / 2.4, so every slider `run_from` silently hit
   the wrong stage (wind_dir → PetStage, erosion sliders →
   TemperatureStage). Swapped for `sim::default_pipeline()`;
   downstream crates now have zero local pipeline builders.
3. **`RunSummary.schema_version` hardcoded** — broke the Sprint 1C
   v1 baselines' forward-compat contract under v2 binaries. Now
   mirrors `CaptureRequest.schema_version`.

**405 tests passing, 5 ignored** across 9 crates (+20 net over Sprint
2's 385). `cargo fmt --check && cargo clippy --workspace -- -D warnings
&& cargo test --workspace` is the hard CI gate, all green. All three
`--headless` baselines self-validate exit 0:

- `sprint_1a_baseline/`: 9 shots (Sprint 1C first-shipped + 2.5.H
  flow_accum clamp-percentile regen + 2.5.Ja biome tuning regen)
- `sprint_1b_acceptance/`: 15 shots (Sprint 1C 9 default-wind +
  Sprint 2.5.E 6 wind-varying; shot `01_baseline_camera_overlays_panels`
  permanently manual — UI state non-serialisable)
- `sprint_2_erosion/`: 6 shots (3 presets × pre/post erosion; Sprint
  2.5 regens for biome + flow_accum)

**Sprint 2.5 acceptance status:**

- ✓ **2.5.F — 3 new archetypes**: `volcanic_caldera_young` /
  `volcanic_twin_old` / `volcanic_eroded_ridge`. Sprint 3 sediment
  work has 5 archetypes to validate on instead of 3. Deviation
  logged: `volcanic_twin_old` ships with default `n_batch=10`, not
  the spec's 15 (fired the 5 % sea-crossing invariant at safe K).
- ✓ **2.5.G — basin CC promotion**: post-process Von4 connected-
  component labelling with `MIN_INTERNAL_LAKE_CELLS=8` threshold +
  new `basin_partition_post_erosion_well_formed` invariant. On real
  terrain the pass is currently vacuous (PitFill re-runs inside
  ErosionOuterLoop eliminate interior depressions); Sprint 3
  sediment-aware SPACE-lite may leave intentional deposition lakes
  for the CC promotion to pick up.
- ✓ **2.5.Ja — ecology tuning**: `volcanic_single @ 128² seed 42
  post-erosion` now shows 5 biomes ≥ 3 % coverage (MontaneWet 8 /
  DryShrub 18 / Grassland 43 / BareRockLava 26 / Riparian 5; pre-
  tune was 3 at ≥ 3 %). Wind 180° swing drives ~14 % argmax flip,
  well above the ≥ 6 % spec target. CoastalScrub + CloudForest
  stay at 0 % on this archetype (dry interior / no fog) — tuned
  bells can still produce them on wetter archetypes.
- ✓ **2.5.B/C/D — hex UX slice**: `HexDebugAttributes` sibling
  struct (slope_variance + river_crossing + accessibility_cost)
  alongside `HexAttributes` (still 8 fields). 3 new overlays
  (`hex_projection_error`, `hex_river_crossing`, `hex_accessibility`)
  plug into the existing `OverlaySource::ScalarDerived` + `Mask`
  pipelines — no new wgpu pipeline. `HexRiverCrossing` uses 4 box
  edges (not spec's 6 hex edges) because the hex tessellation is
  axis-aligned boxes; Sprint 5 S1 does the real-hex rework.
- ✓ **2.5.A — ViewMode toggle**: `Continuous` / `HexOverlay` /
  `HexOnly` via Camera-panel ComboBox. `saved_visibility` holds
  the Continuous baseline while `view_mode != Continuous` so
  any round-trip back to Continuous restores the user's original
  state, regardless of HexOverlay/HexOnly hops.
- ✓ **2.5.E — 1B baseline migration**: 9 → 15 shots on
  `sprint_1b_acceptance/`; `schema_version` bumped 1 → 2 to use
  `preset_override.prevailing_wind_dir`. Distinct byte-level
  `soil_moisture` hashes confirm wind propagation end-to-end.
- ✓ **2.5.H — flow_accumulation audit**: distribution measured
  (P90/max = 0.023, clear washout); `ValueRange::LogCompressedClampPercentile(0.99)`
  variant added + `flow_accumulation` descriptor switched.
- ✓ **2.5.K — per-descriptor alpha**: `OverlayDescriptor.alpha`
  field + OverlayPanel iterates `registry.entries_mut()` to render
  `[checkbox][slider][label]` per row. Row count = registry size,
  not hardcoded. `OverlayRenderer::draw` writes per-frame alpha
  uniforms (`registry.len() × 4 bytes` cost).
- ✓ **2.5.I — dither A/B**: **closed via Sprint 2.6.D DROP decision
  (2026-04-19)**. In-window A/B (`volcanic_single` seed 42 @ 128²
  Hero) reported no perceptible difference; Sprint 1A unconditional
  dither retained in `terrain.wgsl`, the `DITHER_ON` toggle machinery
  was never shipped (added in 2.6.D code commit `cf3b181`, removed in
  2.6.D cleanup `d39e2f3` same sprint).
- ✓ **2.5.L — blue-noise size toggle**: **closed `n/a via upstream
  2.6.D drop`**. Since 2.6.D dropped dither, the blue-noise size
  ComboBox has no signal to A/B against. No commit; `assets/noise/
  blue_noise_2d_{128,256}.png` deleted as part of `d39e2f3` (only
  the `64` tile remains, consumed by `overlay_render.rs`).

**Sprint 2 §10 acceptance status (unchanged from Sprint 2 close-out):**

- ✓ SPIM + hillslope + `ErosionOuterLoop` (scheme B) + CoastType
  + schema v2 + `preset_override` + erosion sliders + 3 new
  invariants + 4 new `SummaryMetrics` fields + sprint_2 paper pack
  + 19-stage doc sync — all §10 items green.
- ✗ **"max_z 下降 10–30 %"** across 3 presets: measured
  0.19 % / 1.54 % / 1.52 % at the safe K=1.5e-3 calibration. The
  18 % projection was physically incompatible with the
  `erosion_no_excessive_sea_crossing` 5 % invariant under uniform
  SPIM (see Sprint 2 residual #1 below and the CLAUDE.md SPIM
  calibration gotcha). Deferred to Sprint 3 sediment-aware
  `K·g(hs)` modulation.
- ✗ **CoastType "每 type 5 % 占比"** per preset × hero shot: at the
  safe K, max coastal slope rarely exceeds 0.07, so Cliff stays at
  0 % across all 3 presets even after threshold tuning. Estuary
  ~2-3 % is bounded by actual river-mouth count (physical limit).
  Beach + RockyHeadland both > 25 %. Deferred to Sprint 3
  sediment-aware terrain (sharper cliffs form naturally) or a
  coast_type v2 classifier with fetch-integral wave exposure.

Both residuals have explicit Sprint 3 anchor points — they are
natural fits for the next sprint's work, not Sprint 2 blockers.

**Next session priorities** (see [QUICK REFERENCE](#quick-reference)):
1. **Sprint 3** — Sediment + Advanced Climate. SPACE-lite sediment-
   aware erosion (`K · g(hs)`), LFPM v3 precipitation, cloud forest
   belt + fog hydrology, Coast type v2 (fetch integral + LavaDelta),
   riparian biome alluvial-fan-aware upgrade, optional DualSeason
   wind. Inherits Sprint 2's two deferred §10 clauses (max_z drop
   range, Cliff coverage) as natural targets. Sprint 2.5's 5
   archetypes + 15-shot 1B baseline + tuned biome bells + hex debug
   overlays, and Sprint 2.6's editor layout + World panel + Fuji-like
   world aspect ratio, together form the starting baseline. Sprint
   2.6 did NOT produce any sim-side changes; the three `--headless`
   baselines are truth-identical to pre-2.6 state (only beauty PNG
   framing drifted). Sprint 3 can open the Sprint 2.6 aspect freeze
   re-decision if "sculpted silhouette" from sediment-aware erosion
   reads differently at some other aspect.
   Doc: `docs/design/sprints/sprint_3_sediment_advanced_climate.md` (TBD).
2. **Sprint 1B paper pack** (low-energy): Bruijnzeel 2005 / 2011,
   Chen 2023 Budyko, Core Pack #2/#3/#5/#6/#8 落地点 sections.

---

## RECENTLY SHIPPED

### Sprint 2.6 — Editor Layout, World Proportions & Visual Tail (2026-04-19, 13 commits on `dev`)

**Doc:** [`docs/design/sprints/sprint_2_6_editor_layout_and_visual_tail.md`](docs/design/sprints/sprint_2_6_editor_layout_and_visual_tail.md) (Obsidian symlink, gitignored)
**Test delta:** 405 → 424 passing (+19), 5 → 8 ignored (+3 new GPU `#[ignore]`'d).

Sprint 2.6 takes the live app from "4 egui windows stacked at (16,16) + a debug vertical-scale slider faking world aspect ratio + no runtime preset switch" to "engine-editor dock layout + world aspect frozen at Fuji-like + runtime preset/seed/geometry panel". Closes out the two Sprint 2.5 display-gated deferrals (2.5.I dither, 2.5.L blue-noise size) with DROP + n/a respectively.

| Commit | Task | What shipped |
|---|---|---|
| `32ed155` | 2.6.A | `render::WORLD_XZ_EXTENT = 3.0` (initial value) — mesh + sea quad + render camera preset LUT + interactive orbit camera all read the const; `vertical_scale` field + slider + `INITIAL_VERTICAL_SCALE` const fully deleted; `INITIAL_CAMERA_DISTANCE` stays pre-EXTENT semantic, multiplied by EXTENT at use sites (Option A, explicit coupling). +2 mesh tests locking `max(x) == EXTENT`. |
| `090337c` | 2.6.A cascade | 3 `--headless` baseline beauty PNGs regen at EXTENT = 3.0. Truth hashes bit-identical; only beauty `byte_hash` + AD7 whitelist (timestamp, ms fields) drifted. |
| `0c4a310` | 2.6.0 | PROGRESS.md roll-forward at sprint start — records the decision to execute 2.6 before Sprint 3 and the 3 scope pillars. |
| `1bda58b` | 2.6.B.1 | `render::ViewportTextureSet` — offscreen colour + depth textures sized to the egui Viewport tab rect. Registered with `egui_wgpu::Renderer` via `register_native_texture`; resizes preserve `egui::TextureId` via `update_egui_texture_from_wgpu_texture`. 3D render pass retargeted from window surface to viewport texture; egui pass `LoadOp::Clear` on the window surface since terrain no longer paints there. +2 GPU-ignored tests. |
| `dfbcd38` | 2.6.B.2 | `egui_dock = "0.19"` integration. New `crates/app/src/dock.rs` with `TabKind` (6 variants, Viewport non-closeable) + `DockLayout::default_layout()` (Overlays 20% left / Viewport centre / World+Camera+Params+Stats tabbed 25% right). All 4 floating `egui::Window`-based panels refactored to take `ui: &mut egui::Ui` as the first parameter — no backwards-compat shim per CLAUDE.md. +4 tests. |
| `d901ad9` | 2.6.B.3 | Viewport-aware input routing + dock layout persistence to `~/.island_proc_gen/dock_layout.ron`. Mouse events gated on `cursor_in_rect_physical(cursor, viewport_rect, ppp)`; delta fraction normalized against viewport rect (not window size) so shrinking the tab keeps orbit sensitivity stable; camera aspect ratio tracks viewport rect. Load gracefully falls back on missing / corrupt file. CLAUDE.md gotcha added for egui_dock ↔ egui lockstep. +6 tests. |
| `4ce2381` | 2.6.C | New `crates/app/src/world_panel.rs` — preset ComboBox (6 built-in archetypes) + seed `DragValue<u64>` + 3 geometry sliders (island_radius / max_relief / sea_level) + Regenerate button. `regenerate_from_world_panel` runs the 7-step full rebuild; `apply_sea_level_fast_path` runs the 5-step drag-release path (`invalidate_from(Coastal)` + `run_from` + `TerrainRenderer::update_sea_level` + overlay refresh + camera Y sync) that avoids the full pipeline rerun. `TerrainRenderer` gained `light: LightRigUniform` + `light_buf: wgpu::Buffer` fields + `update_sea_level` method; `sea_vbo` picked up `COPY_DST` so `queue.write_buffer` works. +5 tests + 1 GPU-ignored. |
| `966e545` | B.3 fix | Viewport drag was swallowed by the `response.consumed` early-return at the top of `handle_window_event` — egui marks cursor-over-Image events as consumed, so our viewport-rect gate never ran. Fix: let mouse events fall through regardless of consumed (the viewport-rect gate inside each arm already routes correctly); keep consumed-early-return for keyboard events (egui remains authoritative for future text inputs). |
| `cf3b181` | 2.6.D code | `DITHER_ON` uniform (reusing `LightRig.sea_level.y` padding slot) + Camera-panel checkbox + `TerrainRenderer::update_dither` method. Default ON (Sprint 1A behaviour). +1 mirror test. Reverted same-sprint after the user's A/B decision. |
| `d39e2f3` | 2.6.D drop | User's 2026-04-19 in-window A/B (`volcanic_single` seed 42 @ 128² Hero) reported no perceptible difference between dither ON and OFF. Shader branch + uniform + Camera-panel toggle + `Runtime::dither_on` all removed; `shaders/terrain.wgsl` reverts to Sprint 1A unconditional dither (`dither_tile = 8.0`, amplitude `1.0/255.0`, `blue_noise_2d_64.png`). `assets/noise/blue_noise_2d_{128,256}.png` deleted; 64 tile kept (overlay_render still uses it). 2.6.E closed `n/a via upstream 2.6.D drop`, no commit. No deferred toggle tail. |
| `9653f4d` | 2.6.A follow-up | User reported `EXTENT = 3.0` still reads as "mountain tip" even after preset/max_relief/island_radius experiments. Parametrize the const: `WORLD_XZ_EXTENT` → `DEFAULT_WORLD_XZ_EXTENT` (still 3.0 initially, baseline-capture contract), every render function gains `extent: f32` explicit parameter, `Runtime::world_xz_extent` field drives the live app, World panel gets an aspect ComboBox (Pico-like 15.0 / Fuji-like 5.0 / Moderate 3.0 / Steep 2.0) firing `aspect_extent_changed: Option<f32>`. `apply_world_aspect` does a render-only rebuild (no sim pipeline rerun). Headless keeps passing `DEFAULT_WORLD_XZ_EXTENT` explicitly. +2 tests. |
| `3d8dc7e` | 2.6.A freeze | After the follow-up in-window A/B, user froze the default at Fuji-like. `DEFAULT_WORLD_XZ_EXTENT: f32 = 5.0`; ComboBox `default` annotation swaps from Moderate to Fuji-like. The runtime override + ComboBox stay in place so Sprint 3 / 3.5 can re-open the decision if sediment-aware erosion's silhouette reads differently at some other aspect. |
| `f35941e` | 2.6.A freeze cascade | Second beauty PNG regen for the 3.0 → 5.0 bump. Truth hashes bit-identical; only beauty `byte_hash` + AD7 whitelist drift. Mirrors `090337c`'s structure. |

### Sprint 2.5 — Hex UX Slice + Sprint 2 Tail Absorption (2026-04-18, 10 commits on `dev`)

**Doc:** [`docs/design/sprints/sprint_2_5_hex_ux_and_tail.md`](docs/design/sprints/sprint_2_5_hex_ux_and_tail.md) (Obsidian symlink, gitignored)
**Test delta:** 385 → 405 passing (+20 across A/B/C/D/E/F/G/H/Ja/K), 5 ignored unchanged.

Sprint 2.5 lands the "continuous → hex aggregation" thesis's first
visual evidence (three hex debug overlays + ViewMode toggle) while
absorbing all Sprint 1A / 1B / 2 polish tail items that aren't
scope-appropriate for Sprint 3's science work. Sprint 3 now starts
from a polished, hex-validated, 5-archetype, biome-diverse baseline.

| Commit | Task | What shipped |
|---|---|---|
| `26ff9a5` | 2.5.F | 3 new archetype RON files — `volcanic_caldera_young`, `volcanic_twin_old`, `volcanic_eroded_ridge` — + 3 round-trip serde tests + `list_builtin()` extension. `volcanic_twin_old` ships with default `n_batch=10` (spec wanted 15 but it fires the 5 % sea-crossing invariant at safe K). |
| `c419415` | 2.5.G | Basin post-process CC labelling — `MIN_INTERNAL_LAKE_CELLS=8` Von4 + `basin_partition_post_erosion_well_formed` invariant. Defensive on real terrain today (PitFill fills all interior depressions); ready for Sprint 3 SPACE-lite deposition lakes. +3 tests. |
| `93f7c5b` | 2.5.Ja | 6 biome suitability bells widened (`suitability.rs` only, zero `climate/` changes per 2.5.Jb scope split). `volcanic_single` collapses 3 biomes → 5 biomes ≥ 3 % coverage. Wind 180° swing → ~14 % argmax flip. Golden-seed snapshots regen. +1 diversity regression test. |
| `e186756` | 2.5.Ja regen | Cascade regen of all 3 `--headless` baselines for `dominant_biome` + `hex_aggregated` hash drift. Non-biome truth hashes bit-identical. |
| `10c77d1` | 2.5.B + D | `HexDebugAttributes` sibling struct (2 fields initially: `slope_variance` + `accessibility_cost`) + 2 per-cell broadcast caches + 2 new overlays (`hex_projection_error`, `hex_accessibility`). `OverlayRegistry::sprint_2_defaults → sprint_2_5_defaults` rename (no compat shim). `W_SLOPE / W_RIVER / W_CLIFF = 3.0 / 2.0 / 5.0` locked constants. +6 tests. |
| `c8730dd` | 2.5.A + C | `ViewMode` enum (`Continuous` / `HexOverlay` / `HexOnly`) + Camera-panel ComboBox + `saved_visibility` baseline snapshot that survives intermediate hops. `HexRiverCrossing` type (4 box edges, not 6 hex — real-hex rework is Sprint 5 S1) as `HexDebugAttributes` 3rd field + per-hex entry/exit argmin/argmax accumulation + Bresenham mask rasterisation + new `hex_river_crossing` mask overlay. No new wgpu pipeline. +5 tests. |
| `554f4f7` | 2.5.E | Sprint 1B baseline 9 → 15 shots. `schema_version` 1 → 2. 6 new wind-varying shots exercising `preset_override.prevailing_wind_dir`. Byte-level `soil_moisture` hash divergence confirmed on opposing wind directions. Permanent-manual exclusion of `01_baseline_camera_overlays_panels` documented in new README. |
| `1073f4e` | 2.5.K | `OverlayDescriptor.alpha: f32` (default 0.6) + OverlayPanel `[checkbox][slider][label]` per descriptor row, iterating `registry.entries_mut()` — zero hardcoded overlay counts. `OverlayRenderer::draw` writes per-frame alpha uniforms. Beauty PNG bit-identical. +1 test. |
| `4dc75ed` | 2.5.H | `ValueRange::LogCompressedClampPercentile(f32)` variant — computes p-quantile of `ln(1+value)` at bake time instead of clamping on max. `flow_accumulation` descriptor switches to `LogCompressedClampPercentile(0.99)`; `volcanic_twin` distribution P90/max = 0.023 established the washout mathematically. Cascade regen of `flow_accumulation` hash across all 3 baselines; other overlays bit-identical. |

**Deferred within Sprint 2.5** (historical — see forward pointer at end):

- **2.5.I — dither A/B audit**: needs interactive display for the
  ±½ LSB banding comparison; headless PNG readback loses the
  subpixel rendering that would reveal the banding. Memo in
  `docs/design/sprints/sprint_2_5_visual_acceptance/dither_ab_audit.md`
  (local-only). No code change — the dither stays in `terrain.wgsl`.
- **2.5.L — blue-noise size toggle**: dependency-gated on 2.5.I per
  the spec decision tree. Re-opens when 2.5.I resolves.

> **Forward pointer (2026-04-19):** 2.5.I and 2.5.L are no longer
> deferred — both have been **absorbed into Sprint 2.6.D / 2.6.E**
> at sprint-planning time. See `DEFERRED TO LATER SPRINTS` section
> for the current absorption status and Sprint 2.5.L lock-rule
> compliant drop-path behaviour. The two paragraphs above are the
> close-out snapshot as of 2026-04-18 and are retained for history.

**Sprint 2.5 plan key decisions (locked):**

- **Hex tessellation is boxes, not hexes**: `crates/hex/src/lib.rs`
  builds an axis-aligned rectangular tiling per Sprint 1B. Sprint 2.5
  `HexRiverCrossing` uses 4 box edges (0=top, 1=right, 2=bottom,
  3=left); Sprint 5 S1 does the real-hex rework and will expand to
  6 edges. `HexRiverCrossing` lives inside `HexDebugAttributes` (not
  `HexAttributes`) precisely to isolate that future expansion.
- **`HexAttributes` stays at 8 fields** (§2 不做 #7 + roadmap §Sprint
  2.5 line 1712). All Sprint 2.5 hex debug data lives in
  `HexDebugAttributes` sibling struct. Sprint 3 / 4 don't read it;
  Sprint 5 S2 settlement consumers can redesign it freely.
- **`OverlayRegistry::sprint_2_5_defaults` = 16 descriptors**: 13
  from Sprint 2 + 3 new from Sprint 2.5 (`hex_projection_error`,
  `hex_accessibility`, `hex_river_crossing`). No `sprint_2_defaults`
  backwards-compat alias — call sites updated directly per CLAUDE.md.
- **ViewMode snapshot policy**: `saved_visibility` holds the
  Continuous baseline whenever `view_mode != Continuous`. Snapshot
  taken on first departure from Continuous, cleared on return. So
  `Continuous → HexOverlay → HexOnly → Continuous` restores the
  user's original state exactly, instead of carrying HexOverlay's
  `hex_aggregated=true` side-effect into the restore.
- **Accessibility cost formula**: `1 + 3·mean_slope + 2·river_penalty
  + 5·cliff_penalty`, with cliff_penalty = fraction of sim cells in
  hex (including sea) that are `CoastType::Cliff`. Sprint 5 S2
  settlement is the real consumer; 2.5.D just makes the formula
  observable in the debug overlay.
- **Ecology tuning is `ecology/` only**: Sprint 2.5.Jb (climate
  constants) explicitly deferred to Sprint 3 LFPM v3. A broader
  `CONDENSATION_RATE` / `RAIN_SHADOW_K` re-tune would have been
  throwaway work ahead of the LFPM v3 precipitation model.
- **Flow accumulation fix via percentile-clamp, not palette change**:
  Turbo is Sprint 1A spec-locked. The washout is in the `ln(1+max)`
  tail-driven ceiling, not in the palette. `LogCompressedClampPercentile(0.99)`
  is the minimal change — generic enough to re-use on any future
  long-tail overlay.

**Invariants preserved:** `cargo tree -p core` still clean;
`WorldState` 3-layer structure unchanged (new fields in `derived`
only); `ScalarField2D<T>` + aliases unchanged; descriptor-based
overlay system preserved (`alpha: f32` is a descriptor field, not
a closure); string keys still confined to `render/src/overlay.rs`;
all 8 architectural invariants green. `HexAttributes` still 8
fields. `StageId` enum unchanged (no new stages this sprint).

---

### Sprint 2 — Geomorph Credibility (2026-04-18, 21 commits on `dev`)

**Doc:** [`docs/design/sprints/sprint_2_geomorph_credibility.md`](docs/design/sprints/sprint_2_geomorph_credibility.md) (Obsidian symlink, gitignored)
**Test delta:** 335 → 385 passing (+50 net across all 11 tasks + retroactive follow-ups), 5 ignored unchanged.

Sprint 2 opens up `authoritative.height` for iterative rewrite for
the first time (Sprint 1A/1B treated it as write-once). SPIM stream-
power incision + hillslope diffusion alternate inside an
`ErosionOuterLoop` composite stage for `n_batch × n_inner = 10 × 10`
iterations, with end-of-batch cache invalidation + re-routing of the
Coastal..RiverExtraction chain. A `CoastType` classifier produces
per-coast-cell categorical output, and `CaptureRequest` bumped to
schema v2 with `preset_override` for scripted pre/post erosion
compares.

| Commit | Task | What shipped |
|---|---|---|
| `8145b38` | 2.1 | `ErosionParams` nested preset field + `StreamPowerIncisionStage` (SPIM `Ef = K · A^m · S^n` with locked `(0.35, 1.0)` per KP17). Clamp at sea_level; non-finite → 0.0. +8 tests. |
| `9cb0920` | 2.10 | `docs/papers/sprint_packs/sprint_2.md` + KP17 substantive write-up + 4 companion note stubs + 2 parking-lot metadata entries (Braun 2023, Yuan 2019). |
| `7621c1f` | 2.2 | `HillslopeDiffusionStage` — explicit Euler 5-point Laplacian, `n_diff_substep` sub-steps per call, skip sea + coast + grid boundary. Double-buffer swap, no per-substep allocation. +7 tests. |
| `a4ab31f` | 2.5 | `CaptureRequest` schema v2 + `PresetOverride::apply_to`. `Eq` dropped from `CaptureRequest` / `CaptureShot` (f32 path via override). v1 RON files continue to parse under v2 binary. +5 tests. |
| `b97a51d` | 2.3 | `ErosionOuterLoop` scheme B at `StageId::ErosionOuterLoop = 8`. `core::world::ErosionBaseline` + `derived.erosion_baseline` sticky-snapshot. 1B variants shifted to 9..=16; `STAGE_COUNT = 17`. `non_eroding_pipeline()` test helper + `non_eroding_index` for bit-exact invalidation round-trips. `invalidate_plus_run_from_equals_fresh_run_at` rewired. +4 tests. |
| `a0bb7d5` | 2.4+2.8 | `CoastType` enum + `CoastTypeStage` at `StageId::CoastType = 9`. 1B variants shifted to 10..=17; `STAGE_COUNT = 18`. `PaletteId::CoastType` + `COAST_TYPE_TABLE`. `OverlayRegistry::sprint_2_defaults()` = 13 descriptors. Alias methods `sprint_1a_defaults` / `sprint_1b_defaults` removed per CLAUDE.md. +12 tests. |
| `159af24` | 2.7 | `ParamsPanel` 4 erosion sliders (Tier A live `spim_k` / `hillslope_d`; Tier B on-release `n_batch` / `n_inner`). Runtime handler mirrors Sprint 1B wind_dir pattern: sync preset → `invalidate_from(ErosionOuterLoop)` → `run_from(ErosionOuterLoop)` → overlay refresh. |
| `f4a55b3` | 2.0 | Doc-drift audit: README / ARCHITECTURE / CLAUDE / sim rustdoc / PROGRESS synced 17-stage → 19-stage wording. Historical RECENTLY SHIPPED table rows preserved per §9. |
| `d4002b6` | 2.9 | `coast_type_well_formed`, `erosion_no_explosion`, `erosion_no_excessive_sea_crossing` — 3 new pipeline-tail invariants. Skip-if-missing semantics preserve 1A/1B-only pipeline behaviour. Constants `EROSION_MAX_GROWTH_FACTOR = 1.05`, `EROSION_MAX_SEA_CROSSING_FRACTION = 0.05`. `full_sprint_2_pipeline_passes_all_11_invariants` integration test. +9 tests. |
| `cfd80ee` — `75acf22` | 2.1–2.7 audit follow-ups | 6 `refactor:` / `fix:` commits from the retroactive simplifier + code-reviewer chain. Caught 3 Critical (CoastType transparency, Runtime pipeline drift, schema_version hardcoding) + 7 Important + polish items. See CURRENT FOCUS above. |
| `f5cb6e1` | 2.6 | 3 golden-seed snapshots regenerated for post-erosion state. |
| `f2eef1b` | 2.6 | `crates/data/golden/headless/sprint_2_erosion/` first shipped — 6 shots at seed 42 (3 presets × pre/post). |
| `ba02975` — `f62c8c7` | 2.6A | 4 new `SummaryMetrics` fields (`erosion_relief_drop_fraction`, `coast_type_counts[4]`, `erosion_sea_crossing_count`, `coast_type_blake3`) + cascade regen of all 3 baselines. |
| `6f3f4ba` — `ab7d5b5` | 2.6B | K calibration tune 1e-3 → 1.5e-3 (grid-size-safe empirical ceiling) + CoastType threshold tune 0.30/0.18/0.05/0.30 → 0.07/0.04/0.02/0.05 + cascade regen of all 3 baselines. |

**Sprint 2 plan key decisions (locked):**
- **SPIM `(m, n) = (0.35, 1.0)`** per KP17 safety margin (stays well
  away from `m/n = 0.5` pathological regime). Not tunable via
  preset because `n ≠ 1` requires implicit solver (Sprint 4).
- **`K = 1.5e-3`** as the locked v1 default (bumped from Sprint 2.1's
  initial 1e-3 after empirical Pareto probe). `K = 2e-3` is unsafe
  on 64² grids (sea-crossing tips to 5.09 %); `K = 3e-3` is unsafe
  on 128² caldera (5.19 %). Any future bump must verify on ALL
  grid sizes in the test suite.
- **`ErosionOuterLoop` scheme B** (single variant, internal
  iteration, holds stage refs for the Coastal..RiverExtraction
  re-run chain) per Sprint 1D 1D.3 memo. α-position (between
  RiverExtraction and Temperature) so 1B climate/ecology
  automatically reads post-erosion state.
- **`derived.erosion_baseline` is sticky** across slider reruns —
  only `invalidate_from(Topography)` clears it, so
  `erosion_no_explosion` / `erosion_no_excessive_sea_crossing`
  always compare against the true pre-erosion state.
- **CoastType v1 cheap proxies** (slope + river_mouth +
  shoreline_normal·wind exposure + island_age). v1 threshold
  values `0.07/0.04/0.02/0.05` are the post-2.6B tune; spec's
  initial `0.30/0.18/0.05/0.30` fired 0 % Cliffs because coastal
  slopes rarely exceed 0.07 at safe K. Cliff bin still at 0 %;
  Beach + RockyHeadland ≥ 25 % each; Estuary bounded by actual
  river-mouth count.
- **`CaptureRequest` schema v2** adds `preset_override` as an
  optional (`#[serde(default)]`) field, keeping v1 request files
  parse-compatible. `RunSummary.schema_version` mirrors the input
  version so v1 baselines continue to exit 0 under v2 binary.
- **Runtime uses `sim::default_pipeline()`** only; no local
  pipeline builders allowed in `app` / `ui` (Sprint 2.7 audit
  finding — the local `build_sprint_1b_pipeline()` had silently
  drifted out of lockstep with StageId since 2.3 / 2.4).

**Acceptance gap residuals deferred to Sprint 3** (see DEFERRED):

1. §10 clause "max_z 下降 10–30 %" — measured 0.19 / 1.54 / 1.52 %
   at K=1.5e-3. Physically incompatible with the 5 %
   sea-crossing invariant under uniform SPIM; larger peak erosion
   is a sediment-aware `K·g(hs)` problem (Sprint 3) or an
   elevation-band-weighted K (out of scope for v1).
2. §11 open #3 "CoastType 每 type 5 %" — Cliff at 0 % across all
   3 presets because no coastal cell has "slope > 0.07 AND
   windward exposure" simultaneously. Natural fix in Sprint 3
   (sharper cliffs form with sediment-aware erosion) or a
   coast_type v2 classifier with fetch-integral wave exposure.

**Invariants preserved:** `cargo tree -p core` still clean (no
`sim` / graphics deps); `WorldState` 3-layer structure unchanged
(new fields land in `derived` only); `ScalarField2D<T>` + aliases
unchanged; descriptor-based overlay system preserved; string keys
still confined to `render/src/overlay.rs`; all 8 architectural
invariants green. 11 pipeline-tail invariants now fire at
`ValidationStage`.

---

### Sprint 1D — Pre-Sprint-2 Cleanup & Erosion Prep (2026-04-18, 3 commits on `dev`)

**Doc:** [`docs/design/sprints/sprint_1d_pre_sprint_2_cleanup.md`](docs/design/sprints/sprint_1d_pre_sprint_2_cleanup.md) (Obsidian symlink, gitignored)
**Test delta:** 327 → 335 passing (+8 new invalidation tests), 5 ignored unchanged.

| Commit | Task | What shipped |
|---|---|---|
| `5cef5c7` | 1D.2 | `sim::invalidate_from(world, StageId)` free function in `crates/sim/src/invalidation.rs`. Per-`StageId` match over `clear_stage_outputs` lockstep (adding a new StageId variant forces adding an arm). Default frontier `StageId::Coastal` for `authoritative.height` mutation. +8 tests: Topography full-wipe, Accumulation upstream-preservation, and frontier-parameterized bit-exact equivalence across Coastal / PitFill / DerivedGeomorph / Precipitation / BiomeWeights / HexProjection. |
| `b2fc274` | 1D.1 | Doc-drift audit: 9 fixes across README, ARCHITECTURE, CLAUDE, sim rustdoc. Canonical wording "17-stage canonical pipeline (16 StageId variants + terminal ValidationStage)" applied uniformly. ARCHITECTURE.md §4 builder-location factual fix (`app/runtime.rs` → `sim::default_pipeline()`). |
| [this commit] | 1D.3 + 1D.4 + close-out | `ErosionOuterLoop` scheme B memo locked in sprint plan §3 Task 1D.3 (no code). `crates/core → crates/ipg-core` rename deferred with 3 re-visit triggers recorded in PROGRESS DEFERRED. CURRENT FOCUS rolled to Sprint 1D. |

**Sprint 1D plan key decisions (locked):**
- **`invalidate_from` crate location:** `sim`, not `core`. `StageId` is a sim enum; the `StageId → derived/baked fields` mapping is pipeline policy. Putting the helper on `impl WorldState` would require `core → sim` reverse dep (violates Sprint 0 crate DAG).
- **Default invalidation frontier for `authoritative.height` mutation:** `StageId::Coastal`. Height mutations may move cells across the `sea_level` threshold, so `coast_mask` / `shoreline_normal` and every downstream stage (PitFill's `z_filled`, DerivedGeomorph's slope/curvature, the whole hydro + climate + ecology + hex chain) are potentially stale. Optimising to `StageId::PitFill` requires empirical proof that a specific mutation doesn't cross sea level — explicitly out of scope for Sprint 1D and Sprint 2, earliest reconsidered at Sprint 3 sediment.
- **`ErosionOuterLoop` form:** scheme B (single `StageId::ErosionOuterLoop` variant, internal iteration, holds stage refs for the Coastal..RiverExtraction re-run chain). Scheme A (flat StageId expansion of 10×10) rejected — loses preset-driven `N_batch` tunability. Scheme C (separate `IterationDriver`) rejected — doubles executor complexity for visibility that Sprint 2 doesn't need. Enum insertion position (post-RiverExtraction vs post-DerivedGeomorph) deferred to Sprint 2 Task 1 based on what stream-power actually reads.
- **`crates/core` rename:** deferred. Re-visit on (a) Sprint 4 introducing more core-crate splits that amplify alias churn, (b) any crates.io publishing decision, (c) a broader cross-crate refactor where bundling the rename is cheaper than doing it standalone.

**Invariants preserved:** `cargo tree -p core` clean (no `sim` edge); `WorldState` 3-layer structure untouched; `authoritative.*` / `seed` / `preset` / `resolution` never mutated by `invalidate_from`; all 8 architectural invariants green. Sprint 1A + 1B `--headless` baselines still exit 0 on `--headless-validate --against`, confirming no pipeline regression.

---

### Sprint 1C — Headless Validation & Offscreen Capture (2026-04-17, 10 commits on `dev`)

**Doc:** [`docs/design/sprints/sprint_1c_headless_validation.md`](docs/design/sprints/sprint_1c_headless_validation.md)
**Test delta:** 270 → 327 passing (+57), 5 ignored (all pass locally on Metal).

| Commit | Task | What shipped |
|---|---|---|
| `060b778` | 1C.1 | `CaptureRequest` RON schema + 4 round-trip tests |
| `bd082f7` | 1C.3 | CPU overlay bake factored to `render::overlay_export::bake_overlay_to_rgba8`; +1 determinism test |
| `abbc298` | 1C.4 | `RunSummary` / `ShotSummary` / `TruthSummary` / `BeautySummary` / `OverallStatus` (5 variants) / `InternalErrorKind` (9 variants) + `canonical_bytes` / `compute_run_id` / `compute_request_fingerprint` / `RunLayout` I/O helpers; 21 tests |
| `7ec7a88` | 1C.5 | `GpuContext::new_headless` + `capture_offscreen_rgba8` with 256-aligned row-pitch readback; `HEADLESS_COLOR_FORMAT` const; 1 regular + 2 `#[ignore]` GPU tests |
| `ab0828c` | 1C.2 | `--headless` + `--headless-validate --against` flag routing on `main.rs`; `ExitCode` return; `OverallStatus::exit_code()` mapped to AD9 process exits; routing test |
| `62f2b4a` | 1C.6 | Executor: `run_request` + `run_shot` + `render_beauty_shot`; typed `ShotError`; AD8 GPU-bootstrap with `IPG_FORCE_HEADLESS_GPU_FAIL` hook; shared helpers `sim::default_pipeline`, `data::golden::SummaryMetrics::compute`, `render::camera::preset_by_name` |
| `71822b4` | 1C.8 | `headless::compare` — AD5 three-step diff (shape → truth hash → beauty artifact-only); `validate` returns `Result<(OverallStatus, Vec<String>)>`; +13 tests |
| `bd35071` | 1C.9 + 1C.10 | Checked-in baselines: `sprint_1a_baseline/` (9 shots) + `sprint_1b_acceptance/` (9 shots); PNGs gitignored; self-validates exit 0 on both |
| `dc70e18` | 1C.11 | CI non-blocking steps run `--headless` + `--headless-validate` against both baselines on `macos-latest` |

**Sprint 1A 9-shot golden visual baseline** (long-deferred since
2026-04-14) is now first-shipped via 1C.9 — no seed-cycling UI
required; the headless harness drives it directly.

**Invariants:** `cargo tree -p core` clean; descriptors-not-closures
preserved (`bake_overlay_to_rgba8` takes `&OverlayDescriptor`);
string keys still confined to `render/src/overlay.rs`; `core::save`
untouched. All 8 architectural invariants green.

---

### Sprint 1B — Climate + Ecology closed loop (2026-04-17, 14+2 commits on `dev`)

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
| `9818e8b` | §10 visual acceptance — window title Sprint 1A → 1B | §10 | 269 (same) |
| `cefd530` | §10 visual acceptance — wind→biome re-run regression guard | §10 | 269 → 270 |

**Sprint 1B visual spec clarification (2026-04-17):** Pass 3 of the
visual acceptance (`docs/design/sprints/sprint_1b_visual_acceptance/INDEX.md`)
originally shot the **Dominant biome** overlay at wind=0 and wind=π
and expected a mirror flip. Actual capture pair rendered nearly
identically. Investigation via the new
`wind_dir_rerun_propagates_through_biome_chain` test confirmed the
pipeline IS correct — `precipitation`, `fog_likelihood`,
`soil_moisture`, `biome_weights`, and `dominant_biome_per_cell` all
mutate on `run_from(Precipitation)`. Root cause of the visual
identity: only ~3 % of land cells flip biome argmax under a 180°
wind swing, because the 8-biome categorical argmax is dominated by
wind-invariant inputs (`z_norm`, `slope`, `river_mask`). Pass 3 was
retargeted to the **Soil moisture** overlay (far more wind-
sensitive — max moisture delta 0.23) which captures the propagation
proof viscerally. The pipeline-level regression guard replaces the
visual `dominant_biome` probe with a deterministic byte-level
assertion, so future `run_from` breakage fires at the test
boundary rather than via human-eyeballed screenshots.

**StageId enum is the single source of truth** for pipeline indices.
The 18-variant enum (`Topography = 0` … `HexProjection = 17`) is
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
in `sim::validation_stage::tests` builds the complete 19-stage
pipeline (18 StageId variants + tail ValidationStage) on a `volcanic_preset` at
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

**From Sprint 2.5 close-out (new — 2026-04-18):**

- ~~**2.5.I — Blue-noise dither A/B visual validation.**~~
  **ABSORBED INTO Sprint 2.6.D (2026-04-19).** The decision memo
  lives at `docs/design/sprints/sprint_2_6_visual_acceptance/dither_ab_decision.md`
  after Sprint 2.6's live-window A/B session; the `DITHER_ON` uniform
  + Camera-panel toggle is Sprint 2.6.D scope (not scope creep anymore
  — the full sprint is structured around closing this decision). See
  Sprint 2.6 plan doc §3 Task 2.6.D.
- ~~**2.5.L — Blue-noise runtime size toggle (64 / 128 / 256).**~~
  **ABSORBED INTO Sprint 2.6.E (2026-04-19).** Three PNG assets
  (`blue_noise_2d_64.png`, `blue_noise_2d_128.png`, `blue_noise_2d_256.png`)
  already present in `assets/noise/`; `render::noise::load_blue_noise_2d`
  is already size-generic (no loader changes needed). 2.6.E is a
  Camera-panel ComboBox + runtime texture hot-reload only, **gated on
  2.6.D keep-dither**. Decision-tree endings (Sprint 2.5.L lock-rule
  compliant — no "future-proofing" exemption):
  - 2.6.D **keeps** dither → 2.6.E executes; all three PNGs retained
    as active consumers.
  - 2.6.D **drops** dither → 2.6.E closes "n/a via upstream 2.6.D";
    `blue_noise_2d_64.png` retained (still loaded by
    `overlay_render.rs:122` for overlay dither), `blue_noise_2d_128.png`
    + `blue_noise_2d_256.png` **deleted from repo** in the same commit
    that removes the terrain dither branch. If blue-noise ever returns
    to the terrain path, re-download from Calinou is a one-liner.
- **2.5.Jb — Climate constant tuning (`CONDENSATION_RATE` /
  `RAIN_SHADOW_K` / other `climate/` constants).** Explicit scope
  split inside Sprint 2.5: only ecology bells were tuned; climate
  constants are deferred to Sprint 3 LFPM v3 which supersedes the
  upwind raymarch entirely. A v1.5 constants pass would be throwaway
  ahead of v3. Sprint 3's precipitation design doc must explicitly
  address whether LFPM v3 covers the "moisture swing not strong
  enough" symptom or whether a v1.5 tune is still needed.
- **2.5.F deviation — `volcanic_twin_old` `n_batch`.** Sprint doc
  specified `n_batch: 15` for the "more eroded look" on this
  archetype; empirical test showed 15 trips the
  `erosion_no_excessive_sea_crossing` 5 % invariant at the safe
  K=1.5e-3. Preset ships with default `n_batch=10` instead. Sprint
  3 sediment-aware `K · g(hs)` modulation unlocks higher n_batch
  because g(hs) damps coastal erosion where sediment pools.
- **2.5.D / 2.5.B — `HexDebugAttributes` is prototype only.**
  Sprint 5 S2 (settlement / road / WFC) will redesign the
  `accessibility_cost` contract when it becomes a real consumer.
  The Sprint 2.5 overlay shows that the formula produces
  distinguishable values (flat ~1, cliff-coast 10+) — the
  production-quality contract is deferred.
- **2.5 CoastalScrub + CloudForest coverage.** After 2.5.Ja tuning
  they remain at 0 % on `volcanic_single` (dry interior + no fog).
  Not a regression — the bells will produce both biomes on wetter /
  foggier archetypes. A synthetic-env kernel test per biome would
  defend against inadvertent kernel narrowing but wasn't in scope.
  Sprint 3 climate v3 (LFPM + fog hydrology) gets the richer
  moisture / fog domains that should make these biomes visible on
  `volcanic_single` too.
- **2.5.G — Basin CC promotion is dormant on real terrain.** The
  post-BFS CC pass activates only when `FLOW_DIR_SINK` land cells
  survive past the end of `ErosionOuterLoop` → PitFill cycle. On
  the current Sprint 2 pipeline PitFill eliminates every interior
  depression, so the `basin_partition` overlay hash is bit-identical
  pre- vs post-2.5.G. Sprint 3 sediment-aware SPACE-lite may
  intentionally leave deposition-lakes unfilled (valley floors,
  caldera lakes), at which point the promotion fires automatically
  + labels them as fresh basins.

**From Sprint 2 close-out (still pending — inherited by Sprint 3):**

- **§10 "max_z 下降 10–30 %"** across 3 presets. Measured
  0.19 / 1.54 / 1.52 % at the safe K=1.5e-3 calibration (close to
  the 5 % sea-crossing ceiling on caldera). Spec DD1's 18 %
  projection was physically incompatible with the
  `erosion_no_excessive_sea_crossing` invariant under uniform SPIM:
  reaching 18 % peak drop requires K ≈ 0.18 (180× default), which
  scales coastal erosion proportionally and shatters the invariant.
  **Sprint 3 anchor point:** sediment-aware `Ef = K · A^m · S^n ·
  g(hs)` with `g(hs) = exp(-hs/H*)` damps coastal erosion where
  sediment pools (alluvial fans / valley floors), unlocking larger
  peak K without breaking the invariant. Chen 2014 §4 is the
  reference.
- **§11 open #3 CoastType "每 type 5 % 占比"** per preset × hero
  shot. Cliff bin at 0 % across all 3 presets after 2.6B tune to
  `S_CLIFF_HIGH=0.07 / EXPOSURE_HIGH=0.05`. Root cause: coastal
  slopes rarely exceed 0.07 because Sprint 2 erosion is too gentle
  to carve steep windward faces. Estuary bounded by actual
  river-mouth count (~3-5 per preset, physical limit). **Sprint 3
  anchor points:** (a) sediment-aware erosion creates sharper
  coastal cliffs naturally; (b) coast_type v2 classifier with
  fetch-integral wave exposure (16-direction wave fetch, not a
  single shoreline_normal dot-product) per sprint doc §11 open #2.

**Absorbed by Sprint 2.5 (close this section):**

- ~~**1A tail — flow accumulation overlay log-compression audit.**~~
  **SHIPPED in 2.5.H** (`4dc75ed`): P90/max = 0.023 confirmed the
  washout; new `ValueRange::LogCompressedClampPercentile(0.99)` variant
  fixes it without palette churn.
- ~~**1B 16-shot visual acceptance full migration.**~~ **SHIPPED in
  2.5.E** (`554f4f7`): 9 → 15 shots, schema v2 `preset_override`
  path exercised, byte-level wind propagation locked. Shot
  `01_baseline_camera_overlays_panels` permanently excluded (UI
  state non-serialisable; stays as manual reference).
- ~~**1B tail — biome suitability parameter tuning.**~~ **SHIPPED
  in 2.5.Ja** (`93f7c5b`): `volcanic_single` 3 biomes → 5 biomes
  ≥ 3 % coverage; wind 180° swing ~14 % argmax flip.
- ~~**1B tail — T2 per-descriptor alpha slider.**~~ **SHIPPED in
  2.5.K** (`1073f4e`): `OverlayDescriptor.alpha: f32` field +
  OverlayPanel row-per-descriptor + per-frame uniform upload.
- ~~**1B tail — T3 blue-noise runtime size toggle.**~~ Not shipped —
  gated on 2.5.I's keep-vs-remove decision; see "From Sprint 2.5
  close-out" above.

**From Sprint 1B close-out (still pending):**

- **Sprint 1B paper pack** — `docs/papers/sprint_packs/sprint_1b.md`
  Bruijnzeel 2005 / 2011 notes, Chen 2023 Budyko writeup, and Core
  Pack #2/#3/#5/#6/#8 "Sprint 1B 落地点" sections. Non-blocking per
  §7; tackle in a low-energy session.
- **Slider cadence measurement.** Re-run cost at 256² is now larger
  than Sprint 1B's estimate (ErosionOuterLoop adds 10×10 inner
  iterations per re-run). The 2026-04-18 acceptance session felt
  responsive at default n_batch/n_inner in practice; no profiling
  numbers captured yet. Sprint 2.5 is the natural slot to measure
  + decide whether Tier A / Tier B erosion-slider throttling (§5)
  needs tightening.

**From Sprint 1D close-out:**

- **`crates/core` → `crates/ipg-core` rename (Task 1D.4).**
  Considered during Sprint 1D pre-work cleanup, **explicitly
  deferred**. The rename would eliminate the `::core` stdlib
  shadowing that forces downstream crates to use
  `island_core = { path = "../core", package = "core" }` aliases
  and keeps `[lib] doctest = false` on `crates/core/Cargo.toml`.
  It is zero-risk refactoring (pure rename, CI gate catches any
  missed alias) but cross-cuts ~8 `Cargo.toml` files + ~30-50
  `use island_core::` sites. Re-visit triggers:
  1. Sprint 4 adds more `crates/core`-splits (e.g. `core::save`
     spun out) and the alias churn compounds.
  2. Any decision to publish to crates.io (rename becomes
     mandatory then).
  3. A cross-crate refactor with enough scope that bundling the
     rename in is cheaper than doing it standalone.
  Until one of those fires, the alias + `doctest = false`
  workaround stays. CLAUDE.md Gotchas already documents the
  shadowing trap so new contributors don't walk into it.

---

## DEVELOPMENT

### Sprint 1C — Headless Validation & Offscreen Capture
**Status:** **Closed on `dev` 2026-04-17.** 10 atomic commits
(060b778 → dc70e18). **327 tests** across 8 crates (+57 from Sprint
1B's 270 baseline; 5 ignored GPU tests pass locally on Metal).
`--headless` and `--headless-validate --against` both live on macOS
Metal. Sprint 1A 9-shot golden baseline first-shipped via 1C.9.
Baseline acceptance host: Apple Silicon + macOS Metal (AD10).
**Doc:** [`docs/design/sprints/sprint_1c_headless_validation.md`](docs/design/sprints/sprint_1c_headless_validation.md)
See [CURRENT FOCUS](#current-focus) and [RECENTLY SHIPPED](#recently-shipped)
for the per-task + per-commit breakdown.

### Sprint 1B — Climate + Ecology closed loop
**Status:** **Closed on `dev` 2026-04-17.** 14 atomic commits +
2 §10 close-out commits (window title + regression guard). **270
tests** across 8 crates (+82 from Sprint 1A's 188 baseline).
Wind-direction slider wired end-to-end (`ParamsPanel →
Runtime::tick → pipeline.run_from(StageId::Precipitation) →
OverlayRenderer::refresh`) and visually verified against the
16-shot acceptance capture pass. Pass 3 retargeted from
`dominant_biome` to `soil_moisture` overlay after investigation
(see RECENTLY SHIPPED for the full write-up and the regression
test that replaced the visual probe).
**Doc:** [`docs/design/sprints/sprint_1b_climate_ecology.md`](docs/design/sprints/sprint_1b_climate_ecology.md)
See [RECENTLY SHIPPED](#recently-shipped) for the per-task + per-commit breakdown.

### Sprint 1A — Terrain + Water Skeleton
**Status:** §3.2 Visual Package complete (A1–A6 + B3 all shipped on
`dev` as of 2026-04-14). 16-shot validation captured + audited; Pass
3.1 post-fix landed for preset framing. The 9-shot golden baseline
(long-deferred since 2026-04-14) shipped in Sprint 1C via 1C.9 —
see `crates/data/golden/headless/sprint_1a_baseline/`.
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

**Residuals from Sprint 1A §7:**
- **9-shot golden visual baseline** — shipped in Sprint 1C via 1C.9
  (`crates/data/golden/headless/sprint_1a_baseline/`). No seed-cycling
  UI was needed; the headless harness drives it directly.
- **Paper pack §6:** Chen 2014 + Génevaux 2013 deep reads, Lague 2014
  target-deep — still outstanding; tackle in a low-energy session.

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

Sprints 1A, 1B, 1C, 1D, 2, 2.5, and 2.6 are shipped. Upcoming work
starts at Sprint 3. Per-sprint plan docs are written **one at a time**
after the previous sprint closes — the roadmap carries the forward-
looking vision until each sprint's doc gets authored.

| Sprint | Focus | Source of truth |
|---|---|---|
| 3 | Sediment v1 + SPACE-inspired dual-equation erosion with `K·g(hs)` modulation (unlocks Sprint 2's deferred "max_z drop 10-30 %" + CoastType Cliff bin), LFPM v3 precipitation, cloud-forest inversion, Coast v2 (fetch integral + LavaDelta). Sprint 2.6 delivered the interactive tuning surface (dock layout + World panel preset/seed/aspect switching + Fuji-like world aspect) that makes Sprint 3's Pareto-probe in-window work pleasant. | Roadmap §Sprint 3 |
| 4 | `crates/gpu/` + `ComputeBackend` refactor, 5 GPU passes, CLI productization (`island-gen`), parity framework, implicit SPIM (Braun 2023) | Roadmap §Sprint 4 |
| 5 | Four subsystems: S1 Hex, S2 Semantic (rule-based + WFC stretch), S3 Web (trunk, curated subset), S4 Demo/Article/Gallery | Roadmap §Sprint 5 |

---

## ON ICE

Nothing paused.

---

## QUICK REFERENCE

**High energy?** → Start Sprint 2.5. The Hex UX slice gives the
clearest visible payoff — `HexOnly` / `HexOverlay` / `Continuous`
view toggle + the `coast_type` and `dominant_biome` overlays both
gain a hex-tile render path. The `preset_override` schema v2 from
Sprint 2 now unblocks migrating the 6 wind-varying shots of the
1B 16-shot visual acceptance into `crates/data/golden/headless/`,
and the 3 new archetypes (`volcanic_caldera_young`,
`volcanic_twin_old`, `volcanic_eroded_ridge`) exercise the Sprint
2 erosion system on more varied terrain than the existing 3
stock presets. Sprint 2.5 doc:
`docs/design/sprints/sprint_2_5_hex_ux_and_tail.md`.

**Medium energy?** → Sprint 1B / 2 tail UI polish: T2
per-descriptor alpha slider for the 13 overlays, T3 blue-noise
runtime size toggle. Or a biome suitability tuning pass to unlock
more than 3 biomes on `volcanic_single` (Task 1B.9 has the slider
hooks). Or measure `run_from(ErosionOuterLoop)` wall-clock under
the Sprint 2 erosion sliders — the 2026-04-18 acceptance felt
responsive but no ms numbers captured.

**Low energy?** → Sprint 1B paper pack. Create
`docs/papers/sprint_packs/sprint_1b.md` per sprint doc §7:
Bruijnzeel 2005 / 2011 TMCF notes, Chen 2023 Budyko readthrough,
and Core Pack #2/#3/#5/#6/#8 "Sprint 1B 落地点" sections pointing
back at DD2 / DD4 / DD6 anchor points. Also fill the Sprint 1A
Chen 2014 / Génevaux 2013 deep reads still outstanding at
`docs/papers/core_pack/`.

**Quick win?** → Tune `suitability.rs` parameters so more than 3
biomes appear in `volcanic_single`. Current output collapses onto
Grassland / BareRockLava / Riparian. Widen the σ on LowlandForest
and MontaneWetForest bells, or lower the `soil_moisture`
thresholds. Task 1B.9 added the slider hooks; the `--headless`
harness now makes parameter sweeps scriptable.

---

**Update this file whenever a sprint ships, scope shifts, or a blocker moves.
Weekly minimum during active sprints.**
