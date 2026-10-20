//! Pure per-plane state machine for guest IOSurfaces.
//!
//! Clean-room implementation based on public Apple IOSurface and Khronos Vulkan specifications.
//! Zero allocations; pure deterministic transitions with typed refusal slugs.

/// Lifecycle states of a single host image backing an IOSurface plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlaneState {
    /// Known to host, not yet written by the host GPU.
    /// Host image may be unallocated or unpopulated.
    Created,
    /// Last writer was the host GPU. The host `VkImage` is authoritative.
    /// Guest physical pages are stale unless a readback synced them.
    GpuAuthoritative,
    /// Guest CPU wrote since the last upload. Guest physical pages are authoritative;
    /// the host image is stale.
    CpuDirty,
    /// Upload (CPU -> GPU) or readback (GPU -> CPU) transfer is currently in flight.
    Syncing,
    /// Guest destroyed the surface. Waiting for in-flight GPU timeline to drain.
    Released,
}

/// Events that drive plane state transitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneEvent {
    /// Host GPU renders to or writes into the plane (color attachment or storage image).
    GpuWrite,
    /// Evidence of guest CPU writing to surface memory (seed change, CPU unlock(write), or WP fault).
    CpuWriteEvidence {
        /// Whether the guest specified a localized dirty sub-rectangle.
        dirty_rect: bool,
    },
    /// GPU needs to sample or read the surface as an input texture.
    GpuReadRequest,
    /// Staged CPU -> GPU upload completed and image is populated.
    UploadCompleted,
    /// GPU -> CPU readback completed into guest memory.
    ReadbackCompleted,
    /// Guest explicitly requested destruction of the surface.
    GuestDestroy,
    /// Timeline semaphore passed the last GPU submission touching this image.
    GpuDrained,
}

/// Actions commanded by a state transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Allocate persistent host image if not already allocated.
    AllocateImage,
    /// Clear image to zero (only permitted on Created -> GpuAuthoritative without prior seed).
    ClearToZero,
    /// Upload guest pages to host image (staging -> image).
    ScheduleUpload { dirty_rect: bool },
    /// Readback host image to guest pages (image -> staging -> guest).
    ScheduleReadback,
    /// Bind persistent image directly without re-materializing or reseeding from guest memory.
    DirectBind,
    /// Defer image freeing until the timeline semaphore passes last_gpu_use.
    DeferFree,
    /// Immediately reclaim and free host resources.
    FreeImage,
}

/// Fixed-capacity stack list of actions returned by a transition (zero heap allocations).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ActionList {
    items: [Option<Action>; 4],
    len: usize,
}

impl ActionList {
    pub const fn empty() -> Self {
        Self {
            items: [None, None, None, None],
            len: 0,
        }
    }

    pub fn push(&mut self, action: Action) {
        if self.len < self.items.len() {
            self.items[self.len] = Some(action);
            self.len += 1;
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn as_slice(&self) -> &[Option<Action>] {
        &self.items[..self.len]
    }

    pub fn contains(&self, action: Action) -> bool {
        for i in 0..self.len {
            if self.items[i] == Some(action) {
                return true;
            }
        }
        false
    }
}

impl IntoIterator for ActionList {
    type Item = Action;
    type IntoIter = ActionListIter;

    fn into_iter(self) -> Self::IntoIter {
        ActionListIter { list: self, index: 0 }
    }
}

pub struct ActionListIter {
    list: ActionList,
    index: usize,
}

impl Iterator for ActionListIter {
    type Item = Action;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.list.len {
            let item = self.list.items[self.index];
            self.index += 1;
            item
        } else {
            None
        }
    }
}

/// Pure transition function: computes next state and commanded actions.
///
/// On illegal transitions, returns an Err with a typed refusal slug.
pub fn transition(state: PlaneState, event: PlaneEvent) -> Result<(PlaneState, ActionList), &'static str> {
    let mut actions = ActionList::empty();

