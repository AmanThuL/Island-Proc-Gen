---
id: whipple_tucker_1999_spim_dynamics
title: "Dynamics of the stream-power river incision model: Implications for height limits of mountain ranges, landscape response timescales, and research needs"
authors: Whipple, Tucker
year: 1999
venue: Journal of Geophysical Research: Solid Earth
doi: 10.1029/1999JB900120
url: https://agupubs.onlinelibrary.wiley.com/doi/10.1029/1999JB900120
pdf: core_pack/whipple_tucker_1999_spim_dynamics.pdf
tags: [stream-power, river-incision, SPIM, mountain-relief, response-time, uplift-erosion]
sprint_first_used: sprint_2
pack: sprint_2
pdf_status: downloaded
---

## Sprint 2 落地点

Sprint 2 DD1 `n = 1.0` 선형-경사 선택의 고전적 참고 자료이다. 비선형 `n != 1` 은 implicit tridiagonal solver를 필요로 하며, 이는 Sprint 4로 미루어진다. Whipple & Tucker 1999는 stream-power 모델의 동역학이 single nondimensional "uplift-erosion number"에 의해 지배됨을 보여주며, slope exponent n이 응답 시간 감도와 평형 경사도에 critical한 영향을 미친다고 설명한다. Sprint 2의 v1 설계가 `n=1.0` (선형 응답)을 선택한 이유는 정확히 이것이 정방향 오일러 안정성을 허용하기 때문이다: 안정적 dt에 대한 CFL 유사 제약은 경사 지수에 선형이며, 지수적이 아니다. `n > 1` 비선형 설계는 실시간 계산 시간 증가로 인한 implicit 스킴이 필수 불가결하며, Sprint 4 GPU 생산화까지 미루어진다. 따라서 Sprint 2는 `n=1.0` 을 명시적 forward-Euler `dt=1.0`으로 수행하며, Whipple & Tucker는 이것이 현실적 K와 A 값에 대해 무조건 안정적임을 보여준다.
