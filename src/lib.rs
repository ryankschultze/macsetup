//! macsetup: a terminal dashboard for a macOS setup engine. The binary is a
//! thin shell around this library so the state and rendering can be tested
//! without a terminal.

pub mod action;
pub mod activity;
pub mod app;
pub mod config;
pub mod engine;
pub mod events;
pub mod model;
pub mod theme;
pub mod ui;
