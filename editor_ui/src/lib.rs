//! Editor UI - Rendering and input handling.
//!
//! This crate provides GPU-accelerated text rendering using wgpu
//! and input handling using winit.

pub mod app;
pub mod font;
pub mod gpu_renderer;
pub mod input;

// Keep the old renderer module for reference, but it's deprecated
#[deprecated(note = "Use gpu_renderer instead")]
pub mod renderer;

pub use app::{run, EditorApp};
pub use gpu_renderer::GpuRenderer;