    match (state, event) {
        // --- Created ---
        (PlaneState::Created, PlaneEvent::GpuWrite) => {
            actions.push(Action::AllocateImage);
            Ok((PlaneState::GpuAuthoritative, actions))
        }
        (PlaneState::Created, PlaneEvent::CpuWriteEvidence { .. }) => {
            Ok((PlaneState::CpuDirty, actions))
        }
        (PlaneState::Created, PlaneEvent::GpuReadRequest) => {
            // First read before any GPU write: must upload from guest memory
            actions.push(Action::AllocateImage);
            actions.push(Action::ScheduleUpload { dirty_rect: false });
            Ok((PlaneState::Syncing, actions))
        }
        (PlaneState::Created, PlaneEvent::GuestDestroy) => {
            actions.push(Action::FreeImage);
            Ok((PlaneState::Released, actions))
        }
        (PlaneState::Created, PlaneEvent::UploadCompleted) => {
            // Initial upload into created plane finished
            actions.push(Action::DirectBind);
            Ok((PlaneState::GpuAuthoritative, actions))
        }
        (PlaneState::Created, PlaneEvent::ReadbackCompleted) => {
            Err("iosurface_state_illegal_readback_when_created")
        }
        (PlaneState::Created, PlaneEvent::GpuDrained) => {
            Ok((PlaneState::Created, actions))
        }

        // --- GpuAuthoritative ---
        (PlaneState::GpuAuthoritative, PlaneEvent::GpuWrite) => {
            // Successive GPU writes update in place; image remains authoritative
            Ok((PlaneState::GpuAuthoritative, actions))
        }
        (PlaneState::GpuAuthoritative, PlaneEvent::GpuReadRequest) => {
            // Sampling GPU-authoritative plane: BIND DIRECTLY WITHOUT RESEED
            actions.push(Action::DirectBind);
            Ok((PlaneState::GpuAuthoritative, actions))
        }
        (PlaneState::GpuAuthoritative, PlaneEvent::CpuWriteEvidence { .. }) => {
            // Guest CPU modified the backing pages
            Ok((PlaneState::CpuDirty, actions))
        }
        (PlaneState::GpuAuthoritative, PlaneEvent::GuestDestroy) => {
            actions.push(Action::DeferFree);
            Ok((PlaneState::Released, actions))
        }
        (PlaneState::GpuAuthoritative, PlaneEvent::UploadCompleted) => {
            // Redundant upload completion; remains authoritative
            Ok((PlaneState::GpuAuthoritative, actions))
        }
        (PlaneState::GpuAuthoritative, PlaneEvent::ReadbackCompleted) => {
            // Readback finished; GPU content has been mirrored to guest
            Ok((PlaneState::GpuAuthoritative, actions))
        }
        (PlaneState::GpuAuthoritative, PlaneEvent::GpuDrained) => {
            Ok((PlaneState::GpuAuthoritative, actions))
        }

        // --- CpuDirty ---
        (PlaneState::CpuDirty, PlaneEvent::GpuWrite) => {
            // GPU overwriting a CPU-dirty surface
            actions.push(Action::AllocateImage);
            Ok((PlaneState::GpuAuthoritative, actions))
        }
        (PlaneState::CpuDirty, PlaneEvent::GpuReadRequest) => {
            // GPU wants to sample CPU-dirty surface: schedule upload
            actions.push(Action::ScheduleUpload { dirty_rect: false });
            Ok((PlaneState::Syncing, actions))
        }
        (PlaneState::CpuDirty, PlaneEvent::CpuWriteEvidence { .. }) => {
            // Successive CPU writes stay in CpuDirty
            Ok((PlaneState::CpuDirty, actions))
        }
        (PlaneState::CpuDirty, PlaneEvent::GuestDestroy) => {
            actions.push(Action::FreeImage);
            Ok((PlaneState::Released, actions))
        }
        (PlaneState::CpuDirty, PlaneEvent::UploadCompleted) => {
            actions.push(Action::DirectBind);
            Ok((PlaneState::GpuAuthoritative, actions))
        }
        (PlaneState::CpuDirty, PlaneEvent::ReadbackCompleted) => {
            Err("iosurface_state_illegal_readback_when_cpudirty")
        }
        (PlaneState::CpuDirty, PlaneEvent::GpuDrained) => {
            Ok((PlaneState::CpuDirty, actions))
        }

        // --- Syncing ---
        (PlaneState::Syncing, PlaneEvent::UploadCompleted) => {
            // Upload finished: image is now current with guest memory
            actions.push(Action::DirectBind);
            Ok((PlaneState::GpuAuthoritative, actions))
        }
        (PlaneState::Syncing, PlaneEvent::ReadbackCompleted) => {
            // Readback finished: guest memory is now current with host image
            Ok((PlaneState::GpuAuthoritative, actions))
        }
        (PlaneState::Syncing, PlaneEvent::GpuWrite) => {
            // GPU write while syncing: moves to GpuAuthoritative upon submission
            Ok((PlaneState::GpuAuthoritative, actions))
        }
        (PlaneState::Syncing, PlaneEvent::GpuReadRequest) => {
            // Another read while syncing: still syncing
            Ok((PlaneState::Syncing, actions))
        }
        (PlaneState::Syncing, PlaneEvent::CpuWriteEvidence { .. }) => {
            // CPU dirtied again during transfer: will need re-upload
            Ok((PlaneState::CpuDirty, actions))
        }
        (PlaneState::Syncing, PlaneEvent::GuestDestroy) => {
            actions.push(Action::DeferFree);
            Ok((PlaneState::Released, actions))
        }
        (PlaneState::Syncing, PlaneEvent::GpuDrained) => {
            Ok((PlaneState::Syncing, actions))
        }

        // --- Released ---
        (PlaneState::Released, PlaneEvent::GpuDrained) => {
            actions.push(Action::FreeImage);
            Ok((PlaneState::Released, actions))
        }
        (PlaneState::Released, PlaneEvent::GpuWrite)
        | (PlaneState::Released, PlaneEvent::GpuReadRequest) => {
            Err("iosurface_released_write_after_release")
        }
        (PlaneState::Released, _) => {
            Err("iosurface_state_illegal_transition_on_released")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_to_gpu_authoritative_on_gpu_write() {
        let (next, actions) = transition(PlaneState::Created, PlaneEvent::GpuWrite).unwrap();
        assert_eq!(next, PlaneState::GpuAuthoritative);
        assert!(actions.contains(Action::AllocateImage));
    }

    #[test]
    fn created_to_cpu_dirty_on_cpu_evidence() {
        let (next, actions) = transition(
            PlaneState::Created,
            PlaneEvent::CpuWriteEvidence { dirty_rect: false },
        )
        .unwrap();
        assert_eq!(next, PlaneState::CpuDirty);
        assert!(actions.is_empty());
    }

    #[test]
    fn created_to_syncing_on_read_request() {
        let (next, actions) = transition(PlaneState::Created, PlaneEvent::GpuReadRequest).unwrap();
        assert_eq!(next, PlaneState::Syncing);
        assert!(actions.contains(Action::AllocateImage));
        assert!(actions.contains(Action::ScheduleUpload { dirty_rect: false }));
    }

    #[test]
    fn gpu_authoritative_direct_bind_on_read_request() {
        // Crucial test: sampling GpuAuthoritative must NOT schedule reseed/upload!
        let (next, actions) =
            transition(PlaneState::GpuAuthoritative, PlaneEvent::GpuReadRequest).unwrap();
        assert_eq!(next, PlaneState::GpuAuthoritative);
        assert_eq!(actions.len(), 1);
        assert!(actions.contains(Action::DirectBind));
        assert!(!actions.contains(Action::ScheduleUpload { dirty_rect: false }));
    }

    #[test]
    fn gpu_authoritative_successive_gpu_writes_remain_authoritative() {
        let (next, actions) =
            transition(PlaneState::GpuAuthoritative, PlaneEvent::GpuWrite).unwrap();
        assert_eq!(next, PlaneState::GpuAuthoritative);
        assert!(actions.is_empty());
    }

    #[test]
    fn gpu_authoritative_moves_to_cpudirty_on_cpu_evidence() {
        let (next, actions) = transition(
            PlaneState::GpuAuthoritative,
            PlaneEvent::CpuWriteEvidence { dirty_rect: true },
        )
        .unwrap();
        assert_eq!(next, PlaneState::CpuDirty);
        assert!(actions.is_empty());
    }

    #[test]
    fn cpu_dirty_schedules_upload_on_read() {
        let (next, actions) = transition(PlaneState::CpuDirty, PlaneEvent::GpuReadRequest).unwrap();
        assert_eq!(next, PlaneState::Syncing);
        assert!(actions.contains(Action::ScheduleUpload { dirty_rect: false }));
    }

    #[test]
    fn syncing_to_gpu_authoritative_on_upload_completed() {
        let (next, actions) =
            transition(PlaneState::Syncing, PlaneEvent::UploadCompleted).unwrap();
        assert_eq!(next, PlaneState::GpuAuthoritative);
        assert!(actions.contains(Action::DirectBind));
    }

    #[test]
    fn destroy_defers_free_when_in_flight() {
        let (next, actions) =
            transition(PlaneState::GpuAuthoritative, PlaneEvent::GuestDestroy).unwrap();
        assert_eq!(next, PlaneState::Released);
        assert!(actions.contains(Action::DeferFree));
    }

    #[test]
    fn write_after_release_refuses_with_typed_slug() {
        let res = transition(PlaneState::Released, PlaneEvent::GpuWrite);
        assert_eq!(res, Err("iosurface_released_write_after_release"));
    }

    #[test]
    fn read_after_release_refuses_with_typed_slug() {
        let res = transition(PlaneState::Released, PlaneEvent::GpuReadRequest);
        assert_eq!(res, Err("iosurface_released_write_after_release"));
    }

    #[test]
    fn drained_releases_and_frees_image() {
        let (next, actions) = transition(PlaneState::Released, PlaneEvent::GpuDrained).unwrap();
        assert_eq!(next, PlaneState::Released);
        assert!(actions.contains(Action::FreeImage));
    }
}
