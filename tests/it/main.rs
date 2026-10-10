//! Real compositions: service + machine + transport + emulator, settings files.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[cfg(unix)]
mod import_plan_stream;
mod settings_files;
#[cfg(unix)]
mod stream_over_pty;
