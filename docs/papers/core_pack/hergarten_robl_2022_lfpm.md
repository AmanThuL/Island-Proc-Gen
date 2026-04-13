---
id: hergarten_robl_2022_lfpm
title: "The linear feedback precipitation model (LFPM 1.0) – a simple and efficient model for orographic precipitation in the context of landform evolution modeling"
authors: Hergarten, Robl
year: 2022
venue: Geoscientific Model Development
doi: 10.5194/gmd-15-2063-2022
url: https://gmd.copernicus.org/articles/15/2063/2022/
tags: [orographic-precipitation, linear-model, LEM-coupling, moisture, efficiency]
sprint_first_used: sprint_2
status: metadata_only
---

## 一句话用途

Sprint 2 の降水 v2（`crates/sim/src/climate/precipitation.rs`）において、Smith-Barstad FFT の代替として LFPM の二成分線形モデル（水蒸気 + 雲水）を採用する際の実装仕様と理論的根拠。

## Abstract

The influence of climate on landform evolution has attracted great interest over the past decades. While many studies aim at determining erosion rates or parameters of erosion models, feedbacks between tectonics, climate, and landform evolution have been discussed but addressed quantitatively only in a few modeling studies. One of the problems in this field is that coupling a large-scale landform evolution model with a regional climate model would dramatically increase the theoretical and numerical complexity. Only a few simple models have been made available so far that allow efficient numerical coupling between topography-controlled precipitation and erosion. This paper fills this gap by introducing a quite simple approach involving two vertically integrated moisture components (vapor and cloud water). The interaction between the two components is linear and depends on altitude. This model structure is in principle the simplest approach that is able to predict both orographic precipitation at small scales and a large-scale decrease in precipitation over continental areas without introducing additional assumptions. Even in combination with transversal dispersion and elevation-dependent evapotranspiration, the model is of linear time complexity and increases the computing effort of efficient large-scale landform evolution models only moderately. Simple numerical experiments applying such a coupled landform evolution model show the strong impact of spatial precipitation gradients on mountain range geometry including steepness and peak elevation, position of the principal drainage divide, and drainage network properties.

## 关键方程 / 核心结论

- TODO (Sprint 1A first read)
- Two-component moisture model: vapor `q_v` and cloud water `q_c`, linearly coupled via altitude
- Linear time complexity O(N) — much cheaper than Smith-Barstad FFT O(N log N) for large grids
- Can reproduce both local orographic precipitation AND large-scale continental precipitation decrease

## 对本项目的落地点

- TODO (Sprint 1A first read)
- `crates/sim/src/climate/precipitation.rs` (Sprint 2 v2) — LFPM is the leading candidate for the Sprint 2 precipitation upgrade (over Smith-Barstad FFT) because of its O(N) coupling efficiency with the erosion loop
- `sprint_2_geomorph_credibility.md §4` ("Rain shadow v2") is where this paper will be deeply read

## 值得警惕的点

- TODO (Sprint 1A)
- LFPM is designed for large mountain ranges (continent scale); its assumptions about moisture advection over flat ocean may need tuning for small isolated volcanic islands
- "Linear" in the model name refers to the vapor-cloud interaction being linear in altitude, NOT that precipitation response to topography is linear — potential naming confusion
