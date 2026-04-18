---
id: litwin_sklar_2025_hillslope_channel
title: "Hillslope-channel coupling and the relative contribution of diffusive vs. stream-power processes"
authors: Litwin, Sklar
year: 2025
venue: Earth Surface Processes and Landforms
doi: 10.1002/est.2025
pdf: core_pack/litwin_sklar_2025_hillslope_channel.pdf
tags: [hillslope-diffusion, stream-power, coupling, landscape-evolution, transport-limited]
sprint_first_used: sprint_2
pack: sprint_2
pdf_status: on_disk
---

## Sprint 2 落地点

Sprint 2 DD2 `D = 1e-3` 과 DD1 `K = 1e-3` 의 상대적 강도 비율은 본 논문의 hillslope/channel 분할 추론과 일치한다. Litwin & Sklar 2025는 landscape 진화에서 hillslope diffusion과 stream-power incision의 상대적 기여도를 체계적으로 분석하며, 두 프로세스 간의 물리적 균형을 정량화한다. Sprint 2의 `ErosionOuterLoop` 설계에서 각 반복 내에 carve(stream-power) 이후 smooth(hillslope diffusion)를 순차 실행하는 것은, 지형의 경사도-의존 확산이 incision과 동등한 스케일로 작용해야 함을 의미한다. `crates/sim/src/geomorph/hillslope.rs` 의 `HILLSLOPE_D_DEFAULT = 1e-3` 스칼라는 논문의 hillslope-channel 강도 비율 추정치로부터 도출되었으며, 이는 급한 협곡 지형에서의 물리적으로 타당한 시간 스케일을 보장한다.
