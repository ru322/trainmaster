//! 重み付き有向グラフの全点対最短経路と経路の復元。ゲームの概念は持たない。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod apsp;
mod dijkstra;
mod floyd;
mod graph;
mod reverse;
mod table;

#[cfg(test)]
mod brute;
#[cfg(test)]
mod proptests;
#[cfg(test)]
mod testgen;
#[cfg(test)]
mod tests;

pub use apsp::AllPairsShortestPath;
pub use dijkstra::RepeatedDijkstra;
pub use floyd::FloydWarshall;
pub use graph::{Cost, Graph, MAX_EDGE_COST, MAX_NODES, NodeId};
pub use reverse::ReverseDijkstra;
pub use table::{Path, RouteTable, build_next_hops};
