---
id: chen_2014_lem_review
title: "Landscape evolution models: A review of their fundamental equations"
authors: Chen, Darbon, Morel
year: 2014
venue: Geomorphology
doi: 10.1016/j.geomorph.2014.04.037
url: https://www.sciencedirect.com/science/article/pii/S0169555X14002402
tags: [landscape-evolution, PDE, stream-incision, hillslope, sediment, review]
sprint_first_used: sprint_0
status: metadata_only
---

## 一句话用途

Sprint 1A 的 `crates/sim/src/geomorph/topography.rs` 和 `crates/sim/src/hydro/flow_routing.rs` 构造初始地形和水流时，用本文作为方程选择框架——特别是 §2 的水流守恒方程和 §3 的 stream incision + sediment 质量守恒组合（选择两方程或三方程模型的判据来自本文的综述架构）。

## Abstract

This paper reviews the main physical laws proposed in landscape evolution models (LEMs). It discusses first the main partial differential equations involved in these models and their variants. These equations govern water runoff, stream incision, regolith-bedrock interaction, hillslope evolution, and sedimentation. The paper proposes three models with growing complexity and with a growing number of components: two-equation models with only two components, governing water and bedrock evolution; three-equation models with three components where water, bedrock, and sediment interact; and finally models with four equations and four interacting components, namely water, bedrock, suspended sediment, and regolith. A key finding is the need for a correct formulation of the water transport equation down slopes, and resolution of contradictions between detachment-limited and transport-limited erosion modes through introduction of suspended sediment as an additional variable. Numerical experiments on real digital elevation models (DEMs) demonstrate landscape evolution results.

## 关键方程 / 核心结论

- **Stream-power incision (detachment-limited):** `∂h/∂t = U - K A^m |∇h|^n` — the canonical SPIM used in Sprint 2 with `m=0.35, n=1.0` (per `sprint_2_geomorph_credibility.md §RD1`)
- **Transport-limited regime (Exner):** sediment flux divergence drives bedrock change; SPACE 1.0 (Shobe 2017) bridges these two regimes
- **Two-equation vs. three-equation model:** v1 of this project uses a two-equation approach (bedrock + water); sediment as a third variable enters Sprint 3
- **Water routing:** the paper warns that naive downstream routing on steep slopes introduces numerical artifacts — Sprint 1A's D8 + pit-fill approach addresses this

## 对本项目的落地点

**Sprint 1A — `crates/sim/src/geomorph/topography.rs` (Task 1A.1):**
The `volcanic_base + ridge_mask - coastal_falloff` synthetic DEM is this project's surrogate for "initial uplift" before erosion. Chen 2014 §2 provides the governing equation for what `U` (uplift) means in LEM terms and why the pre-erosion field must be smooth (C² continuity at the peak, per the smoothstep cone choice in sprint_1a §D3).

**Sprint 1A — `crates/sim/src/hydro/flow_routing.rs` (Task 1A.3) and `crates/sim/src/hydro/accumulation.rs` (Task 1A.4):**
The D8 flow routing + topological accumulation implements the discrete equivalent of Chen's water runoff PDE. The paper's treatment of water transport is the theoretical justification for pit-filling before routing (§D7 Planchon-Darboux) and the ε-jitter tie-breaking in §D6.

**Sprint 2 — `crates/sim/src/geomorph/` erosion stages:**
When Sprint 2 implements `Ef = K A^m S^n`, Chen 2014 is the primary equation-selection reference. The paper's argument that `m/n ≈ 0.45` corresponds to real concavity indices justifies the Sprint 2 default `m=0.35, n=1.0` (conservative, per sprint_2 §RD1). Must re-read §3 (three-equation model) before deciding whether Sprint 3 adds the sediment variable.

**Sprint 3 — Sediment transport:**
Chen 2014 §4 (four-equation model with regolith) is the reading assignment for Sprint 3's sediment budget work. The `authoritative.sediment` field in `crates/core/src/world.rs` is the `WorldState` slot reserved for this.

## 值得警惕的点

- Chen 2014 reviews LEMs for geological timescales (10³–10⁶ yr); our v1 uses 100 explicit iterations as a proxy, not real time steps. The `K` calibration approach (see sprint_2 §RD2) is a workaround for this mismatch.
- The paper's water transport equation uses a continuous flux formulation; our D8 discrete routing is an approximation. Sprint 1A §D6 documents the tie-breaking heuristic that compensates.
- Chen 2014 does not cover the Kwang & Parker 2017 `m/n=0.5` instability — that is a separate must-read before Sprint 2.
- Authors: Alex Chen, Jérôme Darbon, Jean-Michel Morel (applied math perspective, not field geomorphology). The paper is rigorous on PDEs but lighter on calibration guidance; Lague 2014 and Whipple & Tucker 1999 complement it on empirical grounding.
