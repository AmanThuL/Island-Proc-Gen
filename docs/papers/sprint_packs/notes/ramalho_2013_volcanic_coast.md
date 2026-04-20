---
id: ramalho_2013_volcanic_coast
title: "Coastal evolution on volcanic oceanic islands: A complex interplay between volcanism, erosion, sedimentation, sea-level change and biogenic production"
authors: Ramalho, Quartau, Trenhaile, Mitchell, Woodroffe, Ávila
year: 2013
venue: Earth-Science Reviews
doi: 10.1016/j.earscirev.2013.10.007
url: https://www.sciencedirect.com/science/article/abs/pii/S0012825213001736
pdf: ramalho_2013_volcanic_coast.pdf
tags: [volcanic-island, coastal-evolution, lava-delta, erosion, coast-type, geomorphology, hotspot-volcano]
sprint_first_used: sprint_3
status: downloaded
---

## 一句话用途

Sprint 3 DD6 的 CoastType v2 分类体系（16-dir fetch integral + LavaDelta 第 5 类）以本文为主参考：volcanic island 的 "young coast" 特征（lava delta、fresh bench、coastal asymmetry）及其随岛龄演化路径，为 `CoastTypeStage` 的 `age_bias` 判别逻辑提供物理立足点。

## Abstract

The growth and decay of oceanic hotspot volcanoes are intrinsically related to a competition between volcanic construction and erosive destruction, and coastlines are at the forefront of such confrontation. This paper reviews the several mechanisms that interact and contribute to the development of coastlines on oceanic island volcanoes, and how these processes evolve throughout the islands' lifetime. Volcanic constructional processes dominate during the emergent island and subaerial shield-building stages, with surtseyan activity prevailing during the emergent island stage and hydroclastic and pyroclastic structures forming that are generally ephemeral. As islands mature, destructive processes gradually take over and coastlines retreat through marine and fluvial erosion, mass wasting, and subsidence. Reef growth and/or uplift may also prolong the island's lifetime above the waves, though ultimately most islands become submerged. The paper synthesizes volcanism, erosion, sedimentation, sea-level change, and biogenic production as interacting drivers of volcanic island coastal change throughout the geological lifecycle, drawing on case studies from the Atlantic and Pacific.

## 关键方程 / 核心结论

- Lava delta formation is a "young coast" feature: lava flows reaching the ocean form bench-like deltas that are subsequently eroded by marine processes; marine erosion removes ~9× more volume than fluvial erosion from fresh lava deltas (from Rodriguez-Gonzalez et al. case studies cited by this review).
- Coastal morphology transitions: Young island → lava delta / bench / surtseyan tuff ring → Mature → cliff / platform / beach → Old → drowned reef / submarine bank. Coast type is a function of island age AND local wave exposure AND lithology.
- Wave exposure (fetch from open ocean) is the dominant discriminant for Cliff vs Beach on mature-to-old islands; slope and rock strength are secondary.
- Estuary / river-mouth coasts appear at any age where drainage integration has progressed far enough to deliver significant sediment flux.

## 対本項目의 落地点

### Sprint 3 落地点

Ramalho 2013 直接 ground DD6 的 5 类 `CoastType` v2 分类逻辑（`crates/sim/src/geomorph/coast_type.rs`）：

1. **LavaDelta（新增第 5 类）的物理依据：** 本文 §3.1 / §3.2 将 lava delta 定义为 "young volcanic island 近海相低缓斜坡"——正是 DD6 中 `age_bias > 0 (Young) && s ∈ [S_LAVA_LOW, S_LAVA_HIGH] && dist_vol < R_LAVA` 的判别条件的文字描述。`age_bias` 项从本文"岛龄驱动的 coast type 演化"框架中导出。

2. **fetch integral 的物理合理性：** 本文明确指出 wave exposure（来自各方向的 open-water fetch）是 Cliff vs RockyHeadland vs Beach 分类的主导变量，而非单一方向斜坡——DD6 改为 16-dir fetch integral 正是响应此观察。Sprint 2 v1 用 "single-dir slope + wind proxy" 等价于忽略了非主风方向的 swell；本文 §4 reef / platform 类型分布图可见明显的全周向不对称。

3. **island_age 作为 LavaDelta 判别输入之一：** 本文将 "Young / Mature / Old" 三阶段与 coast type 组成高度相关，Young 阶段 lava delta 丰度远高于 Mature / Old。`preset.island_age` 字段在 Sprint 0 就已入库；DD6 的 `age_bias = match preset.island_age { Young=1.0, Mature=0.0, Old=-1.0 }` 直接从本文框架中导出。

4. **与 Rodriguez-Gonzalez 2022 的关系：** 本文是 overview；Rodriguez-Gonzalez 2022 是 lava delta 的专项案例研究，提供了坡度/距离的定量阈值（`S_LAVA_LOW/HIGH`, `R_LAVA`）。两篇联合构成 DD6 LavaDelta 分类的完整文献支撑。

## 值得警惕的点

- 本文研究对象是以百万年为时间单位演化的地质过程；Sprint 3 的 `island_age` 是三态（Young/Mature/Old）的 preset 参数，是定性代理而非实际年代。不要把文中的具体 Ma 数字当作代码常量。
- Lava delta 的 "坡度低缓" 特征在归一化网格坐标下受 `max_relief` 压缩影响；`S_LAVA_LOW = 0.03` 是在 Sprint 2.6 校准地形上经验估算的，不是从文中直接读取的数值。
- 本文 Fig. 1 给出的 coast type 随岛龄演化框架来自大西洋 hotspot 案例（Azores / Canaries / Cape Verde）；太平洋多岛弧体系（Hawaii、French Polynesia）略有差异，特别是 fringing reef 的存在会延缓 cliff-retreat。Sprint 3 暂不模拟 reef，所以忽略 biogenic production 分支。
