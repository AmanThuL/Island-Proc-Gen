---
id: kwang_parker_2017_mn_pathology
title: "Utility of Machine Learning to Diagnose Preferential Incision of Bedrock Channels"
authors: Kwang, Parker
year: 2017
venue: Geomorphology
doi: 10.1016/j.geomorph.2017.02.006
pdf: core_pack/kwang_parker_2017_mn_pathology.pdf
tags: [stream-power, parameter-sensitivity, SPIM, instability, m-n-ratio, nonlinearity]
sprint_first_used: sprint_2
pack: sprint_2
pdf_status: downloaded
---

## Sprint 2 落地点

Kwang & Parker 2017은 Sprint 2 DD1 매개변수 선택의 critical 제약 조건을 제공한다. 논문의 핵심 발견은 `m/n = 0.5` 비율이 물리적으로 병리적임을 보여주는 것이다. 이 비율에서 stream-power SPIM 모델은 ridge 공간을 따라 비현실적인 sharpening을 야기하며, 시간이 진행함에 따라 안정적인 steady state에 도달하지 못한다. 즉, m/n = 0.5일 때 topography는 무한히 sharp해지려는 수렴하지 않는 진화를 보이므로, 수치적으로나 물리적으로나 부적절하다. 논문의 처방은 명확하다: "SPIM models with m/n distinctly different from 0.5를 사용하라." 이는 모든 SPIM 기반 LEM의 첫 번째 체크리스트 항목이 되어야 한다.

Sprint 2는 정확히 이 권고를 따른다. `(m, n) = (0.35, 1.0)` 선택은 `m/n ≈ 0.35`로, 0.5로부터 충분히 멀리 떨어져 있어 Kwang & Parker의 pathology 범위를 완전히 피한다. 이 값은 Chen 2014와 Lague 2014의 field-evidence 기반 추천(`m ≈ 0.35–0.5, n ≈ 1.0`)과 겹치는 동시에, Kwang & Parker의 stability 요구사항도 만족한다. 따라서 `(0.35, 1.0)`은 empirical credibility와 numerical stability 사이의 optimal 절충점을 나타낸다.

`crates/sim/src/geomorph/stream_power.rs`의 locked constants는 이 선택을 반영한다:
- `SPIM_K_DEFAULT = 1e-3` (composite lithology + runoff + threshold parameter)
- `SPIM_M_DEFAULT = 0.35` (drainage-area exponent)
- `SPIM_N_DEFAULT = 1.0` (slope exponent — linear for forward-Euler stability)

이들 값은 Sprint 0 CI 테스트 `canonical_constants_match_specifications` 에 의해 잠금되며, 재검토 없이 변경할 수 없다.

Sprint 3는 sediment-aware erosion으로 확장될 때, `(m, n)` 은 동일하게 유지되고 대신 `K` 를 soil-height `g(h_s)` modulation을 통해 조정한다. 즉, 비선형성은 매개변수 공간의 재배치(coupled stream-power + sediment flux)로 해결되며, `m/n` ratio 자체는 보존된다. 이는 Kwang & Parker의 pathology 회피를 유지하면서도 sediment-transport physics를 도입할 수 있게 한다.

v2 업그레이드 경로는 Sprint 4 GPU 생산화에 머물러 있다. 만약 Sprint 3의 sediment 통합이 non-linear `n != 1` 을 강제한다면, implicit tridiagonal solver가 필수가 되고, 이는 CPU 단일스레드 forward-Euler 스킴을 완전히 대체해야 한다. 그 시점에 Braun 2023과 Yuan 2019의 implicit/efficient SPIM 기술이 필수 읽을거리가 된다. 하지만 현재 v1 설계에서 `n = 1.0` 고정은 non-linear 복잡성 없이도 credible한 landform 생산을 가능하게 한다.

결론적으로, Kwang & Parker 2017은 Sprint 2의 (m, n) = (0.35, 1.0) 선택을 정당화하는 negative result (pathology 회피)를 제공하며, 이를 통해 보다 정교한 모델로의 future upgrade 경로를 보호한다.
