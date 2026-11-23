//! A load benchmark for analytics ingest pipelines: `serve` runs a pipeline
//! design inside a resource-capped container, `load` drives it from outside,
//! `finish` adds container stats and crash loss, and `report` tabulates runs.

pub mod load;
pub mod pipeline;
pub mod report;
pub mod serve;
pub mod store;
