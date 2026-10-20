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
