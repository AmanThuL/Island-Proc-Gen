# Sprint 4 Compute Productization — Benchmark Evidence

This directory holds **evidence artifacts**, not validation truth. The
checked-in CSVs are wall-clock timing snapshots produced by Sprint 4's
`--headless` runs against the 5 golden baselines under
`crates/data/golden/headless/`.

| Path | When written | Schema | Contents |
|---|---|---|---|
| `pre/cpu/<5>.csv` | Sprint 4.0 (lump-sum) → Sprint 4.B (per-stage backfill) | v3 → v4 | "BEFORE" CPU snapshot. Sprint 4.0 lands the lump-sum trio (`pipeline_ms`, `bake_ms`, `gpu_render_ms`) under v3. Sprint 4.B overwrites with full per-stage columns once `RunSummary.schema_version` bumps to 4 and `--print-breakdown` lands. |
| `post/cpu/<5>.csv` | Sprint 4.G | v4 | "AFTER" CPU snapshot. Same five baselines after every Sprint 4 commit lands. Should match `pre/cpu/` modulo small slack — kernel extraction at Task 4.C is a refactor, not a behavioural change. |
| `post/gpu/<5>.csv` | Sprint 4.G | v4 | GPU benchmark — `IPG_COMPUTE_BACKEND=gpu` cascade run after Task 4.F. `ErosionOuterLoop` carries populated `gpu_ms` and (per Sprint 4 DD3 surface A) optional `upload`/`dispatch`/`readback` sub-columns. The other 16 stages stay CPU. |

## What this is NOT

This is **not** a `crates/data/golden/` sibling. The `golden/` tree
carries CLAUDE.md AD7 "validation truth — bit-stable across hosts"
semantics; benchmarks here are wall-clock measurements that drift
across machines, OS schedulers, thermal envelopes, and GPU driver
versions.

- **Same-host before/after diffs are interpretable.** A 30 % regression
  in `Hillslope` between `pre/cpu/` and `post/cpu/` on the same machine
  is a signal worth investigating.
- **Cross-host comparisons are not.** The same baseline timed on
  different hardware (or even the same hardware under different thermal
  load) will diverge by tens of percent without anything having actually
  changed in the simulation. Don't open issues for cross-host diffs.

The CSVs do **not** participate in the AD9 exit-code map — Sprint 4 does
not gate `--headless-validate` on benchmark deltas. Regressions surface
in human review, not CI.

## What "regression" means

Concrete threshold (lock-in for Sprint 4 close-out at 4.G):

> *Any single stage's `cpu_ms` worsens by **> 10 %** across the
> 5-baseline median between the `pre/cpu/` and `post/cpu/` runs on the
> author's machine → investigation required, not auto-block.*

A single baseline drifting by > 10 % is not an automatic regression
(thermal noise + macOS scheduler jitter). The 5-baseline median is the
denoising mechanism.

If the investigation concludes the regression is unfixable in Sprint 4
(e.g., LLVM auto-vectorization defeated by the kernel-extraction
refactor), the close-out commit message must:
1. Document the affected stage and per-baseline slowdown
2. Forward the optimization to Sprint 4.1 / 4.x by name

> Per CLAUDE.md AD9: benchmark slowdown does NOT change the AD9 exit
> code (still 0 / 2 / 3) — `cpu_ms` and `gpu_ms` are AD8-whitelisted
> non-deterministic fields, NEVER part of `--headless-validate` pass /
> fail.

## How to regenerate

```bash
crates/data/benchmarks/sprint_4/regen.sh pre/cpu        # at Sprint 4.0
crates/data/benchmarks/sprint_4/regen.sh post/cpu       # at Sprint 4.G CPU
IPG_COMPUTE_BACKEND=gpu \
  crates/data/benchmarks/sprint_4/regen.sh post/gpu     # at Sprint 4.G GPU
```

The script:
1. Builds `app` in release mode (uses `cargo build -p app --release`).
2. Runs each of the 5 baselines via `./target/release/app --headless`.
3. Pipes each `summary.ron` through `extract_summary.py` and writes the
   per-shot CSV under the requested bucket.
4. After Sprint 4.A lands `--print-breakdown`, the script switches to
   the per-stage path automatically.

The runtime side-effects on `crates/data/golden/headless/<baseline>/`
(an updated `summary.ron` + freshly written PNGs) are gitignored or
reverted before commit. See the per-task regen cadence in
[`../golden/headless/README.md`](../golden/headless/README.md).

## Sprint 4 close-out summary

Filled in at Task 4.G. Until then the section reads "TBD".

> *TBD at Sprint 4.G — per-baseline `ErosionOuterLoop` inner-kernel CPU
> vs GPU ms with upload / dispatch / readback breakdown, whole-pipeline
> wall-time delta (positive **OR** negative), and Sprint 4.x forwarding
> for any unaddressable bottleneck.*
