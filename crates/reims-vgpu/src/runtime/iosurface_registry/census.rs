//! IOSurface registry census counters and telemetry slugs.

use crate::runtime::drain::note_store_route;

/// Note that a shadow registry reseed decision agreed with the legacy pipeline.
pub fn note_shadow_agree() {
    note_store_route("iosurf_shadow_agree");
}

/// Note that legacy detected modified bytes, but the shadow registry would have skipped reseed!
/// In Phase 1, this disagreement count MUST be zero before Phase 2 can be enabled.
pub fn note_shadow_disagree_modified() {
    note_store_route("iosurf_shadow_disagree_modified");
}

/// Note that legacy skipped or saw identical bytes, but the shadow registry would have uploaded.
pub fn note_shadow_disagree_unnecessary_upload() {
    note_store_route("iosurf_shadow_disagree_unnecessary_upload");
}

/// Note a persistent GPU image resolution without guest page copy.
pub fn note_persistent_gpu_hit() {
    note_store_route("iosurf_persistent_gpu_hit");
}

/// Note that a persistent GPU image required guest fallback.
pub fn note_persistent_gpu_fallback() {
    note_store_route("iosurf_persistent_gpu_fallback");
}

/// Note that a CPU upload was skipped because the current seed matches uploaded_seed.
pub fn note_upload_skipped_same_seed() {
    note_store_route("iosurf_upload_skipped_same_seed");
}

/// Note that an upload stride requires repacking because it is not cleanly divisible or aligned.
pub fn note_upload_stride_repack() {
    note_store_route("iosurf_upload_stride_repack");
}

/// Note an explicit readback was requested by the guest driver.
pub fn note_readback_requested() {
    note_store_route("iosurf_readback_requested");
}

/// Note an explicit readback completed and signaled the guest.
pub fn note_readback_done() {
    note_store_route("iosurf_readback_done");
}

/// Note that an explicit readback was avoided due to the AVOID_SYNC option.
pub fn note_readback_avoid_sync() {
    note_store_route("iosurf_readback_avoid_sync");
}

/// Note that a compute dispatch was memoized and execution skipped.
pub fn note_memo_skipped() {
    note_store_route("iosurf_compute_memo_skipped");
}

/// Note that a compute dispatch could not be memoized due to side effects (buffers/atomics).
pub fn note_memo_side_effects() {
    note_store_route("iosurf_compute_memo_side_effects");
}

/// Note that coalesced and legacy dirty results disagreed during debug compare mode.
pub fn note_dirty_batch_mismatch() {
    note_store_route("dirty_batch_mismatch");
}

/// Note that QEMU host_ops lacks range registration support.
pub fn note_dirty_narrow_unavailable() {
    note_store_route("dirty_narrow_unavailable");
}

/// Note that narrow write-protect caught a CPU write to a fallback range.
pub fn note_iosurface_wp_fault() {
    note_store_route("iosurface_wp_fault");
}

