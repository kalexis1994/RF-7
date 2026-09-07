//! The RF-7 PLAY surface.
//!
//! RackForge mounts this in an iframe and talks to it over `postMessage`. The
//! surface never sees the engine: it sees the host's context — the catalog of
//! sounds, the program draft under edit with its typed editor tree, the
//! seventeen performance parameters — and asks the host to change things.
//!
//! The split is deliberate. [`model`] turns host JSON into state, [`render`]
//! turns state into HTML, [`client`] decides what to ask the host and what a
//! reply means; all three are plain Rust with tests. `browser` is the thin
//! wasm-only layer that touches the DOM.

#![recursion_limit = "512"]

#[cfg(target_arch = "wasm32")]
mod browser;
mod cartridge;
pub mod client;
pub mod diagram;
pub mod model;
pub mod render;

pub const PROTOCOL: &str = "rackforge.plugin.web@1";
pub const PLUGIN_ID: &str = "org.rackforge.rf7";
