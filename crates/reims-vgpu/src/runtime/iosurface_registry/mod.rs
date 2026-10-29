//! IOSurface Registry: Persistent host GPU images per guest IOSurface plane.
//!
//! Clean-room implementation based on public Apple IOSurface and Khronos Vulkan specifications.
//! Replaces redundant guest memory re-materializations with persistent `VkImage`s,
//! tracked via a pure state machine and change-detection protocols.

pub mod budget;
pub mod census;
pub mod formats;
pub mod readback;
pub mod shadow;
pub mod state;
pub mod upload;
pub mod views;

#[cfg(test)]
mod tests;

pub use budget::{
    calculate_plane_bytes, BudgetEntry, EvictionAction, Purgeability, Residency, RestoreDecision,
    SurfaceKey, VramBudgetManager, DEFAULT_MAX_WRITEBACK_BYTES_PER_FRAME,
    DEFAULT_UNUSED_FRAME_THRESHOLD, DEFAULT_VRAM_BUDGET_BYTES,
};
pub use readback::{complete_readback, evaluate_lock_sync, evaluate_readback_req, ReadbackAction};
pub use state::{Action, ActionList, PlaneEvent, PlaneState};
pub use upload::{
    compute_mip_level_count, compute_upload_plan, plan_mip_generation, repack_strided_rows,
    MipGenerationPlan, MipLevelExtent, StrideMode, UploadDecision, UploadPlan, UploadRegion,
};
pub use views::{validate_subrect, SubrectViewKey, ViewCache, ViewDecline};
