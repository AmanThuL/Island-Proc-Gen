# `--headless` Golden Baselines

Checked-in regression references for the `--headless-validate` tool.
The tool itself shipped in Sprint 1C; the baselines under this directory
have grown from 1 → 5 across the post-Sprint-1C sprints. Each
sub-directory holds one baseline as three layers:

```
<baseline_id>/
├── request.ron              # the CaptureRequest that produced the baseline
├── summary.ron              # hashes + overall_status (the compare contract)
└── shots/<shot_id>/
    └── metrics.ron          # SummaryMetrics snapshot (only when
                             # include_metrics: true on that shot)
```

**PNGs are NOT checked in.** Per Sprint 1C AD4, overlay exports and
beauty captures live in `/captures/headless/…` (gitignored) when the
harness runs. `--headless-validate` compares the blake3 hex in
`summary.ron` and does not read any PNG file. The root `.gitignore`
carries a `crates/data/golden/headless/**/*.png` rule for belt-and-braces
safety if someone points `output_dir` at a baseline dir and re-runs the
harness locally.

## Baselines

| Dir | Shots | Schema | Scope |
|---|---|---|---|
| `sprint_1a_baseline/` | 9 | v1 | 3 presets × 3 golden seeds × Hero camera. Matrix locked by Sprint 1C Task 1C.9. Seeds `[42, 123, 777]` match `crates/data/golden/snapshots/` so the numeric and visual regressions share one set of pairs. |
| `sprint_1b_acceptance/` | 9 | v2 | Migration of the default-wind subset of the 16-shot `docs/design/sprints/sprint_1b_visual_acceptance/` PNG archive. |
| `sprint_2_erosion/` | 6 | v2 | Sprint 2 DD6 before/after erosion pairs: 3 presets × 2 (pre/post erosion) at seed 42, 128² resolution. `pre_*` shots use `preset_override.erosion.n_batch = 0` to make `ErosionOuterLoop` a noop; `post_*` shots run the locked 10×10 outer loop. Locked by Sprint 2 Task 2.6. |
| `sprint_3_sediment_climate/` | 10 | v2 | Sprint 3 DD1–DD6 before/after pairs (SPACE-lite sediment + LFPM v3 precipitation + CoastType v2): 5 archetypes × 2 (`pre_*` reverts erosion to Sprint 2 `Plain`/`V2Raymarch`/`V1Cheap` variants via `preset_override`; `post_*` runs Sprint 3 defaults) at seed 42, 128². Locked by Sprint 3 Task 3.10. |
| `sprint_3_5_hex_surface/` | 27 | v3 | Sprint 3.5 hex-surface readability lock-in: 3 archetypes (`volcanic_single`, `volcanic_twin_old`, `volcanic_caldera_young`) × 3 seeds (42, 1337, 9001) × 3 view modes (`Continuous`, `HexOverlay`, `HexOnly`) at 128². v3 schema adds `CaptureShot.view_mode: Option<ViewMode>` and three new hashes on `SummaryMetrics` (`hex_attrs_hash`, `hex_debug_river_crossing_hash`, `hex_coast_class_hash`). Truth-path is bit-identical across the three view modes for the same `(seed, preset)` — view_mode only affects the beauty render stack via `render_stack_for(ViewMode)`. Locked by Sprint 3.5 Task 3.5.F. |

### `CaptureRequest` schema versions

| Version | Sprint | What it adds |
|---|---|---|
| v1 | 1C | Baseline shape: `(schema_version, run_id?, output_dir?, shots[])` with `id / seed / preset / sim_resolution / truth / beauty?` per shot. |
| v2 | 2 (DD5) | Optional `CaptureShot.preset_override: Option<PresetOverride>` — every `IslandArchetypePreset` knob can be overridden per-shot, including `prevailing_wind_dir`, `erosion.{spim_k, …, n_batch, spim_variant, coast_type_variant}`, and `climate.{precipitation_variant, q_0, tau_c, tau_f}`. |
| v3 | 3.5 (DD8) | Optional `CaptureShot.view_mode: Option<ViewMode>` (`Continuous` / `HexOverlay` / `HexOnly`); `None` ≡ `Continuous` for backwards compat. v1 and v2 request files still parse cleanly under v3 binaries (additive extensions, all `#[serde(default)]`). |

`RunSummary.schema_version` mirrors the input request's
`schema_version` — a v1 request under a v3 binary still produces a
v1-stamped summary, so older baselines validate without forced
migration.

## World aspect ratio convention

All five baselines were captured at `render::DEFAULT_WORLD_XZ_EXTENT =
5.0` (Fuji-like aspect ≈ 0.17, frozen 2026-04-19). Sprint 2.6.A
originally shipped at `3.0` but the live A/B pushed the final value to
Fuji-like. Headless always passes
`DEFAULT_WORLD_XZ_EXTENT` to the camera math; the live app has a
runtime override (`Runtime::world_xz_extent`) exposed via the World
panel's aspect ComboBox so the user can A/B several values, but that
override does NOT reach the headless path. Whenever the author freezes
a different final aspect, update the const in-place and regen all
baseline beauty PNGs in a follow-up `chore(data):` commit; truth hashes
stay bit-identical (sim is extent-agnostic), only beauty `byte_hash`
values drift with the new camera framing.

