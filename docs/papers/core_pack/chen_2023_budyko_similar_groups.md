---
id: chen_2023_budyko_similar_groups
title: "Revisiting the hydrological basis of the Budyko framework with the principle of hydrologically similar groups"
authors: Chen, Chen, Xue, Yang, Zheng, Cao, Yan, Yuan
year: 2023
venue: Hydrology and Earth System Sciences
doi: 10.5194/hess-27-1929-2023
url: https://hess.copernicus.org/articles/27/1929/2023/
pdf: core_pack/chen_2023_budyko_similar_groups.pdf
tags: [hydrology, Budyko, water-balance, evapotranspiration, soil-moisture, vegetation]
sprint_first_used: sprint_1b
status: downloaded
---

## 一句话用途

Sprint 1B 的 Budyko 水量平衡模块（`crates/sim/src/climate/` 土壤湿度计算，待建）会用本文的 "hydrologically similar groups" 方法量化 Pw 参数——这是把 Budyko 框架从全球平均拉回到海岛特定气候分组的理论依据。

## Abstract

The Budyko framework is a simple and effective tool for estimating the water balance of watersheds. Quantification of the watershed-characteristic-related parameter (Pw) is critical for accurate water balance simulations with the Budyko framework. However, there is no universal method for calculating Pw as the interactions between hydrologic, climatic, and watershed characteristic factors differ greatly across watersheds. To fill this research gap, this study introduced the principle of hydrologically similar groups into the Budyko framework for quantifying the Pw of watersheds in similar environments. We first classified the 366 selected watersheds worldwide into six hydrologically similar groups based on watershed attributes, including climate, soil, and vegetation. Results show that soil moisture (SM) and fractional vegetation cover (FVC) are two controlling factors of the Pw in each group. The SM exhibits a power-law relationship with the Pw values, with increasing SM leading to higher Pw values in dry watersheds (SM ≤ 20 mm) and lower Pw values in humid watersheds (SM > 20 mm). Additionally, the FVC shows to be linearly correlated with the Pw values in most hydrologically similar groups, except in that group with moist soil and no strong rainfall seasonality (SM > 20 mm and seasonal index (SI) ≤ 0.4). Multiple non-linear regression models between Pw and the controlling factors (SM and FVC) were developed to individually estimate the Pw of six hydrologically similar groups. Cross-validations using the bootstrap sampling method (R²=0.63) and validations of time-series Global Runoff Data Centre (GRDC) data (R²=0.89) both indicate that the proposed models perform satisfactorily in estimating the Pw parameter in the Budyko framework. Overall, this study is a new attempt to quantify the unknown Pw in the Budyko framework using the method for hydrologically similar groups. The results will be helpful in improving the applicability of the Budyko framework for estimating the annual runoff of watersheds in diverse climates and with different characteristics.

## 关键方程 / 核心结论

- TODO (Sprint 1A first read)
- Budyko curve: `ET/P = f(PET/P)` where Pw parameterizes basin-specific behavior
- SM and FVC as two controlling factors for Pw — maps to `world.baked.soil_moisture` and biome vegetation fraction in Sprint 1B
- Six watershed groups → for volcanic islands, likely the "moist, high seasonality" group (tropical cyclone belt)

## 对本项目的落地点

- TODO (Sprint 1A first read)
- `crates/sim/src/climate/` (Sprint 1B) — water balance stage will use Budyko framework; this paper provides parameter calibration method
- `world.baked.soil_moisture: Option<ScalarField2D<f32>>` (Sprint 1B field) is directly the SM variable this paper puts at center stage

## 值得警惕的点

- TODO (Sprint 1A)
- Paper uses monthly data from 366 global watersheds; tropical volcanic islands are likely a small fraction — the Pw relationships may not generalize well to small, high-relief islands
- Budyko framework is an annual mean approximation; cyclone-driven precipitation is episodic — this mismatch is documented in Gourbet 2024
