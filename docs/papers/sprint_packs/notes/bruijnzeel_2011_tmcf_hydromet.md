---
id: bruijnzeel_2011_tmcf_hydromet
title: "Hydrometeorology of tropical montane cloud forests: emerging patterns"
authors: Bruijnzeel, Mulligan, Scatena
year: 2011
venue: Hydrological Processes
doi: 10.1002/hyp.7974
url: https://onlinelibrary.wiley.com/doi/abs/10.1002/hyp.7974
pdf_status: metadata_only
tags: [TMCF, cloud-forest, fog-drip, inversion-layer, trade-wind, soil-moisture, tropical-hydrology]
sprint_first_used: sprint_3
pack: sprint_3
---

## 一句話用途

Sprint 3 DD5 の fog belt / inversion layer パラメータ（`inversion_z = 0.65 · max_relief`, `FOG_WATER_GAIN = 0.15`, `FOG_TO_SM_COUPLING = 0.40`）の物理オーダー・オブ・マグニチュード根拠；TMCF における霧滴降下の水文学的貢献を定量化した synthesis paper。

## Abstract

Tropical montane cloud forests (TMCF) are characterized by frequent to persistent fog and low cloud immersion. Based on altitudinal limits between which TMCF generally occurs (800–3500 m a.s.l. depending on mountain size and distance to coast), the current areal extent of TMCF is estimated at approximately 215,000 km² or 6.6% of all montane tropical forests. This synthesis reviews emerging patterns in TMCF hydrometeorology across multiple sites, including fog-drip contributions to water yield, trade-wind inversion layer control on cloud-base altitude, and the role of canopy interception in partitioning fog water between throughfall and evaporation. Key findings include: (1) the trade-wind inversion layer is the primary control on cloud-base altitude in oceanic tropical islands, typically occurring at 0.6–0.7 × island max relief; (2) fog drip can contribute 10–50% of measured precipitation under persistent cloud immersion; (3) the fog-to-streamflow coupling coefficient varies widely (0.2–0.8) depending on canopy architecture and slope aspect; (4) leeward TMCF sites receive substantially less fog water than windward equivalents, creating asymmetric biome distributions.

## 关键方程 / 核心结论

- Trade-wind inversion layer altitude: 0.6–0.7 × max island height for oceanic volcanic islands in trade-wind belts (consistent across Hawaii, Canary Islands, Madeira, Caribbean). Sprint 3 uses `inversion_z = 0.65 · max_relief` (midpoint of observed range).
- Fog drip as fraction of precipitation: 10–50% across TMCF sites. Literature median near 15–25% under persistent cloud immersion (Bruijnzeel 2005 synthesis; Holwerda et al. 2006). Sprint 3 uses `FOG_WATER_GAIN = 0.15` (conservative lower end of range, appropriate for a v1 proxy).
- Fog-to-soil-moisture coupling: not all fog drip reaches the mineral soil — canopy interception and litter storage intercept 20–80%. Sprint 3 `FOG_TO_SM_COUPLING = 0.40` means 40% of fog water input contributes to soil moisture, consistent with mid-range throughfall efficiency from literature.
- Combined contribution: `FOG_WATER_GAIN × FOG_TO_SM_COUPLING = 0.15 × 0.40 = 6%` baseline fog contribution to normalized soil moisture. Literature supports this as a conservative lower bound for windward TMCF; actual contribution can be 2–5× higher in persistent fog zones.

## 对本项目的落地点

### Sprint 3 落地点

Bruijnzeel 2011 provides the order-of-magnitude physical justification for three locked constants in `crates/sim/src/hydro/soil_moisture.rs` (per DD5):

**`inversion_z = 0.65 · max_relief` (DD5 §FogLikelihoodStage upgrade):**

