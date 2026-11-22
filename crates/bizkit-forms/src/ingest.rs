//! Public form ingest: body parsing, bot checks and the routes.

pub mod bot;
pub mod parse;
pub(crate) mod route;

pub use self::route::{Accepted, NOTIFY_PER_HOUR, POW_MAX_NUMBER, SPAM_STORED_PER_HOUR};
