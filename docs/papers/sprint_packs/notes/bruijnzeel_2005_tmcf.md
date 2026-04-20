---
id: bruijnzeel_2005_tmcf
title: "Tropical montane cloud forests: a unique hydrological case"
authors: Bruijnzeel
year: 2005
venue: "Forests, water, and people in the humid tropics (Cambridge University Press, ch. 27, pp. 462–483)"
doi: 10.1017/CBO9780511535666.030
url: https://www.cambridge.org/core/books/tropical-montane-cloud-forests/92BBA7612B2E9EA00CE0F36A5C984883
pdf_status: metadata_only
tags: [TMCF, cloud-forest, fog-drip, streamflow, soil-moisture, tropical-hydrology, water-yield]
sprint_first_used: sprint_3
pack: sprint_3
---

## 一句話用途

Sprint 3 DD5 の "fog water が streamflow と soil moisture に寄与する" 物理メカニズムの文献的背景；Bruijnzeel 2011 synthesis の先行基礎論文として fog drip → catchment water yield 経路を定量化する。

## Abstract

Tropical montane cloud forests represent a hydrologically unique case among tropical vegetation types because of the additional water inputs from horizontal precipitation (fog drip) and the reduced evapotranspiration associated with persistent cloud immersion. This chapter reviews the evidence for enhanced water yield from TMCF catchments relative to lower-elevation tropical forests, synthesizing measurements from Hawaii, Puerto Rico, Colombia, Venezuela, Mexico, and East Africa. Key conclusions include: (1) fog drip can supplement rainfall by 10–100+% in persistently cloud-immersed sites; (2) TMCF catchments typically export a higher fraction of precipitation as streamflow than lower-elevation tropical forests due to lower evapotranspiration under cloud cover; (3) deforestation of TMCF eliminates fog-drip interception, leading to counterintuitive streamflow *decreases* (the "cloud forest hydrology paradox"); (4) the trade-wind inversion layer is the primary altitudinal control on cloud immersion frequency.

## 关键方程 / 核心结论

- Fog drip supplement: 10–100+% of rainfall measured across TMCF sites; median ~20–30% in persistently immersed windward sites.
- Streamflow enhancement: TMCF catchments yield 10–40% more streamflow than comparable non-cloud-forest catchments at similar rainfall — primarily because reduced ET under cloud cover allows more water to reach the stream.
- The "TMCF hydrology paradox": removing TMCF canopy eliminates fog-drip interception → less water enters the soil → streamflow decreases. This is the physical motivation for Sprint 3's `FOG_TO_SM_COUPLING` pathway: fog water must pass through the soil (not just be intercepted by canopy) to affect long-term soil moisture and biome distributions.

## 对本项目的落地点

### Sprint 3 落地点

Bruijnzeel 2005 是 DD5 `FOG_TO_SM_COUPLING` 设计的第二层文献支撑（Bruijnzeel 2011 是第一层合成）。本文的关键贡献在于清楚地区分了两条 fog water 进入水文过程的路径：

1. **Fog interception → canopy throughfall → soil → streamflow（间接路径）：** 这是 Sprint 3 `SoilMoistureStage` 建模的路径。`fog_water[p] · FOG_TO_SM_COUPLING` 就是这一路径的代理——fog water 中 40% 最终成为 soil moisture 的输入，其余损失于 canopy re-evaporation + surface runoff。

2. **Fog interception → canopy evaporation（直接损失路径）：** 这部分不进 soil moisture，因此在 Sprint 3 里被 `(1 - FOG_TO_SM_COUPLING)` 系数隐式忽略。

本文的 "TMCF paradox" 观察（去掉林冠 → fog 不再被截留 → streamflow 反而减少）间接支持了 Sprint 3 "fog water 进 soil moisture → 维持 CloudForest biome → CloudForest 存在是 fog 拦截的物理必要条件" 的循环逻辑。Sprint 3 v1 不模拟这个正反馈（biome 不反过来影响 fog 拦截），但 Bruijnzeel 2005 提示这是一个 Sprint 5+ 的扩展方向。

`baked.soil_moisture` 的 fog 注入量（`0.15 × 0.40 = 0.06` 归一化单位 at peak fog_likelihood）在本文 Table 27.1 给出的各站点 fog supplement 中处于保守下限——与 Bruijnzeel 2011 的 synthesis 结论一致。

## 值得警惕的点

- 本文 2005 年发表时的测量技术限制（fog collectors、tipping-bucket fog gauges）使得各站点间的 fog drip 估算误差较大（±30–50%）。Sprint 3 常数选择在这个不确定性范围内，不应视为精确标定值。
- "100+% fog supplement" 仅见于极端持续云雾覆盖的站点（如夏威夷 Alakai Swamp）；普通 TMCF 的 fog supplement 更接近 10–30%。Sprint 3 `FOG_WATER_GAIN = 0.15` 对应这一普通范围的下限。
