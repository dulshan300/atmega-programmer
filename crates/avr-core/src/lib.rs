//! Core domain logic for AVR Studio Visual.
//!
//! This crate is deliberately independent of Tauri so that every rule that
//! matters for correctness (device data, project persistence, graph
//! validation, code generation, toolchain and programmer invocation) can be
//! tested headlessly on any host.

pub mod build;
pub mod codegen;
pub mod config_validation;
pub mod device_registry;
pub mod diagnostics;
pub mod errors;
pub mod fuses;
pub mod graph;
pub mod memory;
pub mod paths;
pub mod process;
pub mod programmer;
pub mod project_store;
pub mod serial;
pub mod settings;
pub mod toolchains;

pub use errors::{CoreError, CoreResult};
