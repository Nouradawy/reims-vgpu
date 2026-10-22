//! IOSurface Registry: Persistent host GPU images per guest IOSurface plane.
//!
//! Clean-room implementation based on public Apple IOSurface and Khronos Vulkan specifications.
//! Replaces redundant guest memory re-materializations with persistent `VkImage`s,
//! tracked via a pure state machine and change-detection protocols.

pub mod census;
pub mod readback;
pub mod shadow;
pub mod state;
pub mod upload;
pub mod views;

#[cfg(test)]
mod tests;

pub use readback::{complete_readback, evaluate_lock_sync, evaluate_readback_req, ReadbackAction};
pub use state::{Action, ActionList, PlaneEvent, PlaneState};
pub use upload::{compute_upload_plan, StrideMode, UploadDecision, UploadPlan, UploadRegion};
pub use views::{validate_subrect, SubrectViewKey, ViewCache, ViewDecline};
