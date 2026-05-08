//! Errors raised by [`crate::init`]. Each variant identifies which step of
//! the startup sequence failed so callers can display a precise diagnostic
//! at the boundary.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    #[error("failed to create tokio runtime")]
    Runtime(#[source] io::Error),

    #[error("failed to initialise local database")]
    Database(#[source] anyhow::Error),

    #[error("failed to load saved terminal session")]
    SessionLoad(#[source] anyhow::Error),

    #[error("failed to read terminal registration")]
    Registration(#[source] anyhow::Error),
}
