---
id: smith_barstad_2004_linear_orographic
title: "A Linear Theory of Orographic Precipitation"
authors: Smith, Barstad
year: 2004
venue: Journal of the Atmospheric Sciences
doi: 10.1175/1520-0469(2004)061<1377:ALTOOP>2.0.CO;2
url: https://journals.ametsoc.org/view/journals/atsc/61/12/1520-0469_2004_061_1377_altoop_2.0.co_2.xml
tags: [orographic-precipitation, linear-model, Fourier-transform, moisture, climate]
sprint_first_used: sprint_1b
status: metadata_only
---

## 一句話用途

Sprint 1B 的降水模块（`crates/sim/src/climate/precipitation.rs`，待建）会选择或拒绝使用 Smith-Barstad 的 Fourier-domain 线性模型作为 v1 orographic precipitation 实现——本文是该选择的理论基础和接口规格。

## Abstract

A linear theory of orographic precipitation is developed, including airflow dynamics, condensed water advection, and downslope evaporation. The model extends previous "upslope" approaches by solving vertically integrated steady-state equations for condensed water using Fourier transform methods. Closed-form solutions for special cases are derived and a computationally efficient approach using terrain transforms multiplied by wavenumber-dependent transfer functions is created. The framework incorporates five critical length scales: mountain width, buoyancy wave scale, moist layer depth, and two condensed water advection distances. The model demonstrates sensitivity to forced ascent decay and downwind condensed water transport into descending regions. Results are demonstrated using the Olympic Mountains in Washington State.

## 关键方程 / 核心结论

- TODO (Sprint 1A first read)
- Key concept: precipitation rate P(x,y) = IFT[ Ĥ(k,l) · T̂(k,l) ] where Ĥ is the terrain transform and T̂ is a wavenumber-dependent transfer function incorporating airflow + condensation physics
- Five length scales govern behavior: mountain width, buoyancy wave scale, moist-layer depth, two condensed-water advection distances

## 对本项目的落地点

- TODO (Sprint 1A first read)
- Likely: `crates/sim/src/climate/precipitation.rs` (Sprint 1B) — will evaluate full Smith-Barstad FFT vs. simpler upwind raymarching proxy; sprint_1b will make this call
- Note: sprint_2 §4 explicitly defers Smith-Barstad to Sprint 2+ in favor of "upwind raymarch proxy v1"

## 值得警惕的点

- TODO (Sprint 1A)
- Smith-Barstad is a linear model valid for gentle terrain slopes; volcanic island topography with steep caldera walls may violate the linearization assumptions
- Requires FFT over the full domain — computational cost scales as O(N log N) per timestep, needs profiling before committing to this in the hot path
