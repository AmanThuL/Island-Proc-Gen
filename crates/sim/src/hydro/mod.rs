//! Hydrological simulation stages.
//!
//! Sprint 1A: Task 1A.5 `FlowRoutingStage`, Task 1A.6 `AccumulationStage`,
//! Task 1A.7 `BasinsStage`, Task 1A.8 `RiverExtractionStage`.

pub mod accumulation;
pub mod basins;
pub mod flow_routing;
pub mod rivers;

pub use accumulation::AccumulationStage;
pub use basins::BasinsStage;
pub use flow_routing::FlowRoutingStage;
pub use rivers::RiverExtractionStage;

/// D8 neighbour offset table. Index 0..7 encoded in `flow_dir[p]`.
///
/// Order: E, NE, N, NW, W, SW, S, SE — clockwise from east.
pub(crate) const D8_OFFSETS: [(i32, i32); 8] = [
    ( 1,  0), // 0: E
    ( 1, -1), // 1: NE
    ( 0, -1), // 2: N
    (-1, -1), // 3: NW
    (-1,  0), // 4: W
    (-1,  1), // 5: SW
    ( 0,  1), // 6: S
    ( 1,  1), // 7: SE
];

/// Sentinel value written to `flow_dir` for coast cells, sea cells, and
/// genuine sinks (should not occur on interior land after pit fill).
pub(crate) const FLOW_DIR_SINK: u8 = 0xFF;
