---
id: chen_2014_lem_review
title: "Landscape evolution models: A review of their fundamental equations"
authors: Chen, Darbon, Morel
year: 2014
venue: Geomorphology
doi: 10.1016/j.geomorph.2014.04.037
url: https://www.sciencedirect.com/science/article/pii/S0169555X14002402
pdf: core_pack/chen_2014_lem_review.pdf
tags: [landscape-evolution, PDE, stream-incision, hillslope, sediment, review]
sprint_first_used: sprint_0
pack: sprint_2
pdf_status: metadata_only
---

## Sprint 2 落地点

Sprint 2 DD1 + DD2 의 carve→smooth 순서는 본 논문 §3 의 erosion-diffusion coupling 논의에서 직접 근거를 얻는다. Chen 2014는 두 프로세스의 상호작용과 결합 방식을 체계적으로 분석하여, 각 외부 루프 반복에서 stream-power incision (carve)을 먼저 실행한 후 hillslope diffusion (smooth)을 실행하는 순서가 물리적으로 타당함을 보여준다. 이 순서는 급한 협곡에서의 과도한 확산을 피하면서 동시에 국소 경사에 비례한 평탄화를 허용한다. `crates/sim/src/geomorph/stream_power.rs` 와 `crates/sim/src/geomorph/hillslope.rs` 의 단계별 실행 순서는 이 논리적 근거에 직접 대응된다.
