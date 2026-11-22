//! Test harness for bizkit: fakes for every side effect, a real app on a
//! temp-dir database, a client that checks each response against the app's
//! own OpenAPI document, and an MCP client.

pub mod fakes;
pub mod mcp_client;
pub mod spec_client;
pub mod test_app;

pub use bizkit_core::dev::personas::{Manifest, Persona};

pub use crate::fakes::{FakeClock, FakeHttp, FakeMailer, MemBlobs};
pub use crate::mcp_client::McpClient;
pub use crate::spec_client::{ApiResponse, RawBody, SpecChecker, SpecClient, Typed};
pub use crate::test_app::{BASE_URL, TestApp, TestAppBuilder};
