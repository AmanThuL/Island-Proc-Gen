---
id: whipple_tucker_1999_spim_dynamics
title: "Dynamics of the stream-power river incision model: Implications for height limits of mountain ranges, landscape response timescales, and research needs"
authors: Whipple, Tucker
year: 1999
venue: Journal of Geophysical Research: Solid Earth
doi: 10.1029/1999JB900120
url: https://agupubs.onlinelibrary.wiley.com/doi/10.1029/1999JB900120
pdf: core_pack/whipple_tucker_1999_spim_dynamics.pdf
tags: [stream-power, river-incision, SPIM, mountain-relief, response-time, uplift-erosion]
sprint_first_used: sprint_2
status: downloaded
---

## 一句话用途

Sprint 2 の erosion ループ設計（`sprint_2_geomorph_credibility.md §RD3`）において、SPIM の動的挙動（特に uplift-erosion 数と応答時間の n 依存性）を理解するための基礎参照——時間ステップ安定性と steady-state relief の予測に使う。

## Abstract

The longitudinal profiles of bedrock channels significantly determine mountain relief and peak elevation. These channels transmit tectonic and climatic signals across landscapes, dictating how mountainous regions respond to external forces. The authors examined the stream-power erosion model to understand topographic relief, its sensitivity to tectonic and climate changes, system response times to uplift disturbances, and parameter sensitivity. Key findings include that the dynamic behavior of the stream-power erosion model is governed by a single nondimensional group termed the uplift-erosion number. The slope exponent (n) representing nonlinearity in stream incision rates versus channel gradient emerges as critical, influencing relationships between the uplift-erosion number, equilibrium channel gradient, and total fluvial relief. The predicted response time to rock uplift rate changes depends on climate, rock strength, and tectonic perturbation magnitude, with the slope exponent controlling sensitivity to these factors. Response time remains relatively insensitive to drainage basin size. The authors identify urgent research needs: understanding bedrock erosion physics, extreme flood sensitivity, transient climate and uplift responses, and scaling from local rock erosion studies to reach-scale modeling.

## 关键方程 / 核心结论

- TODO (Sprint 1A first read)
- Uplift-erosion number: nondimensional group governing SPIM dynamic behavior — determines whether system is erosion-limited or uplift-limited
- Slope exponent n controls response time sensitivity and equilibrium gradient — Sprint 2 uses `n=1.0` (linear response); Whipple & Tucker show `n>1` produces knickpoint-dominated dynamics
- Response time formula: `τ ~ χ / K` where χ is the drainage integral — Sprint 2's 100-iteration proxy must be calibrated against this

## 对本项目的落地点

**Sprint 2 — `crates/sim/src/geomorph/stream_power.rs` (DD1) and `ErosionOuterLoop` (DD3):**
Whipple & Tucker 1999 is the canonical reference for `n=1.0` (linear slope dependence) being the dynamically simplest choice for forward-Euler stability. The paper's uplift-erosion number framework explains why non-linear `n > 1` requires implicit (tridiagonal) solvers to avoid timestep blow-up. Sprint 2's v1 design selects `n=1.0` precisely because it permits explicit integration: the CFL-like constraint on stable `dt` is linear in the slope exponent, not exponential. The 10×10 outer-loop iteration scheme (10 full-pipeline carve-smooth-carve cycles per slider update) trades iteration count for per-iteration stability — each outer iteration uses `n=1.0` forward-Euler with `dt=1.0`, which Whipple & Tucker show is unconditionally stable for realistic K and A values. Non-linear `n != 1` would require implicit schemes that Sprint 2 defers to Sprint 4's GPU pivot.

- `crates/sim/src/geomorph/` (Sprint 2) — SPIM erosion loop; Whipple & Tucker's stability analysis justifies Sprint 2's explicit forward-Euler scheme with `dt=1.0` and `n=1.0` (linear → no CFL blow-up for reasonable K)
- `sprint_2_geomorph_credibility.md §RD3` references the CFL-like constraint on `dt * K * max_A^m * max_S^n` — this derivation traces back to Whipple & Tucker's stability analysis
- Sprint 2 §RD1 parameter selection (m=0.35, n=1.0) is anchored against the uplift-erosion number framework from this paper

## 值得警惕的点

- TODO (Sprint 1A)
- 1999 paper — predates the Kwang & Parker 2017 instability discovery. Must cross-reference Kwang & Parker when selecting (m,n) — Whipple & Tucker alone is insufficient for the parameter choice
- Response time analysis assumes steady-state tectonic uplift; our synthetic volcanic islands have no tectonic driver — the "uplift" is the initial topographic field, not an ongoing process. Apply the response-time framework qualitatively
- Whipple & Tucker analyze 1D channel profiles; Sprint 2 runs 2D — the scaling may differ in the presence of diverging/converging flow
