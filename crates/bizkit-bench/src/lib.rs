//! A load benchmark for analytics ingest pipelines: `serve` runs a pipeline
//! design inside a resource-capped container, `load` drives it from outside,
//! `count` finds acked events a crash lost, and `report` tabulates runs.

pub mod pipeline;
pub mod store;
