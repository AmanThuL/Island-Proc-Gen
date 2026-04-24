# PROGRESS

**Last Updated:** 2026-04-24 (Sprint 3.5 Hex Surface Readability closed on
`dev` — 31 commits `6c0059f → a2992c5`; `cargo test --workspace` 528 → 618
passing / 8 ignored (net +90 A–F); 4 existing baselines per-task regen +
1 first-shipped `sprint_3_5_hex_surface/` at `schema_version: 3` (27 shots,
3 view modes × 3 archetypes × 3 seeds). DD1 flat-top / DD2 axial-offset
kernel / DD3 6-edge river / DD4 `HexCoastClass` / DD5 surface contract /
DD6 bounded G7 retune / DD7 pick + `HexInspectPanel` / DD8 schema v3 all
shipped. **Sprint 4 Compute Productization** is now active-next.)

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

**Shipped-sprint details older than the last two live in
[`docs/history/progress_archive_milestone_1.md`](docs/history/progress_archive_milestone_1.md)**
(Obsidian symlink, gitignored — same pattern as `docs/design/`; resolvable on
the author's machine only, cold-storage reference). Active-next sprint brief
is at [`docs/design/sprints/sprint_3_5_hex_surface_readability.md`](docs/design/sprints/sprint_3_5_hex_surface_readability.md)
(also a vault symlink; empty until Sprint 3.5 starts).

---

## CURRENT FOCUS

**Primary:** Sprint 4 — Compute Productization (*active-next*). `crates/gpu/`
+ `ComputeBackend` refactor, GPU passes (hillslope / rainfall-proxy-v2 /
hex-projection + stream-power / flow-accumulation), `island-gen` CLI
productization, CPU/GPU parity harness, artifact-system maturity.
Physical-unit calibration closes §10 G4. Sprint 3.5's forwarded G5 (Cliff
coverage) and G7 CoastalScrub foothold land here too — the DD4 empirical
escape was triggered at 3.5 close-out; slopes need the Sprint 4 unit-flux
rework to sharpen before Cliff thresholds fire. Does NOT do first-pass
beauty, semantic layer, or re-shape hex readability.

Sprint 3.5's closed contracts (frozen in CLAUDE.md Gotchas §Sprint 3.5):
flat-top hex convention; DD2 axial-offset aggregation kernel; 6-edge
`HexEdge` numbering `E=0/NE=1/NW=2/W=3/SW=4/SE=5`; `HexCoastClass`
placement in `core::world` (crate-DAG constraint); DD6 `coastal_margin`
SM floor `COASTAL_MARGIN_MAX_DIST=3` + floor 0.25; CloudForest
`T_PEAK=18 / T_SIGMA=6`; DD7 off-grid-clicks-are-no-op + read-only
`HexInspectPanel`; DD8 schema_version 3 + optional `view_mode` on
`CaptureShot`; `render_stack_for(ViewMode)` parity tier-1 gate.

Sprint 4's plan doc is not yet authored; the empty placeholder file at
`docs/design/sprints/sprint_4_*.md` will be written when Sprint 4 starts.
Until then, the roadmap §Sprint 4 entry carries the forward-looking intent.

**Last shipped:** Sprint 3.5 Hex Surface Readability (2026-04-24, 31
commits A→F). **Previous:** Sprint 3.4 Module Boundary Cleanup
(2026-04-23, 5 commits). Both full tables below in RECENTLY SHIPPED.

---

## RECENTLY SHIPPED

### Sprint 3.5 — Hex Surface Readability (2026-04-24, 31 commits on `dev`)

**Doc:** [`docs/design/sprints/sprint_3_5_hex_surface_readability.md`](docs/design/sprints/sprint_3_5_hex_surface_readability.md) (Obsidian symlink, gitignored)
**Test delta:** 528 → 618 passing / 8 → 8 ignored (net +90 across A–F; includes +6 pixel_to_axial edge-case tests at 3.5.E c1, +2 screen_to_picked_hex at 3.5.E c2, +2 HexInspectPanel at 3.5.E c3, and validator + value-lock additions across A/B/C/D). Commits `6c0059f → a2992c5` (spanning 3.5.A schema lift through 3.5.F close-out).
**Close-out status:** DD1–DD8 all shipped; DD6 bounded G7 retune reached CloudForest foothold (met); G5 Cliff coverage and G7 CoastalScrub foothold both forwarded to Sprint 4 per DD4 + Q4 empirical escape — slope sharpening and θ gating structure require Sprint 4's physical-unit calibration, not in-scope for a hex-readability sprint.

