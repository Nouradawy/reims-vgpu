//! IOSurface Registry: Persistent host GPU images per guest IOSurface plane.
//!
//! Clean-room implementation based on public Apple IOSurface and Khronos Vulkan specifications.
//! Replaces redundant guest memory re-materializations with persistent `VkImage`s,
//! tracked via a pure state machine and change-detection protocols.

pub mod census;
pub mod shadow;
pub mod state;

#[cfg(test)]
mod tests;

pub use state::{Action, ActionList, PlaneEvent, PlaneState};