## Regenerating a baseline (author workflow)

```bash
cargo run -p app --release -- --headless crates/data/golden/headless/<baseline_id>/request.ron
```

The harness writes into `output_dir` (set in `request.ron` to the same
baseline dir), producing `request.ron` (round-tripped pretty form),
`summary.ron`, and — if `include_metrics: true` on the shot — a
`shots/<id>/metrics.ron`. Any PNG files written are gitignored; delete
them with `find crates/data/golden/headless/<baseline_id> -name '*.png'
-delete` before committing if you want a clean workspace.

### Per-task regen cadence (Sprint 3.5 onwards)

Truth-path-changing commits are required to land their snapshot regen
in the *same* sub-sprint task, never deferred to a sprint close-out.
The pattern (per Sprint 3.5 §3 expected-red-commit policy):

1. The truth-path change ships as a **known-red commit** (a single
   diagnosable hash field moves; `cargo test --workspace` would fail
   on `golden_seed_regression` + the affected baselines).
2. The very next commit is a `chore(data):` regen that updates all
   affected `summary.ron` + `shots/*/metrics.ron` files, restoring
   green. The commit message attributes which hash field moved.

Each sub-sprint that touches simulation output (Sprint 3.5.A through
3.5.D) followed this cadence — never leave a known-red commit on
`dev` after a task closes.

### Two-step `--headless-validate` protocol

For author-driven verification of a regen commit pair:

- **Step A (pre-regen, red-expected):** immediately after the known-red
  commit, run `cargo run -p app --release -- --headless-validate
  <captures-run> --against <current-baseline-dir>`. Expected exit code
  is **2** (`FailedMetricsValidation`). Read `summary.ron` manually and
  confirm the diff is exactly the expected fields and no more.
- **Step B (post-regen, green):** after the regen commit updates the
  baseline files, re-run the validate command. Expected exit code
  **0** (`Passed`).

Step A is the reviewer-confirmable evidence that the regen does what
its commit message claims; step B proves the tree is back to green.

## Validating against a baseline (CI / PR workflow)

```bash
cargo run -p app --release -- \
    --headless-validate /captures/headless/<new_run>/ \
    --against crates/data/golden/headless/<baseline_id>/
```

Exit codes (AD9 public contract):
- **0** — `Passed` / `PassedWithBeautySkipped` (truth-green; warnings via stderr)
- **2** — `FailedTruthValidation` / `FailedMetricsValidation`
- **3** — `InternalError` (tool-level: IO, RON parse, shot-set mismatch, etc.)

## Scope caveats

### Sprint 1B migration is a subset, not a superset

`docs/design/sprints/sprint_1b_visual_acceptance/INDEX.md` lists 16
shots; `sprint_1b_acceptance/` migrates 9 of them. The remaining 7 —
shots 50–53 and 60–61 (the Pass 2 wind-direction slider sweeps plus
Pass 3 soil-moisture wind pair) and shot 01 (the egui panel smoke
test) — were originally outside the Sprint 1C v1 `CaptureRequest`
schema's reach. Sprint 2's v2 `preset_override.prevailing_wind_dir`
closed that schema gap retroactively; the wind-varying shots are now
*expressible* but have not been migrated:

- **Wind-varying shots (6)** could be expressed today by setting
  `preset_override.prevailing_wind_dir = Some(<radians>)` per shot.
  Migration is purely a data-entry exercise (one new shot per wind
  value × archetype) and would extend `sprint_1b_acceptance/` from 9
  → 15 shots. Out of scope at every sprint that's looked at it
  because the wind-propagation contract is already locked mechanically
  by
  `sim::validation_stage::tests::wind_dir_rerun_propagates_through_biome_chain`
  — a pipeline-level byte assertion that `run_from(Precipitation)`
  mutates `precipitation / fog_likelihood / soil_moisture /
  biome_weights / dominant_biome_per_cell`. That regression guard is
  authoritative; the unmigrated wind PNG archive is for human visual
  reference only.
- **Shot 01 (panel smoke test)** asserts UI state (the Climate section
  in ParamsPanel, the OverlayPanel). `--headless` does not
  instantiate egui by design (Sprint 1C AD1), so this shot is
  structurally outside the harness's scope across every schema
  version.

The original 16 PNGs remain in `docs/design/sprints/sprint_1b_visual_acceptance/`
as human-readable visual references; replacing them with migrated
v2 baselines is a future-sprint cleanup, not a hard residual.

### Sprint 3.5's view-mode dimension is truth-invariant

Within `sprint_3_5_hex_surface/`, the 3 view modes per `(seed,
preset)` produce **bit-identical truth output** — `overlay_hashes` and
all `SummaryMetrics` hashes (including the three new DD8 hex hashes)
are equal across `Continuous` / `HexOverlay` / `HexOnly`. View mode
only affects the beauty render stack via `render_stack_for(ViewMode)`
in `crates/app/src/runtime/view_mode.rs`. If a future change makes
view_mode truth-affecting, that's an architectural violation — split
the dimension into a real pipeline knob first.