Sprint 3.5 was the first sprint where hex became a readable **final**
surface rather than a debug slice: true axial-offset aggregation, 6-edge
river continuity, 5-class hex coast grammar, dominant-surface contract,
and interactive pick + read-only inspect panel.

| Task | What shipped | Key commits |
|---|---|---|
| 3.5.A (DD1/DD2/DD5/DD8) | DD8 `SummaryMetrics` extension (`hex_attrs_hash` + `hex_debug_river_crossing_hash` + `hex_coast_class_hash`) and `CaptureShot.view_mode`; `schema_version: 2 → 3`; 4 existing baselines schema-lift roll-in with real pre-3.5 hashes (no semantic truth change). True-hex axial-offset aggregation kernel in `build_hex_grid` with witnessed `hex_attrs_hash` delta. `HexSurfaceRenderer` with procedural unit-hex VB + per-instance buffer, tonal-ramp elevation cue per DD5. Runtime wiring + frame.rs/executor.rs `render_stack_for(ViewMode)` parity. DD8 `schema_v1_and_v2_still_parse_under_v3_binary` regression test + `HexAttributes` 8-field exhaustive-destructure compile-time lock. | `6c0059f` → `6d292ef` (11 commits including regen) |
| 3.5.B (DD3) | `HexRiverCrossing` edges promoted from 4-box-edge (Sprint 2.5) to 6-hex-edge encoding (`E=0 … SE=5`). Per-hex `RiverWidth` bucket from `max(flow_accumulation)`. `HexRiverRenderer` with edge-to-edge polyline/spline. Witnessed by `hex_debug_river_crossing_hash` in 4 baselines' regen. | `1d0d7e9` → `7c508ac` (4 commits) |
| 3.5.C (DD4) | `derived.coast_fetch_integral` persisted by `CoastTypeStage` (no duplicate raycast). `HexCoastClass` enum in `core::world` (7 variants: `Inland/OpenOcean/Beach/RockyHeadland/Estuary/Cliff/LavaDelta`) + `sim::hex_coast_class` classifier written by `HexProjectionStage`. Hex-edge decoration vocabulary per DD4 (render-side). Validators `hex_coast_class_well_formed` + `hex_coast_class_requires_fetch_integral`. | `f3a01f9` → `bc1a7c1` (4 commits) |
| 3.5.D (DD5/DD6) | `coastal_margin` SM floor in `SoilMoistureStage` (Von4≤3 land → θ≥0.25; `COASTAL_MARGIN_MAX_DIST=3` named const). CloudForest `f_t` envelope widening (`T_PEAK 15→18`, `T_SIGMA 4→6`). DD5 dominant-surface contract locked + overlay-vs-base-read policy. Validators `coastal_margin_sm_floor_applied` + `cloud_forest_f_t_envelope_matches_sprint_3_5_lock`. | `c933c8b` → `35f5726` (4 commits) |
| 3.5.E (DD7) | `pixel_to_axial` inverse math + 6 pick-critical edge-case tests (vertex / edge-midpoint / negative-axial / degenerate-grid / odd-row-right-edge / shipping-hex_size). Click-handler in `runtime/events.rs`: ray → sea plane → `pixel_to_offset` with click-vs-drag discrimination (`CLICK_DRAG_THRESHOLD_PHYS_PX=3.0` Manhattan). DD7 "off-grid clicks → no-op" enforced (reviewer-caught latent bug during commit). `HexInspectPanel` egui_dock tab: read-only two-column grid, 11 attrs per DD7 schema; pre-3.5 layouts fall back via existing `dock.rs` failed-parse path. | `9a4e7d9` → `776bca7` (3 commits) |
| 3.5.F | 5th `--headless` baseline `sprint_3_5_hex_surface/` at `schema_version: 3`: 3 archetypes × 3 seeds × 3 view modes = 27 shots, all `overall_status: Passed`, truth-path bit-identical across view_modes for the same `(seed, preset)`. CLAUDE.md Gotchas §Sprint 3.5 subsection + PROGRESS.md close-out (this commit). | `a2992c5` → this commit |

