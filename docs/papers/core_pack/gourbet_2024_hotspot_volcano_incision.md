---
id: gourbet_2024_hotspot_volcano_incision
title: "Climate impact on river incision on hotspot volcanoes: insights from paleotopographic reconstructions and numerical modelling"
authors: Gourbet, Gallen, Famin, Michon, Ramanitra, Gayer
year: 2024
venue: Earth and Planetary Science Letters
doi: 10.1016/j.epsl.2024.118801
url: https://www.sciencedirect.com/science/article/pii/S0012821X24004060
pdf: core_pack/gourbet_2024_hotspot_volcano_incision.pdf
tags: [volcanic-islands, river-incision, stream-power, climate, Réunion, Mauritius, Kauai, hotspot]
sprint_first_used: sprint_2
status: downloaded
---

## 一句话用途

Sprint 2 的 stream-power erosion 参数校准（`sprint_2_geomorph_credibility.md §RD2`）可以用本文在 Réunion / Mauritius / Kauaʻi 岛上的 Bayesian K 值反演结果作为"真实火山岛地貌演化"的经验参照——这是验证 Sprint 2 SPIM 参数合理性的关键外部证据。

## Abstract

Climate's role in governing landscape evolution has been intensely studied for several decades, but few studies clearly document climate-landscape interactions in natural landscapes. The researchers examined tropical hotspot volcanic islands (Réunion, Mauritius, and Kauaʻi) as natural laboratories to understand how climate influences erosion and geomorphic processes. They reconstructed paleotopography from relict volcanic features and compared it to modern topography to quantify eroded volumes. Using geochronology of volcanic flows to constrain timing, they determined basin-average erosion rates and calibrated stream power models through Bayesian inversion. Key findings indicate that basins eroding at rates below approximately 1 mm/yr demonstrate positive correlations with mean annual precipitation and negative correlations with erosion duration. The stream power parameters showed significant climate correlations primarily on Réunion Island, particularly with mean annual cyclonic precipitation. The research demonstrates that both mean annual precipitation and extreme events control long-term landscape evolution on volcanic islands.

## 关键方程 / 核心结论

- TODO (Sprint 1A first read)
- SPIM calibrated on real hotspot volcanic islands: K values from Bayesian inversion on Réunion + Mauritius + Kauaʻi
- Mean annual cyclonic precipitation (extreme events) controls long-term incision — relevant to rain shadow + cyclone track modeling in Sprint 1B/2
- Erosion rates < 1 mm/yr positively correlated with MAP; higher rates decoupled (threshold effects)

## 对本项目的落地点

- TODO (Sprint 1A first read)
- Sprint 2: `crates/sim/src/geomorph/` SPIM erosion stages — use Gourbet's K values as plausibility brackets when calibrating `preset.spim_K` (default `3e-4` in sprint_2 §RD2)
- Case studies (Layer C): Réunion + Mauritius + Kauaʻi are the target island archetypes for Sprint 2–3 calibration; Gourbet 2024 is the primary geomorphic reference for these case studies
- `docs/papers/case_studies/` — Réunion and Mauritius stubs to be created in Sprint 2 will cite Gourbet 2024 as the foundational erosion reference

## 值得警惕的点

- TODO (Sprint 1A)
- Gourbet uses real paleotopographic reconstruction from dated lava flows — our synthetic topography has no real-age constraint. K calibration from this paper is for plausibility checking, not direct parameter copy
- Mean Annual Cyclonic Precipitation is not the same as mean annual precipitation — extreme events matter more on high-relief islands; Sprint 2's rain shadow model must account for this distinction
