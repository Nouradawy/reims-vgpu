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

/// Note that a view was declined with a specific typed reason.
pub fn note_view_declined(reason: &str) {
    if reason == "geometry" {
        note_store_route("iosurface_view_declined_geometry");
    } else if reason == "stride" {
        note_store_route("iosurface_view_declined_stride");
    } else if reason == "format" {
        note_store_route("iosurface_view_declined_format");
    } else {
        note_store_route("iosurface_view_declined_other");
    }
}

/// Note that a sub-rect alias could not be represented directly.
pub fn note_view_subrect_unsupported() {
    note_store_route("iosurface_view_subrect_unsupported");
}

/// Note that a sub-rect view was admitted under LAB_IOSURFACE_SUBRECT_VIEWS.
pub fn note_subrect_view_admitted() {
    note_store_route("iosurface_subrect_view_admitted");
}

/// Note that a format was substituted with an emulation or conversion fallback.
pub fn note_format_fallback(from: &str, _to: &str) {
    if from == "bgra10_xr" {
        note_store_route("format_fallback_bgra10_xr_rgba16f");
    } else if from == "astc_4x4_ldr" || from == "ASTC_4x4_LDR" {
        note_store_route("format_fallback_astc_4x4_ldr_rgba8");
    } else if from == "etc2_rgb8" || from == "ETC2_RGB8" {
        note_store_route("format_fallback_etc2_rgb8_rgba8");
    } else {
        note_store_route("format_fallback_general");
    }
}

/// Note that a format had no native mapping or fallback and was declined.
pub fn note_format_unsupported(name: &str) {
    if name == "astc_4x4_ldr" || name == "ASTC_4x4_LDR" {
        note_store_route("format_unsupported_astc_4x4_ldr");
    } else if name == "etc2_rgb8" || name == "ETC2_RGB8" {
        note_store_route("format_unsupported_etc2_rgb8");
    } else {
        note_store_route("format_unsupported_general");
    }
}

/// Note that mip generation was unsupported for the format.
pub fn note_mipgen_unsupported() {
    note_store_route("format_mipgen_unsupported");
}

/// Note that an entry was evicted from the persistent registry without writeback (e.g. CpuDirty).
pub fn note_evict() {
    note_store_route("iosurface_evict");
}

/// Note that a GpuAuthoritative entry required asynchronous writeback before eviction (Rule R6).
pub fn note_evict_writeback() {
    note_store_route("iosurface_evict_writeback");
}

/// Note that an evicted entry was restored on next use by uploading from guest pages (Rule R4).
pub fn note_restore() {
    note_store_route("iosurface_restore");
}

/// Note that the registry exceeded its VRAM budget and could not evict enough non-in-flight entries.
pub fn note_budget_exceeded() {
    note_store_route("iosurface_budget_exceeded");
}

/// Note that a cold clear was refused for an entry with prior content (Rule R4).
pub fn note_cold_clear_refused() {
    note_store_route("iosurface_cold_clear_refused");
}
