//! IOSurface VRAM budget accounting, LRU eviction, and backing restore.
//!
//! Clean-room implementation based on public Apple IOSurface and Khronos Vulkan specifications.
//! Implements Phase 6 of the persistent host IOSurface plan:
//! - Rule R6: Evicting a GpuAuthoritative image requires asynchronous write-back to guest memory first.
//! - Rule R4: Never cold-clear an image with prior content; restore from guest pages on reuse.
//! - Memory budget accounting (VK_EXT_memory_budget alignment and configurable ceiling).
//! - LRU eviction with in-flight timeline guards and per-frame write-back byte caps.

use super::census;
use super::state::PlaneState;
use reims_vgpu_config::{self, Count, Switch};
use std::collections::BTreeMap;

/// Default VRAM budget cap: 1536 MiB (1.5 GiB) on the target NVIDIA RTX 3080.
pub const DEFAULT_VRAM_BUDGET_BYTES: u64 = 1536 * 1024 * 1024;

/// Default idle frames before an entry becomes eligible for LRU eviction (K = 60 frames).
pub const DEFAULT_UNUSED_FRAME_THRESHOLD: u64 = 60;

/// Default per-frame write-back budget cap (64 MiB/frame) to prevent GPU/bus stalls.
pub const DEFAULT_MAX_WRITEBACK_BYTES_PER_FRAME: u64 = 64 * 1024 * 1024;

/// Unique key for an IOSurface plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SurfaceKey {
    pub surface_id: u32,
    pub plane: u32,
}

impl SurfaceKey {
    pub const fn new(surface_id: u32, plane: u32) -> Self {
        Self { surface_id, plane }
    }
}

/// Host residency status of an IOSurface plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Residency {
    /// Host `VkImage` is allocated and resident in VRAM.
    Resident,
    /// Evicted from VRAM due to memory budget pressure.
    /// `written_back` indicates whether GPU-authoritative pixels were mirrored to guest memory.
    Evicted { written_back: bool },
    /// Memory is directly imported from guest RAM via zero-copy.
    GuestImported,
    /// Pending allocation or asynchronous migration.
    Pending,
}

/// Guest purgeability status (Apple IOSurface purgeability hints).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purgeability {
    /// Non-volatile: content cannot be discarded without writeback.
    NonVolatile,
    /// Volatile: guest marked content as purgeable cache; can be dropped without writeback.
    Volatile,
    /// Empty: guest marked content as emptied/purged.
    Empty,
}

/// Accounting entry for an IOSurface plane in the VRAM budget manager.
#[derive(Clone, Debug)]
pub struct BudgetEntry {
    pub key: SurfaceKey,
    pub incarnation: u64,
    pub bytes: u64,
    pub last_used_frame: u64,
    pub last_gpu_use: u64,
    pub state: PlaneState,
    pub residency: Residency,
    pub purgeability: Purgeability,
    pub is_pinned: bool,
}

/// Eviction action decided for a plane candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvictionAction {
    /// Drop host image immediately with no writeback required
    /// (guest memory is authoritative, e.g. CpuDirty, or plane is Volatile).
    DropHostImageFree { key: SurfaceKey, bytes: u64 },
    /// Schedule asynchronous writeback to guest pages before freeing host image (Rule R6).
    ScheduleWritebackAndEvict { key: SurfaceKey, bytes: u64 },
    /// Entry must be retained (in-flight on GPU timeline, pinned, or quota filled).
    Retain,
}

/// Restore decision when an evicted plane is accessed again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreDecision {
    /// Restore host image by uploading from guest backing pages (Rule R4).
    RestoreFromGuestPages { key: SurfaceKey },
    /// Cold clear refused because neither written-back copy nor guest pages are current.
    ColdClearRefused { key: SurfaceKey, slug: &'static str },
}

