pub mod agent;
pub mod cli;
pub mod config;
pub mod diff;
pub mod docs;
pub mod engine;
pub mod error;
pub mod git;
pub mod output;
pub mod prompt;
pub mod state;

pub use error::{AutospecError, Result};
