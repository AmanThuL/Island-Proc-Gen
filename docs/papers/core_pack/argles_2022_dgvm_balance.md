---
id: argles_2022_dgvm_balance
title: "Dynamic Global Vegetation Models: Searching for the balance between demographic process representation and computational tractability"
authors: Argles, Moore, Cox
year: 2022
venue: PLOS Climate
doi: 10.1371/journal.pclm.0000068
url: https://journals.plos.org/climate/article?id=10.1371/journal.pclm.0000068
pdf: core_pack/argles_2022_dgvm_balance.pdf
tags: [vegetation, DGVM, demographics, ESM, carbon, tractability]
sprint_first_used: sprint_1b
status: downloaded
---

## 一句话用途

Sprint 1B 的植被模块（`crates/sim/src/ecology/`，待建）设计时用本文的 DGVM 分类框架（Individual / Average-Area / 2D-Cohort / 1D-Cohort）判断应实现哪个复杂度层级——Argles 的结论是"1D Cohort 模型提供最佳可行性与真实性平衡"。

## Abstract

Vegetation experiences multiple 21st-century pressures including climate shifts, atmospheric changes, and human land use modifications. These alterations feedback to climate through impacts on carbon and water surface-atmosphere fluxes. Dynamic Global Vegetation Models (DGVMs) serve as key Earth System Model components, though future land carbon sink projections show wide variation due to challenges in representing complex ecosystem processes at large scales (approximately 100km grid lengths). The authors categorize DGVMs into four groups: Individual, Average Area, Two Dimensional Cohort, and One Dimensional Cohort models. Their analysis suggests that tree size distributions within forests represent the minimum complexity necessary for effectively modeling carbon storage changes under shifting climate and disturbance conditions. They find that observed size distributions align with Demographic Equilibrium Theory, and propose that One Dimensional Cohort models with a focus on tree size offer the best balance between computational tractability and realism for Earth System Model applications.

## 关键方程 / 核心结论

- TODO (Sprint 1A first read)
- Four DGVM categories: Individual (most complex), Average Area (least), 2D Cohort, 1D Cohort (recommended balance)
- Minimum necessary complexity: tree size distribution representation — relevant to Sprint 1B's biome suitability design

## 对本项目的落地点

- TODO (Sprint 1A first read)
- `crates/sim/src/ecology/` (Sprint 1B) — biome suitability weights (`world.baked.biome_weights`) design decision: which DGVM category to approximate?
- Fisher 2018 is a complementary read for the same Sprint 1B decision

## 值得警惕的点

- TODO (Sprint 1A)
- DGVMs are designed for ESM-scale (100km grids); our island grid is 256×256 normalized cells (~1km equivalent). Scale assumptions may not transfer
- Carbon cycle focus may not map cleanly to our visual biome classification goals
