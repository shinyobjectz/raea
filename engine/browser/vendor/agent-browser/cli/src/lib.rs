//! (raea) agent-browser as a library: the same modules main.rs declares, so a host can run the daemon's command
//! execution in its own process (`native::actions::execute_command` on a `DaemonState`) without the socket or the CLI.
//! Vendored from vercel-labs/agent-browser at dac13bb (0.39.1, Apache-2.0); this file and `autobins = false` and the
//! `[lib]` section in Cargo.toml are raea's changes (see ../RAEA-CHANGES).
#![allow(dead_code, unused_imports)]
pub mod ca_bundle;
pub mod chat;
pub mod color;
pub mod commands;
pub mod connection;
pub mod doctor;
pub mod flags;
pub mod install;
pub mod mcp;
pub mod native;
pub mod output;
pub mod plugins;
pub mod read;
pub mod skills;
#[cfg(test)]
pub mod test_utils;
pub mod tls;
pub mod upgrade;
pub mod validation;
