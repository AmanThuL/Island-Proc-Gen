---
id: lague_2014_spim_evidence
title: "The stream power river incision model: Evidence, theory and beyond"
authors: Lague
year: 2014
venue: Earth Surface Processes and Landforms
doi: 10.1002/esp.3462
url: https://onlinelibrary.wiley.com/doi/10.1002/esp.3462
pdf: core_pack/lague_2014_spim_evidence.pdf
tags: [stream-power, river-incision, SPIM, threshold-stochastic, knickpoint, geomorphology]
sprint_first_used: sprint_2
pack: sprint_2
pdf_status: downloaded
---

## Sprint 2 落地点

Sprint 2 DD1 `(m, n) = (0.35, 1.0)` 의 경험적 근거는 본 논문의 현장 데이터 지점 컴파일로부터 나온다. Lague 2014는 다양한 지질학적 환경에서 측정된 stream-power 지수의 광범위한 통계를 제시하며, field evidence에서 m은 대체로 0.3–0.5 범위에, n은 knickpoint 상류 steady-state 구간에서 1.0에 가깝다는 점을 보여준다. Sprint 2의 상수-K 정방향 오일러 적분 선택과 `n=1.0` (선형 경사 의존성)은 Lague가 식별한 knickpoint 전파 근사에 따른 것이다. `crates/sim/src/geomorph/stream_power.rs` 의 `SPIM_K_DEFAULT` 스칼라 값은 v1 해상도(256²)에서 100 반복 내 눈에 띄는 지형 감소를 생산하도록 보정되었으며, 이는 K를 암석 강도·유출·역값의 복합 변수로 해석하는 Lague의 프레임워크와 일치한다.