/// Computes the VRAM footprint in bytes for a surface plane.
pub fn calculate_plane_bytes(
    width: u32,
    height: u32,
    bytes_per_block: u32,
    block_width: u32,
    block_height: u32,
    mip_levels: u32,
    array_layers: u32,
) -> u64 {
    let bw = block_width.max(1);
    let bh = block_height.max(1);
    let bpb = (bytes_per_block as u64).max(1);
    let layers = (array_layers as u64).max(1);
    let mips = mip_levels.max(1);

    let mut total_bytes: u64 = 0;
    let mut w = width.max(1);
    let mut h = height.max(1);

    for _ in 0..mips {
        let blocks_x = ((w + bw - 1) / bw) as u64;
        let blocks_y = ((h + bh - 1) / bh) as u64;
        let level_bytes = blocks_x * blocks_y * bpb;
        total_bytes += level_bytes;

        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }

    total_bytes * layers
}

/// VRAM budget and LRU eviction manager.
pub struct VramBudgetManager {
    budget_bytes: u64,
    current_bytes: u64,
    unused_frame_threshold: u64,
    max_writeback_per_frame: u64,
    entries: BTreeMap<SurfaceKey, BudgetEntry>,
}

impl VramBudgetManager {
    /// Creates a new budget manager with a specific byte limit.
    pub fn new(budget_bytes: u64) -> Self {
        Self {
            budget_bytes,
            current_bytes: 0,
            unused_frame_threshold: DEFAULT_UNUSED_FRAME_THRESHOLD,
            max_writeback_per_frame: DEFAULT_MAX_WRITEBACK_BYTES_PER_FRAME,
            entries: BTreeMap::new(),
        }
    }

    /// Creates a budget manager initialized from environment configuration.
    ///
    /// Narrows default cap (1536 MiB) if `REIMS_VGPU_LAB_IOSURFACE_VRAM_BUDGET_MB` is set.
    pub fn with_config() -> Self {
        let ceiling_mb = (DEFAULT_VRAM_BUDGET_BYTES / (1024 * 1024)) as u64;
        let budget_mb = match reims_vgpu_config::count(
            reims_vgpu_config::LAB_IOSURFACE_VRAM_BUDGET_MB,
            ceiling_mb,
        ) {
            Count::Narrowed(mb) => mb,
            _ => ceiling_mb,
        };

        Self::new(budget_mb * 1024 * 1024)
    }

    /// Checks if the budget and eviction system is enabled by environment switch.
    pub fn is_budget_enabled() -> bool {
        reims_vgpu_config::switch(reims_vgpu_config::LAB_IOSURFACE_BUDGET) == Switch::On
    }

    /// Sets the idle frame threshold (K frames) before an entry is eligible for eviction.
    pub fn set_unused_frame_threshold(&mut self, threshold: u64) {
        self.unused_frame_threshold = threshold;
    }

    /// Sets the maximum asynchronous writeback bytes permitted in a single frame.
    pub fn set_max_writeback_per_frame(&mut self, bytes: u64) {
        self.max_writeback_per_frame = bytes;
    }

    /// Total resident VRAM bytes tracked by the manager.
    pub fn total_vram_bytes(&self) -> u64 {
        self.current_bytes
    }

    /// Configured VRAM budget ceiling in bytes.
    pub fn budget_limit_bytes(&self) -> u64 {
        self.budget_bytes
    }

    /// Returns true if resident VRAM exceeds the configured budget.
    pub fn is_over_budget(&self) -> bool {
        self.current_bytes > self.budget_bytes
    }

    /// Registers a newly allocated surface plane into the budget manager.
    pub fn register_surface(
        &mut self,
        key: SurfaceKey,
        incarnation: u64,
        bytes: u64,
        purgeable: bool,
        frame: u64,
    ) {
        let purgeability = if purgeable {
            Purgeability::Volatile
        } else {
            Purgeability::NonVolatile
        };

        if let Some(old) = self.entries.insert(
            key,
            BudgetEntry {
                key,
                incarnation,
                bytes,
                last_used_frame: frame,
                last_gpu_use: 0,
                state: PlaneState::Created,
                residency: Residency::Resident,
                purgeability,
                is_pinned: false,
            },
        ) {
            if old.residency == Residency::Resident {
                self.current_bytes = self.current_bytes.saturating_sub(old.bytes);
            }
        }

        self.current_bytes += bytes;
    }

