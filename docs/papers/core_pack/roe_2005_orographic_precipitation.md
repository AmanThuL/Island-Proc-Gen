---
id: roe_2005_orographic_precipitation
title: "Orographic Precipitation"
authors: Roe
year: 2005
venue: Annual Review of Earth and Planetary Sciences
doi: 10.1146/annurev.earth.33.092203.122541
url: https://www.annualreviews.org/content/journals/10.1146/annurev.earth.33.092203.122541
pdf: core_pack/roe_2005_orographic_precipitation.pdf
tags: [orographic-precipitation, review, wind-driven, rain-shadow, climate]
sprint_first_used: sprint_1b
status: downloaded
---

## 一句话用途

Sprint 1B 的降水 + 雨影区模块（`crates/sim/src/climate/`）设计时用本文作为 "orographic precipitation 机理综述"，特别是 rain shadow 效应和 upwind/downwind 不对称性的定性框架——这是 sprint_1b 上风向迎风坡降水建模的理论背景。

## Abstract

This paper reviews the physical mechanisms governing orographic precipitation, covering fluid dynamics, thermodynamics, and cloud microphysical processes at scales from individual mountains to continental ranges. The review discusses how surface orography influences precipitation patterns through forced ascent of moist air, condensation, and the resulting rain shadow effects on the leeward side. Key topics include the role of atmospheric stability, the relationship between prevailing wind direction and precipitation distribution, and how spatial scale influences precipitation mechanisms and amounts.

## 关键方程 / 核心结论

- TODO (Sprint 1A first read)
- Key concept: windward/leeward asymmetry — moist air forced up the windward slope precipitates, descending on the leeward side re-evaporates → rain shadow
- `preset.prevailing_wind_dir` in Sprint 0's `IslandArchetypePreset` directly encodes the upwind direction that this paper motivates

## 对本项目的落地点

- TODO (Sprint 1A first read)
- `crates/sim/src/climate/precipitation.rs` (Sprint 1B) — Roe 2005 is the conceptual justification for why a rain shadow effect must be modeled even if the full Smith-Barstad FFT is not used
- Sprint 1B's "upwind raymarch proxy v1" is a simplified implementation of the physical intuition this review provides

## 值得警惕的点

- TODO (Sprint 1A)
- Annual review format — broad but not implementation-ready; use Smith & Barstad 2004 and Hergarten & Robl 2022 for actual equations