**§10 Acceptance verdicts at 3.5 close:**

- **G5 CoastType v2 Cliff coverage** → **forwarded to Sprint 4** (DD4 Q4 empirical escape triggered). Sprint 3.1 probes had already ruled out threshold-tuning; hex-edge decoration grammar (3.5.C c3) is in place but cell-level Cliff discrimination needs Sprint 4's physical-unit K / H* calibration to sharpen slopes.
- **G7 CloudForest foothold** → **met** (foothold > 0% on ≥ 1 archetype post-DD6 bounded retune; exact per-archetype numbers captured in 3.5.D c2's regen commit msg).
- **G7 CoastalScrub foothold** → **forwarded to Sprint 4** (DD6 `coastal_margin` SM floor raised moisture but CoastalScrub's θ gate + temperature structure requires biome-suitability rework, explicitly out of DD6's bounded scope).

**Verification evidence (captured in close-out commit msgs):**

- `cargo test --workspace` = **618 passed / 0 failed / 8 ignored** (net +90 A–F).
- `cargo clippy --workspace -- -D warnings` green throughout; `cargo fmt --all --check` green.
- `cargo tree -p core` — no `wgpu` / `winit` / `egui*` / `png` / `image` / `tempfile` / `naga` — CLAUDE.md invariant #1 held.
- 5 baselines:
  - 4 existing (`sprint_1a_baseline`, `sprint_1b_acceptance`, `sprint_2_erosion`, `sprint_3_sediment_climate`) regenerated per-task across 3.5.A c2/c5, 3.5.B c2, 3.5.C c2, 3.5.D c2; truth-path diffs confined to the expected per-DD witness hashes at each regen commit.
  - 1 first-shipped (`sprint_3_5_hex_surface/`) with 27 shots at `schema_version: 3`, self-`--headless-validate` exit 0.
- `render_stack_for(ViewMode)` tier-1 parity test (GPU-free) green in every workspace test run. Tier-2 (`IPG_RUN_VISUAL_PARITY=1`) is opt-in and unran at close-out (pending an author-driven session with visual acceptance).
- `HexAttributes` 8-field compile-time lock held (DD1 convention stable for Sprint 5 S2 consumer).

**Handoff to Sprint 4:**

- `derived.coast_fetch_integral` and `derived.hex_coast_class` are now part of the `WorldState.derived` shape; Sprint 4's GPU ports of coast-type + hex-projection must preserve their invalidation arms (`CoastType` for fetch integral, `HexProjection` for hex_coast_class).
- `HexRiverCrossing` 6-edge encoding is the stable contract; any GPU-side river grammar must produce the same `E=0 … SE=5` numbering.
- LFPM v3 precipitation + CloudForest `f_t` envelope are at their Sprint 3.5 values; Sprint 4's physical-unit calibration is expected to re-tune them alongside the K / H* sweep.
- The 5th baseline's 27 shots are the regression reference for Sprint 4's GPU port: if Sprint 4 changes a stage's CPU output, the hash delta per view_mode must be explainable.

### Sprint 3.4 — Module Boundary Cleanup (2026-04-23, 5 commits on `dev`)

**Doc:** [`docs/design/sprints/sprint_3_4_module_boundary_cleanup.md`](docs/design/sprints/sprint_3_4_module_boundary_cleanup.md) (Obsidian symlink, gitignored)
**Test delta:** 528 → 528 passing / 8 → 8 ignored (strictly equal per plan's zero-behavioural-change thesis). Commits `27cf464 → eb93a91` (plus kickoff `cf5d52c` and `.claude` untrack `219a372`).
**Close-out status:** all §5 acceptance criteria met; refactor-only sprint with four `--headless` baselines `summary.ron` bit-identical modulo AD8 whitelist (`timestamp_utc` / `pipeline_ms` / `bake_ms` / `gpu_render_ms` only — truth-path `byte_hash`, `overall_status`, `warnings` unchanged).

Sprint 3.4 was an interstitial structural-cleanup sprint between Sprint 3.1
close-out and Sprint 3.5's hex-surface / Sprint 4's compute-productization
work. It split three high-friction single files into directory modules
grouped by responsibility, introduced the first crate-local `test_support`
module (Pattern A), and re-pointed CLAUDE.md / AGENTS.md invariant #8 at a
single new file (keeping the rule strictly file-scoped).

| Commit | Task | What shipped |
|---|---|---|
| `cf5d52c` | 3.0 | PROGRESS.md Sprint 3.4 roll-forward at sprint start. |
| `219a372` | — | `chore: untrack .claude/` — orthogonal cleanup folded into the sprint's commit range; `.claude/` now gitignored alongside `CLAUDE.local.md`. |
| `27cf464` | 3.4.A | `crates/app/src/runtime.rs` (1378 LOC) → `runtime/{mod,events,frame,regen,view_mode,tabs}.rs`. Visibility unchanged; `Runtime::new` / `handle_window_event` / `tick` / `run_from` stable entry points. No downstream file required edits. |
| `ae9e41e` | 3.4.B | `crates/core/src/validation.rs` (2282 LOC) → `validation/{mod,hydro,climate,erosion,biome,hex}.rs` grouped by invariant family. `mod.rs` holds `ValidationError` enum + `pub use` re-exports; all 16 `island_core::validation::<name>` paths stay byte-identical. No `validate_world` aggregator introduced — orchestration still lives in `sim::ValidationStage::run`. `cargo tree -p core` remains clean. |
| `6929449` | 3.4.C | `crates/render/src/overlay.rs` (978 LOC) → `overlay/{mod,catalog,range,resolve}.rs`. New `SourceKey` enum handle in `resolve.rs`; `catalog.rs`'s 20 descriptors reference overlay sources via `resolve::source_for(SourceKey::…)` rather than embedding raw `&'static str` field keys. `sprint_3_defaults()` returns the same 20 descriptors with bit-identical palettes. **Invariant #8 in CLAUDE.md + AGENTS.md repointed** from `crates/render/src/overlay.rs` to `crates/render/src/overlay/resolve.rs` — still file-scoped. |
| `eb93a91` | 3.4.D | `crates/core/src/test_support.rs` (new) holds a single `test_preset()`; 5 validation family files (`biome`, `climate`, `erosion`, `hex`, `hydro`) now import from it instead of each carrying an identical inline copy (Pattern A per Sprint 3.4 §DD4). `sim::` NOT deduped — its 5 copies differ non-trivially per module. Pattern B (`tests/common/mod.rs`) intentionally deferred. |
| 3.4.F | docs | CLAUDE.md Gotchas §Sprint 3.4 + PROGRESS.md close-out (this commit's predecessor). |

**Verification evidence (captured in commit messages, for future bisection):**

- `cargo test --workspace` = **528 passed / 8 ignored** (strictly equal to pre-3.4 snapshot captured at `219a372`).
- `cargo clippy --workspace -- -D warnings` green; `cargo fmt --all --check` green.
- `cargo tree -p core` — no `wgpu` / `winit` / `egui*` / `png` / `image` / `tempfile` / `naga` — CLAUDE.md invariant #1 held.
- 4 baselines (`sprint_1a_baseline`, `sprint_1b_acceptance`, `sprint_2_erosion`, `sprint_3_sediment_climate`) — `--headless` exit 0 each; `summary.ron` diff fields limited to the AD8 whitelist. Baselines restored to pristine checked-in state before commit.
- `StageId` / `default_pipeline` / `WorldState` layout / crate DAG all untouched.

Sprint 3.4 §10 G4 / G5 / G7 are **untouched** — those remain forwarded to
Sprint 3.5.D and Sprint 4.

**Subagent cadence note:** Sprint 3.4 dispatched 3.4.A / 3.4.B / 3.4.C in
parallel via git-worktree-isolated subagents (per CLAUDE.local.md cadence).
3.4.C's subagent wrote files to the main working tree instead of its worktree
(harness bug or prompt-path ambiguity — unclear); the work product was
reviewed manually, applied in main, and committed. Future parallel subagent
dispatch should include explicit absolute-path anchors in the prompt or run
sequentially if worktree isolation cannot be trusted.

### Sprint 3.1 — Calibration Tail (2026-04-22, 7 commits on `dev`) [ARCHIVED]

*Per "last two live here" policy, Sprint 3.1 details now live only in
[`docs/history/progress_archive_milestone_1.md`](docs/history/progress_archive_milestone_1.md).
Quick summary retained below; expand in the archive for full close-out
attribution.*

**Doc:** [`docs/design/sprints/sprint_3_1_calibration_tail.md`](docs/design/sprints/sprint_3_1_calibration_tail.md) + LFPM diagnosis at [`docs/design/sprints/sprint_3_1_lfpm_diagnosis.md`](docs/design/sprints/sprint_3_1_lfpm_diagnosis.md) (both Obsidian symlinks, gitignored)
**Test delta:** 527 → 527 passing (net 0; +1 new `HS_INIT_LAND` value-lock test offset by reporting granularity). Commits `1fa1e96 → 86f0e7b`.
**Close-out status:** §10 G4 / G5 / G7 all closed **DONE_WITH_CONCERNS** with residuals forwarded to Sprint 3.5.D (biome + coast rework) and Sprint 4 (physical-unit calibration). One real behavioural improvement shipped (LFPM v3 62× precipitation collapse fix, Task 3.1.C.0).

Sprint 3.1 tried to close the three Sprint 3 deferred §10 gates via const-only
retune of already-shipped SPACE-lite / CoastType v2 / Fog-hydrology stages.
Hard physical limits at two of three gates: K can't be raised without
tripping `erosion_no_excessive_sea_crossing` at the smallest test grids;
CoastType v2 Cliff thresholds can't produce Cliffs until slopes sharpen in
Sprint 4; CloudForest / CoastalScrub temperature + θ gating requires
structural biome rework. The LFPM v3 gate was a genuine regression bug
(const miscalibration giving 81 % per-cell fallout) that 3.1.C.0 diagnosed
and fixed.

| Commit | Task | What shipped |
|---|---|---|
| `1fa1e96` | 3.0 | PROGRESS.md Sprint 3.1 roll-forward at sprint start. |
| `7f0be98` | 3.1.A | SPACE-lite K / H* / hs_init calibration probe DONE_WITH_CONCERNS. Three candidates tested; all tripped sea-crossing invariant at some grid size. Retained Sprint 3 defaults; extracted `HS_INIT_LAND` as a named `const`; added 3:1 `K_sed = 3·K_bed` ratio-lock assertions to two test suites. No behavioural change. §10 G4 forwarded to Sprint 4 physical-unit calibration. |
| `01d48e2` | 3.1.C.0 | LFPM v3 precipitation collapse fix — **the one real behavioural improvement**. `TAU_F_DEFAULT 0.60 → 5.0`, `Q_0_DEFAULT 1.0 → 1.3`, `MARINE_RECHARGE_DECAY 0.08 → 0.025`. 81 % per-cell fallout → 18 %; moisture now propagates across the 128² domain. `mean_precipitation` 0.004 → 0.023 (6×); `windward_leeward_precip_ratio` 773 → 27 (28×). Full diagnosis at `sprint_3_1_lfpm_diagnosis.md`. Subsumes Task 3.1.E. |
| `98f513e` | 3.1.C.0 regen | `sprint_3_sediment_climate/` post_* shots + 3 golden_seed_regression snapshots. |
| `51ebd6d` | 3.1.C | Fog + CloudForest bell retune DONE_WITH_CONCERNS. `FOG_WATER_GAIN 0.15 → 0.30`, `FOG_TO_SM_COUPLING 0.40 → 0.60`, `CLOUD_FOREST_SIGMA_FOG 0.08 → 0.15`, `CLOUD_FOREST_FOG_PEAK_WEIGHT 0.30 → 0.40`. Max fog → SM boost 0.06 → 0.18. DryShrub → Grassland shift + MontaneWetForest foothold expanded. CloudForest + CoastalScrub still 0 %. §10 G7 forwarded to Sprint 3.5.D. Task 3.1.D (CoastalScrub bell) SKIPPED per plan §DD3 fallback. |
| `20a05d4` | 3.1.C regen | `sprint_3_sediment_climate/` all shots + 3 golden_seed_regression snapshots. |
| `86f0e7b` | 3.1.F | 4-baseline cascade regen: `sprint_1a_baseline`, `sprint_1b_acceptance`, `sprint_2_erosion`. All affected by 3.1.C.0 + 3.1.C via `default_pipeline()`. `overall_status: Passed` on all 24 regenerated shots. |
| 3.1.G | docs | CLAUDE.md Gotchas + PROGRESS.md close-out. |

Tasks 3.1.B, 3.1.D, 3.1.E closed with no code change (either subsumed into
another task, analytically dominated by upstream residuals, or structurally
out-of-scope per the plan's const-only thesis). Net delivery: 1 real fix +
3 structural cleanups + extensive calibration evidence for Sprint 4.

**Handoff to Sprint 3.5:**
- `authoritative.sediment` continues to carry `hs_init = HS_INIT_LAND *
  is_land` with `HS_INIT_LAND = 0.10` — stable.
- `baked.precipitation` now carries post-3.1.C.0 values (mean 0.012–0.031
  across archetypes, W/L ratio 1.2–27.4) — meaningful signal, no longer
  numerical collapse.
- `baked.fog_water_input` is 2× the pre-3.1 value across every archetype.
- `CoastType v2 counts` continue to show 0 Cliffs on stock archetypes —
  forwarded to 3.5.D's hex-edge grammar.
- `biome_coverage_percent` shifts post-3.1.C: DryShrub ↓, Grassland ↑
  everywhere; MontaneWetForest expanded; CloudForest + CoastalScrub still 0 %.

---

For close-out details on Sprints 0 / 1A / 1B / 1C / 1D / 2 / 2.5 / 2.6 / 3,
see [`docs/history/progress_archive_milestone_1.md`](docs/history/progress_archive_milestone_1.md)
(Obsidian vault symlink, gitignored — resolves on the author's machine only).

---

## DEFERRED TO LATER SPRINTS

Live forwarded residuals. Items absorbed or shipped have been archived with
the corresponding sprint in `docs/history/progress_archive_milestone_1.md`
(vault symlink; see header note above).

**Resolved at Sprint 3.5 close-out:**

- **§10 G7 CloudForest foothold** → **met** at Sprint 3.5.D via DD6
  bounded retune (CloudForest `f_t` envelope widened to `T_PEAK=18 /
  T_SIGMA=6`; `coastal_margin` SM floor raises θ into the bell).
  Foothold > 0% on ≥ 1 archetype; exact per-archetype numbers in
  3.5.D c2's regen commit msg.

**Forwarded to Sprint 4 (new at 3.5 close-out):**

- **§10 G5 — CoastType v2 Cliff coverage.** DD4 Q4 empirical escape
  triggered at 3.5.C close: hex-edge decoration grammar is in place
  but cell-level Cliff discrimination needs Sprint 4's physical-unit
  K / H* calibration to sharpen slopes. Sprint 3.1 probes had already
  confirmed threshold-tuning alone cannot produce Cliffs at Sprint 3
  defaults.
- **§10 G7 CoastalScrub foothold.** Still 0% on 5 archetypes post-DD6's
  `coastal_margin` SM floor + CloudForest retune. DD6 was bounded
  retune, not biome-suitability rework; CoastalScrub's combined θ
  gate + temperature structure is the actual blocker and needs the
  full unit-flux calibration pass that Sprint 4 brings.
- **2.5.D / 2.5.B — `HexDebugAttributes` production contract.** Sprint 5
  S2 (settlement / road / WFC) will redesign `accessibility_cost` when
  it becomes a real consumer. Sprint 3.5 consumed the existing shape
  (DD7 panel displays `accessibility_cost` read-only) but did not
  redesign; forward to Sprint 5 S2 as previously planned.

**Forwarded to Sprint 4 (carried from 3.1):**

- **§10 G4 — erosion relief drop fraction.** 0/5 archetypes hit the
  [0.10, 0.30] target post-3.1.A. Drops 0.00144–0.01447 across the 5
  archetypes. Any K bump above Sprint 3 default 5.0e-3 tips the smallest
  test grids (40²/64²) over the 5 % `erosion_no_excessive_sea_crossing`
  invariant. Natural home: Sprint 4's physical-unit calibration (absolute
  flux units would eliminate the `[0, 1]` normalization artifact).
- **LFPM v3 mean precipitation gap vs V2Raymarch.** Post-3.1.C.0
  `mean_precipitation` is 0.012–0.031, still below V2's 0.235. Residual
  is a normalization-by-max-P artifact inherent to the sweep structure;
  Sprint 4's physical-unit work would remove the `[0, 1]` normalization
  step in favour of absolute fluxes.

**Forwarded out of Sprint 3.5 (non-G-gate):**

- **Tier-2 interactive ↔ headless beauty parity test evidence.** The
  `IPG_RUN_VISUAL_PARITY=1`-gated integration test at
  `crates/app/tests/interactive_headless_parity.rs` (planned in 3.5.A)
  is unran at 3.5.F close — needs a GPU-attached session and visual
  acceptance. Tier-1 (`render_stack_for(ViewMode)` CPU parity) is
  green in every workspace test. Not blocking Sprint 4; pick up on a
  future visual-acceptance pass.
- **Sprint 3.5 hero shot pack** at
  `docs/design/sprints/sprint_3_5_visual_acceptance/` (Obsidian
  symlink, gitignored) — 9 curated hero shots (3 archetypes × 3 seeds,
  one representative view each) + `INDEX.md` mirroring 1A/1B/2.5
  pattern. Requires author-driven visual curation; does not block
  Sprint 4.

**Forwarded (long-standing):**

- **`crates/core` → `crates/ipg-core` rename (Task 1D.4).** Zero-risk
  refactor deferred — cross-cuts ~8 `Cargo.toml` files + ~30–50 `use
  island_core::` sites. Re-visit triggers:
  1. Sprint 4 adds more `crates/core`-splits that amplify alias churn.
  2. Any decision to publish to crates.io (rename becomes mandatory).
  3. A cross-crate refactor with enough scope that bundling the rename in
     is cheaper than doing it standalone.
- **Sprint 1B paper pack** — `docs/papers/sprint_packs/sprint_1b.md`
  Bruijnzeel 2005 / 2011 notes, Chen 2023 Budyko writeup, and Core Pack
  #2/#3/#5/#6/#8 "Sprint 1B 落地点" sections. Non-blocking; low-energy
  session.
- **Sprint 1A paper pack deep reads** — Chen 2014 + Génevaux 2013 + Lague
  2014 target-deep sections still outstanding at `docs/papers/core_pack/`.
- **Slider cadence measurement.** Re-run cost at 256² grew with Sprint 2's
  `ErosionOuterLoop` (10×10 inner iterations per re-run) and Sprint 3's
  SPACE-lite. No profiling numbers captured yet. Natural slot: a Sprint
  3.5 or Sprint 4 quick-win pass.

---

## LIVE

Nothing shipped to users yet — this is a pre-alpha research project.
`cargo run -p app` opens a local window on macOS with Metal backend; no
distribution, no wasm build, no binary releases.

---

## UPCOMING SPRINTS

Sprints 0 → 3.5 are shipped. Upcoming work starts at Sprint 4. Per-sprint
plan docs are written **one at a time** after the previous sprint closes —
the roadmap below carries the forward-looking vision until each sprint's
doc gets authored.

> **Roadmap vNext (2026-04-20, with 2026-04-22 3.4 insertion):**
> post-Sprint-3 sequence is
> `3 (science) → 3.1 (calibration) → 3.4 (structural cleanup) → 3.5 (hex readability) → 4 (infra) → 4.5 (beauty/demo) → 5 (semantic completion)`.
> Each sprint has a single thesis and its own out-of-scope list.
> See [roadmap §Post-Sprint-3 Roadmap Revision](docs/design/island_generation_complete_roadmap.md#post-sprint-3-roadmap-revision-vnext-2026-04-20).

| Sprint | Type | Focus | Source of truth |
|---|---|---|---|
| 4 | infra | `crates/gpu/` + `ComputeBackend` refactor, GPU passes (hillslope / rainfall-proxy-v2 / hex-projection + stream-power / flow-accumulation), `island-gen` CLI productization, CPU/GPU parity harness, artifact-system maturity. Physical-unit calibration closes §10 G4 (and unblocks 3.5's forwarded G5 Cliff coverage + G7 CoastalScrub foothold). Does NOT do first-pass beauty, semantic layer, or re-shape hex readability. | Roadmap §Sprint 4 |
| 4.5 | presentation | Canonical base-look lock (sky / fog / sea tonality, terrain shading polish, day-light rig), Water/Coast Presentation Pass, Depth & Framing Pass, Hero Seed Pack (6–10 curated worlds), Demo Artifact Pack (polished screenshots + GIFs + before/after strip), README / Demo Story Pass. First sprint where screenshots alone sell the repo. | Roadmap §Sprint 4.5 |
| 5 S2 | semantics | Settlement suitability + village/town placement + road graph v1 (MST + Dijkstra on hex, weighted by Sprint 3.5's semantic-consumable `accessibility_cost`). | Roadmap §Sprint 5 S2 |
| 5 S3 | semantics | WFC / rule-based semantic filling (points of interest, local pattern coherence). Rule-based guaranteed; 5×5 WFC patch experiment stretch. | Roadmap §Sprint 5 S3 |
| 5 S4 | optional ship tail | Web curated subset (wasm32, trunk, WebGPU, URL seed sharing, static seed gallery viewer) + semantic-layer interaction refinement. Explicitly optional. | Roadmap §Sprint 5 S4 |

---

## ON ICE

Nothing paused.

---

## QUICK REFERENCE

Active sprint: **Sprint 4 — Compute Productization** (*infra*).

**High energy?** → Author the Sprint 4 plan doc into
`docs/design/sprints/sprint_4_compute_productization.md` (currently empty
placeholder), then start the `crates/gpu/` + `ComputeBackend` scaffolding.
Sprint 3.5 closed with `HexSurfaceRenderer` / `HexRiverRenderer` already
instancing-based and `render_stack_for(ViewMode)` bridging interactive
and headless paths — GPU ports of sim stages land alongside these
without re-shaping the render layer. CLAUDE.local.md's subagent cadence
(implementer → simplifier → superpowers reviewer Opus) applies.

**Medium energy?** → Tier-2 interactive ↔ headless parity evidence.
Run `IPG_RUN_VISUAL_PARITY=1 cargo test -p app --test
interactive_headless_parity` in a GPU-attached session; attach output
as the delayed 3.5 evidence in a follow-up doc commit.

**Medium energy?** → Sprint 3.5 hero shot pack at
`docs/design/sprints/sprint_3_5_visual_acceptance/` (vault symlink).
9 curated shots (3 archetypes × 3 seeds, one representative view each) +
`INDEX.md` mirroring 1A/1B/2.5 pattern.

**Medium energy?** → Sprint 1B paper pack. Create
`docs/papers/sprint_packs/sprint_1b.md`: Bruijnzeel 2005 / 2011 TMCF notes,
Chen 2023 Budyko readthrough, Core Pack #2/#3/#5/#6/#8 "Sprint 1B 落地点"
sections. Parallelizable with any Sprint 4 implementation task.

**Low energy?** → Fill Sprint 1A deep reads still outstanding at
`docs/papers/core_pack/` (Chen 2014, Génevaux 2013, Lague 2014). Or annotate
remaining Sprint 2 / 3 paper "落地点" sections.

**Quick win?** → Slider cadence measurement. Re-run cost at 256² grew with
Sprint 2's `ErosionOuterLoop` (10×10 inner iters) and Sprint 3's SPACE-lite.
Sprint 3.5 did not add sim stages (only a `coastal_margin` branch inside
`SoilMoistureStage::run`) so cadence is unchanged from 3.1. Still no
profiling numbers captured; a baseline measurement here feeds Sprint 4's
GPU-parity budget.

---

**Update this file whenever a sprint ships, scope shifts, or a blocker moves.
Weekly minimum during active sprints.**