    /// Unregisters and reclaims accounting for a destroyed surface plane.
    pub fn unregister_surface(&mut self, key: SurfaceKey) -> Option<BudgetEntry> {
        let entry = self.entries.remove(&key);
        if let Some(ref e) = entry {
            if e.residency == Residency::Resident {
                self.current_bytes = self.current_bytes.saturating_sub(e.bytes);
            }
        }
        entry
    }

    /// Notes GPU use and submission timeline of a plane.
    pub fn note_gpu_use(&mut self, key: SurfaceKey, submission: u64, frame: u64) {
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.last_gpu_use = entry.last_gpu_use.max(submission);
            entry.last_used_frame = entry.last_used_frame.max(frame);
            entry.state = PlaneState::GpuAuthoritative;
            if entry.residency == Residency::Resident {
                // Plane is GPU authoritative, guest memory is now stale
                entry.residency = Residency::Resident;
            }
        }
    }

    /// Notes CPU write evidence (seed bump or unlock) to a plane.
    pub fn note_cpu_write(&mut self, key: SurfaceKey, frame: u64) {
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.last_used_frame = entry.last_used_frame.max(frame);
            entry.state = PlaneState::CpuDirty;
        }
    }

    /// Notes completion of asynchronous writeback to guest pages for an evicted entry.
    pub fn note_writeback_completed(&mut self, key: SurfaceKey) {
        if let Some(entry) = self.entries.get_mut(&key) {
            if let Residency::Evicted {
                ref mut written_back,
            } = entry.residency
            {
                *written_back = true;
            }
        }
    }

    /// Sets pinned status for an entry (e.g. scanout / active swapchain).
    pub fn set_pinned(&mut self, key: SurfaceKey, pinned: bool) {
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.is_pinned = pinned;
        }
    }

    /// Sets purgeability status for a surface.
    pub fn set_purgeability(&mut self, key: SurfaceKey, purgeability: Purgeability) {
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.purgeability = purgeability;
        }
    }

    /// Evaluates access to a plane upon draw/compute binding.
    ///
    /// If the plane is resident, marks it as used this frame.
    /// If evicted, determines the appropriate restore action under Rule R4.
    pub fn evaluate_access(
        &mut self,
        key: SurfaceKey,
        frame: u64,
    ) -> Result<Option<RestoreDecision>, &'static str> {
        let Some(entry) = self.entries.get_mut(&key) else {
            return Err("surface not found in budget manager");
        };

        entry.last_used_frame = entry.last_used_frame.max(frame);

        match entry.residency {
            Residency::Resident | Residency::GuestImported | Residency::Pending => Ok(None),
            Residency::Evicted { written_back } => {
                if written_back
                    || entry.state == PlaneState::CpuDirty
                    || entry.state == PlaneState::Created
                {
                    census::note_restore();
                    // Restore from guest pages into host image
                    entry.residency = Residency::Resident;
                    self.current_bytes += entry.bytes;
                    Ok(Some(RestoreDecision::RestoreFromGuestPages { key }))
                } else {
                    // Stale guest pages and no writeback completed: cold clear refused!
                    census::note_cold_clear_refused();
                    Ok(Some(RestoreDecision::ColdClearRefused {
                        key,
                        slug: "iosurface.cold_clear.refused",
                    }))
                }
            }
        }
    }

    /// Plans evictions to bring resident VRAM usage within budget.
    ///
    /// Follows strict priority order:
    /// 1. In-flight entries (`last_gpu_use > completed_timeline`) are NEVER evicted.
    /// 2. Pinned entries are NEVER evicted.
    /// 3. Volatile / empty purgeable entries are evicted first with immediate drop.
    /// 4. `CpuDirty` or `Created` entries are evicted with immediate drop (guest pages authoritative).
    /// 5. Least recently used `GpuAuthoritative` entries:
    ///    - If already clean/mirrored, drop freely.
    ///    - If stale, schedule asynchronous writeback (Rule R6), capped at `max_writeback_per_frame`.
    pub fn plan_evictions(
        &mut self,
        current_frame: u64,
        completed_timeline: u64,
    ) -> Vec<EvictionAction> {
        if !self.is_over_budget() {
            return Vec::new();
        }

        let mut actions = Vec::new();
        let mut writeback_bytes_scheduled: u64 = 0;

        // Collect candidates that are idle and not in flight on GPU timeline
        let mut candidate_keys: Vec<SurfaceKey> = self
            .entries
            .iter()
            .filter_map(|(&k, e)| {
                if e.residency != Residency::Resident || e.is_pinned {
                    return None;
                }
                // Guard: timeline must have passed last GPU use
                if e.last_gpu_use > completed_timeline {
                    return None;
                }
                // Guard: must have been unused for threshold frames
                if current_frame < e.last_used_frame + self.unused_frame_threshold {
                    return None;
                }
                Some(k)
            })
            .collect();

        // Sort candidates by eviction priority:
        // Priority 0: Volatile purgeable entries (oldest first)
        // Priority 1: CpuDirty / Created entries (oldest first)
        // Priority 2: GpuAuthoritative entries (oldest first)
        candidate_keys.sort_by_key(|&k| {
            let e = &self.entries[&k];
            let category_rank = match (e.purgeability, e.state) {
                (Purgeability::Volatile | Purgeability::Empty, _) => 0,
                (_, PlaneState::CpuDirty | PlaneState::Created) => 1,
                (_, PlaneState::GpuAuthoritative | PlaneState::Syncing | PlaneState::Released) => 2,
            };
            (category_rank, e.last_used_frame)
        });

        for key in candidate_keys {
            if !self.is_over_budget() {
                break;
            }

            let entry = self.entries.get_mut(&key).unwrap();
            let bytes = entry.bytes;

            match (entry.purgeability, entry.state) {
                (Purgeability::Volatile | Purgeability::Empty, _) => {
                    // Purgeable volatile: drop immediately without writeback
                    entry.residency = Residency::Evicted {
                        written_back: false,
                    };
                    self.current_bytes = self.current_bytes.saturating_sub(bytes);
                    census::note_evict();
                    actions.push(EvictionAction::DropHostImageFree { key, bytes });
                }
                (_, PlaneState::CpuDirty | PlaneState::Created) => {
                    // Guest pages are authoritative: drop host image freely
                    entry.residency = Residency::Evicted { written_back: true };
                    self.current_bytes = self.current_bytes.saturating_sub(bytes);
                    census::note_evict();
                    actions.push(EvictionAction::DropHostImageFree { key, bytes });
                }
                (_, PlaneState::GpuAuthoritative | PlaneState::Syncing | PlaneState::Released) => {
                    // Host GPU is authoritative: Rule R6 requires writeback before drop!
                    if writeback_bytes_scheduled + bytes <= self.max_writeback_per_frame {
                        writeback_bytes_scheduled += bytes;
                        entry.residency = Residency::Evicted {
                            written_back: false,
                        };
                        self.current_bytes = self.current_bytes.saturating_sub(bytes);
                        census::note_evict_writeback();
                        actions.push(EvictionAction::ScheduleWritebackAndEvict { key, bytes });
                    } else {
                        // Exceeded per-frame writeback quota; retain this frame to amortize bus traffic
                        actions.push(EvictionAction::Retain);
                    }
                }
            }
        }

        if self.is_over_budget() {
            // Still over budget after all eligible candidates considered
            census::note_budget_exceeded();
        }

        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculate_plane_bytes_standard_and_mips() {
        // 1024x1024 RGBA8 (4 bytes/pixel), 1 mip, 1 layer = 4 MiB
        let b = calculate_plane_bytes(1024, 1024, 4, 1, 1, 1, 1);
        assert_eq!(b, 4 * 1024 * 1024);

        // 64x64 RGBA8 with full mip chain (7 levels: 64, 32, 16, 8, 4, 2, 1)
        // 4096 + 1024 + 256 + 64 + 16 + 4 + 1 = 5461 pixels * 4 = 21844 bytes
        let b_mips = calculate_plane_bytes(64, 64, 4, 1, 1, 7, 1);
        assert_eq!(b_mips, 21844);

        // 2DArray 4 layers
        let b_array = calculate_plane_bytes(256, 256, 4, 1, 1, 1, 4);
        assert_eq!(b_array, 256 * 256 * 4 * 4);
    }

    #[test]
    fn budget_manager_registration_and_accounting() {
        let mut mgr = VramBudgetManager::new(10 * 1024 * 1024); // 10 MiB limit
        let k1 = SurfaceKey::new(1, 0);
        let k2 = SurfaceKey::new(2, 0);

        mgr.register_surface(k1, 1, 4 * 1024 * 1024, false, 1);
        assert_eq!(mgr.total_vram_bytes(), 4 * 1024 * 1024);
        assert!(!mgr.is_over_budget());

        mgr.register_surface(k2, 1, 8 * 1024 * 1024, false, 1);
        assert_eq!(mgr.total_vram_bytes(), 12 * 1024 * 1024);
        assert!(mgr.is_over_budget());

        mgr.unregister_surface(k1);
        assert_eq!(mgr.total_vram_bytes(), 8 * 1024 * 1024);
        assert!(!mgr.is_over_budget());
    }

    #[test]
    fn in_flight_entries_are_never_evicted() {
        let mut mgr = VramBudgetManager::new(4 * 1024 * 1024); // 4 MiB limit
        let k1 = SurfaceKey::new(1, 0);
        let k2 = SurfaceKey::new(2, 0);

        // Register two 4 MiB surfaces at frame 1
        mgr.register_surface(k1, 1, 4 * 1024 * 1024, false, 1);
        mgr.register_surface(k2, 1, 4 * 1024 * 1024, false, 1);
        assert!(mgr.is_over_budget());

        // k1 was used in GPU submission 100, completed timeline is only 50
        mgr.note_gpu_use(k1, 100, 1);
        // k2 was used in GPU submission 40, completed timeline is 50 (drained)
        mgr.note_gpu_use(k2, 40, 1);

        // At frame 100 (idle > 60 frames):
        let actions = mgr.plan_evictions(100, 50);

        // k1 MUST NOT be evicted because submission 100 > completed 50!
        assert!(!actions.contains(&EvictionAction::DropHostImageFree {
            key: k1,
            bytes: 4 * 1024 * 1024,
        }));
        assert!(
            !actions.contains(&EvictionAction::ScheduleWritebackAndEvict {
                key: k1,
                bytes: 4 * 1024 * 1024,
            })
        );

        // k2 is drained (40 <= 50) and can be scheduled for writeback & eviction
        assert!(
            actions.contains(&EvictionAction::ScheduleWritebackAndEvict {
                key: k2,
                bytes: 4 * 1024 * 1024,
            })
        );
    }

    #[test]
    fn cpu_dirty_entries_drop_freely_without_writeback() {
        let mut mgr = VramBudgetManager::new(2 * 1024 * 1024);
        let k1 = SurfaceKey::new(1, 0);

        mgr.register_surface(k1, 1, 4 * 1024 * 1024, false, 1);
        mgr.note_cpu_write(k1, 1); // Marked CpuDirty

        let actions = mgr.plan_evictions(100, 0);
        assert_eq!(
            actions,
            vec![EvictionAction::DropHostImageFree {
                key: k1,
                bytes: 4 * 1024 * 1024,
            }]
        );
        assert_eq!(mgr.total_vram_bytes(), 0);
    }

    #[test]
    fn gpu_authoritative_entries_require_writeback_under_rule_r6() {
        let mut mgr = VramBudgetManager::new(2 * 1024 * 1024);
        let k1 = SurfaceKey::new(1, 0);

        mgr.register_surface(k1, 1, 4 * 1024 * 1024, false, 1);
        mgr.note_gpu_use(k1, 10, 1); // Marked GpuAuthoritative

        let actions = mgr.plan_evictions(100, 20);
        assert_eq!(
            actions,
            vec![EvictionAction::ScheduleWritebackAndEvict {
                key: k1,
                bytes: 4 * 1024 * 1024,
            }]
        );

        // Entry is now Evicted { written_back: false } until writeback completes
        assert_eq!(
            mgr.entries[&k1].residency,
            Residency::Evicted {
                written_back: false
            }
        );

        // Before writeback completes, reuse triggers cold clear refusal
        let access = mgr.evaluate_access(k1, 101);
        assert_eq!(
            access,
            Ok(Some(RestoreDecision::ColdClearRefused {
                key: k1,
                slug: "iosurface.cold_clear.refused",
            }))
        );

        // After writeback completes, reuse restores from guest pages under Rule R4
        mgr.note_writeback_completed(k1);
        assert_eq!(
            mgr.entries[&k1].residency,
            Residency::Evicted { written_back: true }
        );

        let access_after = mgr.evaluate_access(k1, 102);
        assert_eq!(
            access_after,
            Ok(Some(RestoreDecision::RestoreFromGuestPages { key: k1 }))
        );
        assert_eq!(mgr.entries[&k1].residency, Residency::Resident);
    }

    #[test]
    fn per_frame_writeback_quota_is_respected() {
        let mut mgr = VramBudgetManager::new(1 * 1024 * 1024);
        mgr.set_max_writeback_per_frame(4 * 1024 * 1024); // 4 MiB max writeback / frame

        let k1 = SurfaceKey::new(1, 0);
        let k2 = SurfaceKey::new(2, 0);

        // Two 4 MiB GpuAuthoritative surfaces
        mgr.register_surface(k1, 1, 4 * 1024 * 1024, false, 1);
        mgr.note_gpu_use(k1, 10, 1);
        mgr.register_surface(k2, 1, 4 * 1024 * 1024, false, 1);
        mgr.note_gpu_use(k2, 10, 2);

        let actions = mgr.plan_evictions(100, 20);

        // First surface gets writeback, second is retained because quota was filled
        assert_eq!(
            actions,
            vec![
                EvictionAction::ScheduleWritebackAndEvict {
                    key: k1,
                    bytes: 4 * 1024 * 1024,
                },
                EvictionAction::Retain,
            ]
        );
    }

    #[test]
    fn purgeable_entries_evicted_first_without_writeback() {
        let mut mgr = VramBudgetManager::new(5 * 1024 * 1024); // 5 MiB limit (8 MiB total -> 4 MiB after k_vol)
        let k_vol = SurfaceKey::new(1, 0);
        let k_gpu = SurfaceKey::new(2, 0);

        // k_vol is volatile purgeable, k_gpu is standard non-volatile GPU authoritative
        mgr.register_surface(k_vol, 1, 4 * 1024 * 1024, true, 10);
        mgr.note_gpu_use(k_vol, 5, 10);

        mgr.register_surface(k_gpu, 1, 4 * 1024 * 1024, false, 5); // Older frame!
        mgr.note_gpu_use(k_gpu, 5, 5);

        let actions = mgr.plan_evictions(100, 20);

        // Even though k_gpu was used at frame 5 and k_vol at frame 10, k_vol is evicted FIRST
        // and dropped without writeback because it is volatile!
        assert_eq!(
            actions,
            vec![EvictionAction::DropHostImageFree {
                key: k_vol,
                bytes: 4 * 1024 * 1024,
            }]
        );
    }
}
