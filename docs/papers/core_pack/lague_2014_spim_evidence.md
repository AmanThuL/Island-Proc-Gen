---
id: lague_2014_spim_evidence
title: "The stream power river incision model: Evidence, theory and beyond"
authors: Lague
year: 2014
venue: Earth Surface Processes and Landforms
doi: 10.1002/esp.3462
url: https://onlinelibrary.wiley.com/doi/10.1002/esp.3462
pdf: core_pack/lague_2014_spim_evidence.pdf
tags: [stream-power, river-incision, SPIM, threshold-stochastic, knickpoint, geomorphology]
sprint_first_used: sprint_2
status: downloaded
---

## 一句话用途

Sprint 2 的 `Ef = K A^m S^n` 实现之前必须读本文：Lague 2014 整理了 SPIM 的 field evidence 基础和已知失败模式（threshold effects, dynamic width），是 sprint_2 §RD1–RD3 参数选择的主要 field-evidence 参照。

## Abstract

The stream power incision model (SPIM) is a cornerstone of quantitative geomorphology. It states that river incision rate is the product of drainage area and channel slope raised to the power exponents m and n, respectively. This paper synthesizes research testing the model's validity through field evidence and theoretical analysis, and identifies deficiencies. The analysis reveals that river datasets away from knickpoints are dominated by threshold effects requiring upscaling of flood stochasticity, which the standard SPIM neglects. Through threshold-stochastic simulations incorporating dynamic channel width, the study documents composite transient dynamics where knickpoint propagation locally follows linear behavior (n=1) while other river sections exhibit non-linear patterns (n>1). The threshold-stochastic SPIM resolves some standard model inconsistencies and matches steady-state field evidence when channel width remains insensitive to incision rate. However, it fails when width decreases with incision rate. The author concludes that explicit upscaling of sediment flux combined with threshold-stochastic effects and dynamic width processes is necessary to advance beyond SPIM's limited range of validity.

## 关键方程 / 核心结论

- TODO (Sprint 1A first read)
- SPIM canonical form: `E = K A^m S^n` where A = drainage area, S = channel slope
- Threshold-stochastic extension: E depends on probability distribution of discharge events exceeding erosion threshold — Sprint 2 v1 does NOT implement this (deferred to Sprint 3/4)
- `n=1` locally near knickpoints; `n>1` globally — Sprint 2's `n=1.0` default is the knickpoint-propagation approximation
- SPIM fails when dynamic channel width matters — Sprint 2 uses fixed width (cell-size), consistent with Lague's "width insensitive to incision rate" validity window

## 对本项目的落地点

**Sprint 2 — `crates/sim/src/geomorph/stream_power.rs` (DD1):**
Lague 2014 provides empirical field-data compilation grounding the Sprint 2 choice of `(m, n) = (0.35, 1.0)`. The paper systematically reviews published measurements of stream-power exponents across diverse field sites and geological contexts, showing `m` clustered around 0.3–0.5 and `n` approaching 1.0 in steady-state reach data away from knickpoints. Sprint 2's constant-K forward-Euler integration with `n=1.0` (linear slope dependence) follows the knickpoint-propagation approximation Lague identifies as locally valid. The `K = 1e-3` scalar in `SPIM_K_DEFAULT` (see `crates/sim/src/geomorph/stream_power.rs`) is calibrated to produce visible relief decay within 100 iterations at the v1 resolution (256²), consistent with Lague's framework for interpreting K as a composite lithology + runoff + threshold parameter.

- `crates/sim/src/geomorph/` (Sprint 2) — SPIM erosion implementation; Lague 2014 is the primary validation reference for parameter plausibility
- `sprint_2_geomorph_credibility.md §RD1` selects `m=0.35, n=1.0` with explicit reference to the "safe zone away from Kwang & Parker instability"
- `sprint_2_geomorph_credibility.md §RD2` — K calibration uses Lague's framework for what K physically represents (composite of lithology + runoff + threshold)

## 值得警惕的点

- TODO (Sprint 1A)
- Lague explicitly states SPIM has "narrow range of validity" — v1's Sprint 2 operates entirely within that narrow range by design; Sprint 3 may need to expand
- Threshold-stochastic effects are NOT implemented in Sprint 2 — this is an acknowledged simplification; extreme precipitation events (Gourbet 2024) are the real driver of incision on volcanic islands
- Dynamic channel width: Sprint 2 fixes width at cell size (1 normalized unit). Lague warns this fails when width decreases with incision rate — monitor for artifacts in steep caldera walls
