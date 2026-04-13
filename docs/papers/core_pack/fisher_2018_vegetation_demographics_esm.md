---
id: fisher_2018_vegetation_demographics_esm
title: "Vegetation demographics in Earth System Models: A review of progress and priorities"
authors: Fisher, Koven, Arora, Christoffersen, Davies-Barnard, Duffy, Hajima, Kim, Knox, Lawrence, McCormack, Middleton, Poulter, Reich, Shadaydeh, Shinozaki, Slot, Smith, Takahashi, Tjoelker, Turetsky, Wieder, Wright, Xu, Zaehle, Zhu, Bonan, Meinshausen, Oreskes, Peng, Ryan, Shevliakova, Thornton
year: 2018
venue: Global Change Biology
doi: 10.1111/gcb.13910
url: https://pubmed.ncbi.nlm.nih.gov/28921829/
tags: [vegetation, demographics, ESM, carbon, plant-mortality, fire, disturbance]
sprint_first_used: sprint_1b
status: metadata_only
---

## 一句话用途

Sprint 1B 的植被模块设计时（`crates/sim/src/ecology/`，待建）用本文识别 ESM 植被人口统计模块的关键实现差距，避免 Sprint 1B 重蹈过度简化或过度复杂化的覆辙。

## Abstract

Numerous current efforts seek to improve the representation of ecosystem ecology and vegetation demographic processes within Earth System Models (ESMs). These developments are widely viewed as an important step in developing greater realism in predictions of future ecosystem states and fluxes. Increased realism, however, leads to increased model complexity, with new features raising a suite of ecological questions that require empirical constraints. Here, we review the developments that permit the representation of plant demographics in ESMs, and identify issues raised by these developments that highlight important gaps in ecological understanding. These issues inevitably translate into uncertainty in model projections but also allow models to be applied to new processes and questions concerning the dynamics of real-world ecosystems. We argue that stronger and more innovative connections to data, across the range of scales considered, are required to address these gaps in understanding. The development of first-generation land surface models as a unifying framework for ecophysiological understanding stimulated much research into plant physiological traits and gas exchange. Constraining predictions at ecologically relevant spatial and temporal scales will require a similar investment of effort and intensified inter-disciplinary communication.

## 关键方程 / 核心结论

- TODO (Sprint 1A first read)
- Key tension: demographic realism vs. computational tractability — same trade-off as Argles 2022
- Empirical constraints required: plant functional types, mortality rates, disturbance regimes — our v1 uses preset biome categories instead

## 对本项目的落地点

- TODO (Sprint 1A first read)
- `crates/sim/src/ecology/` (Sprint 1B) — biome weight calculation; Fisher 2018 is the literature reference for why Sprint 1B uses simplified biome suitability scores rather than full demographic model
- Pair with Argles 2022 for the Sprint 1B design decision meeting

## 值得警惕的点

- TODO (Sprint 1A)
- Fisher 2018 covers global ESMs with 100+ year projections; our island simulates landscape aesthetic quality, not real carbon budgets
- The "demographic equilibrium" assumption is valid at geological timescales, not necessarily at Sprint 2's 100-iteration LEM proxy
