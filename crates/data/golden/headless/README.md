# `--headless` Golden Baselines

Checked-in regression references for Sprint 1C's `--headless-validate`
tool. Each sub-directory holds one baseline as three layers:

```
<baseline_id>/
├── request.ron              # the CaptureRequest that produced the baseline
├── summary.ron              # hashes + overall_status (the compare contract)
└── shots/<shot_id>/
    └── metrics.ron          # SummaryMetrics snapshot (only when
                             # include_metrics: true on that shot)
```

**PNGs are NOT checked in.** Per sprint doc AD4, overlay exports and
beauty captures live in `/captures/headless/…` (gitignored) when the
harness runs. `--headless-validate` compares the blake3 hex in
`summary.ron` and does not read any PNG file. The root `.gitignore`
carries a `crates/data/golden/headless/**/*.png` rule for belt-and-braces
safety if someone points `output_dir` at a baseline dir and re-runs the
harness locally.

## Baselines

| Dir | Shots | Scope |
|---|---|---|
| `sprint_1a_baseline/` | 9 | 3 presets × 3 golden seeds × Hero camera. Matrix locked by Sprint 1C Task 1C.9. Seeds `[42, 123, 777]` match `crates/data/golden/snapshots/` so the numeric and visual regressions share one set of pairs. |
| `sprint_1b_acceptance/` | 9 | Migration of the default-wind subset of the 16-shot `docs/design/sprints/sprint_1b_visual_acceptance/` PNG archive. |

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
Pass 3 soil-moisture wind pair) and shot 01 (the egui panel smoke test)
— cannot be expressed in the Sprint 1C v1 `CaptureRequest` schema:

- **Wind-varying shots (6)** require per-shot override of
  `preset.prevailing_wind_dir`. The v1 schema has no such field; see
  Sprint 1C doc §7 open question 4 for the candidate v2 hook
  (`CaptureShot.preset_override`). The wind-propagation contract is
  already locked mechanically by
  `sim::validation_stage::tests::wind_dir_rerun_propagates_through_biome_chain`
  — a pipeline-level byte assertion that `run_from(Precipitation)`
  mutates `precipitation / fog_likelihood / soil_moisture /
  biome_weights / dominant_biome_per_cell`. That regression guard is
  authoritative.
- **Shot 01 (panel smoke test)** asserts UI state (the Climate section
  in ParamsPanel, the 12-entry OverlayPanel). `--headless` does not
  instantiate egui by design (AD1), so this shot is structurally
  outside the harness's scope.

The original 16 PNGs remain in `docs/design/sprints/sprint_1b_visual_acceptance/`
as human-readable visual references; deleting or replacing them is out
of Sprint 1C's scope.
