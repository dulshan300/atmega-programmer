//! Visual program pipeline: graph → validation → typed IR.

pub mod catalog;
pub mod compile;
pub mod ir;
pub mod templates;

pub use compile::compile_graph;
