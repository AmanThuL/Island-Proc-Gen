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

## Sprint 4 close-out summary (2026-04-26)

Captured on Apple M4 Pro Metal at 128² resolution (the canonical baseline
size). All numbers are mean over the shots in each baseline. AFTER
benchmarks were taken at HEAD `23a2689` (Sprint 4.F shipped — both GPU
pilots active).

### Pre/CPU vs Post/CPU — kernel extraction is a no-op refactor

| Baseline | pre/cpu pipeline_ms | post/cpu pipeline_ms | drift |
|---|---:|---:|---:|
| sprint_1a_baseline | 23.04 | 23.21 | +0.7% |
| sprint_1b_acceptance | 19.81 | 19.71 | -0.5% |
| sprint_2_erosion | 12.35 | 12.44 | +0.7% |
| sprint_3_sediment_climate | 19.85 | 20.28 | +2.2% |
| sprint_3_5_hex_surface | 18.02 | 18.01 | -0.1% |

All 5 baselines drift < 5% threshold from pre to post under default
`--compute-backend cpu`. Sprint 4.C's `hillslope_diffusion_kernel` /
`stream_power_incision_kernel` extraction successfully preserves CPU
performance — the trait dispatch overhead is in the noise floor.

### Post/CPU vs Post/GPU — wall-clock characterization

| Baseline | post/cpu pipeline_ms | post/gpu pipeline_ms | factor |
|---|---:|---:|---:|
| sprint_1a_baseline | 23.21 | 1004.31 | **43× slower** |
| sprint_1b_acceptance | 19.71 | 1013.00 | **51× slower** |
| sprint_2_erosion | 12.44 | 484.50 | **39× slower** |
| sprint_3_sediment_climate | 20.28 | 999.24 | **49× slower** |
| sprint_3_5_hex_surface | 18.01 | 1002.27 | **56× slower** |

The GPU path is **40–55× slower end-to-end** than CPU on the canonical
128² grids. ErosionOuterLoop dominates at ~98% of pipeline time on the
GPU path (vs ~85% on CPU), confirming the inner kernel is the bottleneck
— but on the wrong side of the speedup ledger.

### Attribution

Sprint 4 ships *measurement-rich foundation*, not guaranteed speedup —
`§10 G5` of the sprint plan accepts negative whole-pipeline delta as
long as the bottleneck attribution is clear. The 40–55× slowdown breaks
down as:

- **100 dispatches per regen** (10 outer batches × 10 inner iterations,
  each running both Hillslope + StreamPower kernels = 20 dispatches per
  batch × 10 batches = 200 dispatches, but the figure-of-merit is "100
  per kernel pair").
- Each dispatch carries: CPU → GPU buffer upload (height + sediment +
  is_land + accum + slope) + GPU dispatch + **synchronous** sync
  readback via `device.poll(wgpu::PollType::Wait{...})` (DD6 lock).
- GPU inner kernels are sub-microsecond on M4 Pro Metal — they
  **underflow `wgpu::Features::TIMESTAMP_QUERY` resolution**, reading
  `gpu_ms = Some(0.0)` in the parity test output. The wall-clock cost
  is dominated by I/O round-trip latency, not compute.
- 128² grids are too small for the GPU to amortize. A 1024² grid would
  shift the picture; the parity tests run on 128² to keep CI tractable.

### Forwarding to Sprint 4.x

Sprint 4 explicitly does NOT solve the GPU slowdown — it only makes the
attribution measurable. Sprint 4.x candidates (in priority order):

1. **Persistent buffers across outer batches.** Today every inner
   dispatch re-uploads height/sediment/is_land/accumulation/slope from
   CPU. Persistent buffers would amortize the upload across all 100
   inner iterations of a regen. Expected savings: ~80% of GPU
   wall-clock.
2. **Deferred readback.** `device.poll(wgpu::PollType::Wait)` blocks
   the CPU thread for 10+ ms per dispatch. Deferred readback (timestamp
   queries written N frames later, skip a frame's reading if not yet
   ready) would let GPU dispatches stay queued.
3. **Kernel fusion.** Combine Hillslope + StreamPower into one
   compute-pass-encoder dispatch within each inner iteration —
   amortizes the encoder setup cost.
4. **Larger grids.** Re-run the benchmark at 256² / 512² / 1024² to find
   the cross-over point where GPU ≥ CPU wall-clock.

### What this proves vs doesn't prove

- ✅ The Sprint 4 abstraction (`ComputeBackend` trait, `default_pipeline_with_backend`,
  `--compute-backend cpu|gpu` flag, schema_version 4 `stage_timings`,
  Profiler tab MVP) is correct and behaviour-equivalent. CPU stays
  canonical truth (DD5).
- ✅ Per-stage timing measurement works end-to-end across the 18-stage
  pipeline. ErosionOuterLoop is reliably identified as the dominant
  inner cost (83-86% on CPU, ~98% on GPU).
- ✅ DD8 numerical parity is met by huge margins:
  hillslope `max_abs_interior=5.96e-8` (167× under 1e-5 limit); stream-power
  per-iter `0.0e0` for all DD8 sub-bounds; accumulated 100-iter
  `5.96e-8` vs tolerance `9.9e-4` (16,000× margin).
- ❌ GPU dispatch is NOT faster than CPU end-to-end at 128² with the
  current synchronous-readback architecture. **This was acceptable per
  DD7 / §10 G5 but is the dominant Sprint 4.x optimization target.**

### Headline (commit message + CLAUDE.md Gotchas + PROGRESS.md):

> *On `sprint_3_sediment_climate` post_volcanic_single seed 42 at 128²
> on Apple M4 Pro Metal: post/cpu pipeline = 20.28 ms, post/gpu pipeline
> = 999.24 ms (49× slowdown). The slowdown is dominated by 100
> synchronous CPU↔GPU readbacks per regen × ~10 ms each = ~1 s wall-
> clock. Inner GPU kernels are sub-microsecond (TIMESTAMP_QUERY
> underflow). DD8 parity met by 167-1000× margins. Sprint 4.x
> investigates persistent buffers, deferred readback, and kernel fusion
> — those are the unblocking work, not Sprint 4 scope.*