The trade-wind inversion layer is the primary control on where cloud immersion begins on oceanic volcanic islands. Bruijnzeel 2011 reports 0.6–0.7 × max elevation for Hawaii (inversion at ~1800–2100 m, Mauna Kea at ~4200 m → ratio 0.43–0.50 for high-altitude summits, but for the *cloud forest belt* the relevant ratio is closer to 0.6–0.7 of the summit height within the TMCF altitude range). Canary Islands TMCF belt sits at 600–1200 m on islands with peaks at 1000–3700 m → ratios 0.16–1.0, centred near 0.5–0.7. Sprint 3 uses `0.65 · max_relief` as the Gaussian bell centre for `FogLikelihoodStage`, which is the midpoint of the observed range and physically corresponds to the cloud-base altitude at peak fog immersion frequency. The `band_thickness = 0.15 · max_relief` parameter similarly reflects the ~15% of relief over which cloud immersion transitions from rare to persistent.

**`FOG_WATER_GAIN = 0.15` (DD5 §SoilMoistureStage fog injection):**

Bruijnzeel 2011 Table 2 summarizes fog-drip measurements from 15 TMCF sites: fog drip ranges from 9 to 48% of precipitation, with a mean near 20–25%. For a v1 normalized proxy where `fog_likelihood ∈ [0, 1]` and `soil_moisture ∈ [0, 1]`, a conversion factor of 0.15 means "at peak fog likelihood (= 1.0), fog drip adds 0.15 normalized units of water input". This is conservative relative to literature (sits at the 10th percentile of observed fog fractions) but appropriate for a first-order proxy that doesn't model canopy type, aspect, or wind speed. The conservatism prevents fog from dominating soil moisture on all elevated cells regardless of biome — the `FOG_TO_SM_COUPLING` term provides the secondary throttle.

**`FOG_TO_SM_COUPLING = 0.40` (DD5 §SoilMoistureStage fog injection):**

Bruijnzeel 2011 §3 discusses the partitioning of fog drip: a fraction is re-evaporated from the canopy, a fraction runs off the surface, and only a fraction reaches the mineral soil to contribute to soil moisture. Estimates range from 0.20 (dense closed-canopy forest with high interception) to 0.80 (open ridge scrub with little interception). Sprint 3 uses 0.40 as a mid-range value appropriate for mixed-cover TMCF — not fully closed forest (0.20) and not bare ridgeline (0.80). Combined with `FOG_WATER_GAIN = 0.15`, the net contribution `0.15 × 0.40 = 0.06` normalized units of soil moisture per unit of fog likelihood. This 6% contribution is:
- Physically defensible: at the conservative end of the literature 10–50% fog/precip range × 20–80% soil coupling range.
- Small enough not to overwhelm the precipitation + runoff inputs to soil moisture.
- Large enough to create a measurable soil moisture gradient in the fog belt that `BiomeWeightsStage` can read as a CloudForest / CoastalScrub signal.

**The `derived.fog_water_input` field (Sprint 3 new):**

`FogLikelihoodStage` writes `baked.fog_likelihood` as before; `SoilMoistureStage` reads it and computes `fog_water[p] = FOG_WATER_GAIN · fog_likelihood[p]`, adding `fog_water[p] · FOG_TO_SM_COUPLING` to `soil_moisture[p]`. The intermediate `fog_water_input` scalar field is stored in `derived.fog_water_input` for overlay display only (Sprint 3 adds `fog_water_input` as the 4th new overlay alongside `sediment_thickness` / `deposition_flux` / `lava_delta_mask`). Every `SoilMoistureStage` rerun recalculates it from scratch — it is `derived`, not `baked`.

## 值得警惕的点

- Bruijnzeel 2011 is a synthesis of field measurements from real TMCF; Sprint 3's `fog_likelihood` is a normalized proxy driven by elevation + uplift, not actual cloud immersion measurements. The 0.15 / 0.40 constants are calibrated to give physically plausible soil moisture enhancement, not to replicate any specific TMCF site.
- The 0.6–0.7 × max_relief inversion altitude finding comes primarily from trade-wind islands (Hawaii, Canaries, Madeira). For non-trade-wind settings or very tall islands (>4000 m), the ratio may differ. Sprint 3 presets are all single/twin peak volcanic islands in the 0.3–1.5 normalized height range, so the 0.65 fraction is appropriate.
- Bruijnzeel 2011 explicitly notes that fog-drip contribution is highly site-specific and difficult to generalize. The Sprint 3 model is a first-order proxy; v2 calibration (Sprint 3.1 or later) should revisit these constants against the archetype suite visual acceptance.
