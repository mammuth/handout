//! handout — share agent-generated files with recipients via links.
//! Terminology (owner, recipient, file, version, invite, access, gate, open) is defined in AGENTS.md.

pub mod api;
pub mod app;
pub mod auth;
pub mod blobs;
pub mod config;
pub mod content;
pub mod db;
pub mod error;
pub mod ids;
pub mod markdown;
pub mod mcp;
pub mod models;
pub mod oauth;
pub mod pdf;
pub mod ratelimit;
pub mod service;
pub mod shell;
pub mod view;
pub mod webauth;

pub use app::{AppState, build_app};
pub use config::Config;
