//! IOSurface explicit readback handshake and ticket tracking.
//!
//! Clean-room implementation based on public Apple IOSurface specifications.
//! Rule R3: Guest CPU reads of GPU-authoritative surfaces require an explicit
//! readback (image -> staging -> guest pages) before lock completion.

use super::census;
use super::state::PlaneState;
use reims_vgpu_protocol::iosurface_events::{
    ReimsMsgIosurfReadbackDone, IOSURF_LOCK_AVOID_SYNC, IOSURF_LOCK_READ_ONLY,
};

/// Readback execution requirement for a lock or readback request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadbackAction {
    /// No readback is necessary (guest pages are authoritative, or AVOID_SYNC is set).
    /// Returns completion response with status 0.
    ImmediateDone(ReimsMsgIosurfReadbackDone),
    /// Explicit readback must be queued and executed before completion response.
    ScheduleReadback {
        surface_id: u32,
        plane: u32,
        sequence: u64,
    },
    /// No action required (e.g. lock with AVOID_SYNC and no sequence handshake).
    None,
}

/// Evaluates a guest lock message against a plane's current state.
pub fn evaluate_lock_sync(
    surface_id: u32,
    plane: u32,
    plane_state: PlaneState,
    lock_options: u32,
    sequence: u64,
) -> ReadbackAction {
    // If AVOID_SYNC is requested, bypass readback completely.
    if (lock_options & IOSURF_LOCK_AVOID_SYNC) != 0 {
        census::note_readback_avoid_sync();
        if sequence != 0 {
            return ReadbackAction::ImmediateDone(ReimsMsgIosurfReadbackDone {
                surface_id,
                plane,
                sequence,
                status: 0,
                reserved: 0,
            });
        }
        return ReadbackAction::None;
    }

    // A read-only lock or read-write lock requires sync if host GPU is authoritative.
    let needs_sync = (lock_options & IOSURF_LOCK_READ_ONLY) != 0 || true; // Non-avoid-sync lock needs sync
    if needs_sync && plane_state == PlaneState::GpuAuthoritative {
        census::note_readback_requested();
        ReadbackAction::ScheduleReadback {
            surface_id,
            plane,
            sequence,
        }
    } else {
        // Plane is Created, CpuDirty, Syncing, or Released -> guest pages are already authoritative.
        if sequence != 0 {
            census::note_readback_done();
            ReadbackAction::ImmediateDone(ReimsMsgIosurfReadbackDone {
                surface_id,
                plane,
                sequence,
                status: 0,
                reserved: 0,
            })
        } else {
            ReadbackAction::None
        }
    }
}

/// Evaluates an explicit readback request packet (`CHILD_OP_IOSURF_READBACK_REQ`).
pub fn evaluate_readback_req(
    surface_id: u32,
    plane: u32,
    plane_state: PlaneState,
    sequence: u64,
) -> ReadbackAction {
    census::note_readback_requested();
    if plane_state == PlaneState::GpuAuthoritative {
        ReadbackAction::ScheduleReadback {
            surface_id,
            plane,
            sequence,
        }
    } else {
        census::note_readback_done();
        ReadbackAction::ImmediateDone(ReimsMsgIosurfReadbackDone {
            surface_id,
            plane,
            sequence,
            status: 0,
            reserved: 0,
        })
    }
}

/// Creates a successful readback done packet after transfer completion.
pub fn complete_readback(surface_id: u32, plane: u32, sequence: u64) -> ReimsMsgIosurfReadbackDone {
    census::note_readback_done();
    ReimsMsgIosurfReadbackDone {
        surface_id,
        plane,
        sequence,
        status: 0,
        reserved: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_authoritative_triggers_readback() {
        let action = evaluate_lock_sync(
            100,
            0,
            PlaneState::GpuAuthoritative,
            IOSURF_LOCK_READ_ONLY,
            12345,
        );
        assert_eq!(
            action,
            ReadbackAction::ScheduleReadback {
                surface_id: 100,
                plane: 0,
                sequence: 12345,
            }
        );
    }

    #[test]
    fn avoid_sync_bypasses_readback() {
        let action = evaluate_lock_sync(
            100,
            0,
            PlaneState::GpuAuthoritative,
            IOSURF_LOCK_AVOID_SYNC,
            12345,
        );
        assert_eq!(
            action,
            ReadbackAction::ImmediateDone(ReimsMsgIosurfReadbackDone {
                surface_id: 100,
                plane: 0,
                sequence: 12345,
                status: 0,
                reserved: 0,
            })
        );
    }

    #[test]
    fn cpu_dirty_or_created_completes_immediately() {
        let action_created = evaluate_lock_sync(100, 0, PlaneState::Created, 0, 999);
        assert_eq!(
            action_created,
            ReadbackAction::ImmediateDone(ReimsMsgIosurfReadbackDone {
                surface_id: 100,
                plane: 0,
                sequence: 999,
                status: 0,
                reserved: 0,
            })
        );

        let action_dirty = evaluate_lock_sync(100, 0, PlaneState::CpuDirty, 0, 1000);
        assert_eq!(
            action_dirty,
            ReadbackAction::ImmediateDone(ReimsMsgIosurfReadbackDone {
                surface_id: 100,
                plane: 0,
                sequence: 1000,
                status: 0,
                reserved: 0,
            })
        );
    }

    #[test]
    fn readback_req_gpu_authoritative_schedules() {
        let action = evaluate_readback_req(200, 1, PlaneState::GpuAuthoritative, 555);
        assert_eq!(
            action,
            ReadbackAction::ScheduleReadback {
                surface_id: 200,
                plane: 1,
                sequence: 555,
            }
        );
    }

    #[test]
    fn readback_req_non_gpu_authoritative_completes_immediately() {
        let action = evaluate_readback_req(200, 1, PlaneState::CpuDirty, 555);
        assert_eq!(
            action,
            ReadbackAction::ImmediateDone(ReimsMsgIosurfReadbackDone {
                surface_id: 200,
                plane: 1,
                sequence: 555,
                status: 0,
                reserved: 0,
            })
        );
    }
}
