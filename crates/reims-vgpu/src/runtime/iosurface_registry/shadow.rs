//! Phase 1 Shadow Registry: pure shadow validation alongside the legacy pipeline.

use std::sync::Mutex;
use reims_vgpu_core::fasthash::FxHashMap;
use super::census;
use super::state::{transition, Action, PlaneEvent, PlaneState};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShadowKey {
    pub surface_id: u32,
    pub plane: u32,
}

pub struct ShadowRegistry {
    planes: FxHashMap<ShadowKey, PlaneState>,
}

impl Default for ShadowRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ShadowRegistry {
    pub fn new() -> Self {
        Self {
            planes: FxHashMap::default(),
        }
    }

    pub fn on_event(&mut self, surface_id: u32, plane: u32, event: PlaneEvent) -> Option<PlaneState> {
        let key = ShadowKey { surface_id, plane };
        let current = self.planes.get(&key).copied().unwrap_or(PlaneState::Created);
        match transition(current, event) {
            Ok((next, actions)) => {
                if actions.contains(Action::FreeImage) {
                    self.planes.remove(&key);
                } else {
                    self.planes.insert(key, next);
                }
                Some(next)
            }
            Err(slug) => {
                crate::runtime::drain::note_store_route(slug);
                None
            }
        }
    }

    pub fn query_state(&self, surface_id: u32, plane: u32) -> PlaneState {
        self.planes
            .get(&ShadowKey { surface_id, plane })
            .copied()
            .unwrap_or(PlaneState::Created)
    }

    /// Compare shadow decision against legacy pipeline behavior.
    ///
    /// If legacy saw modified bytes on a plane the shadow would have treated as GpuAuthoritative
    /// (skipping reseed), this is a critical disagreement that could cause rendering divergence.
    pub fn record_legacy_reseed(&mut self, surface_id: u32, plane: u32, legacy_modified_bytes: bool) {
        let key = ShadowKey { surface_id, plane };
        let shadow_state = self.planes.get(&key).copied().unwrap_or(PlaneState::Created);

        let shadow_would_skip = shadow_state == PlaneState::GpuAuthoritative;

        if shadow_would_skip && legacy_modified_bytes {
            census::note_shadow_disagree_modified();
        } else if !shadow_would_skip && !legacy_modified_bytes {
            census::note_shadow_disagree_unnecessary_upload();
        } else {
            census::note_shadow_agree();
        }
    }
}

static SHADOW_INSTANCE: Mutex<Option<ShadowRegistry>> = Mutex::new(None);

/// Global accessor for shadow registry event recording under `REIMS_VGPU_LAB_IOSURFACE_REGISTRY_SHADOW`.
pub fn with_shadow<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut ShadowRegistry) -> R,
{
    if crate::config::read(crate::config::LAB_IOSURFACE_REGISTRY_SHADOW).0 != crate::config::Switch::On {
        return None;
    }
    if let Ok(mut guard) = SHADOW_INSTANCE.lock() {
        let reg = guard.get_or_insert_with(ShadowRegistry::new);
        Some(f(reg))
    } else {
        None
    }
}

/// Record a shadow event if shadow registry is active.
pub fn note_shadow_event(surface_id: u32, plane: u32, event: PlaneEvent) {
    with_shadow(|reg| {
        reg.on_event(surface_id, plane, event);
    });
}

/// Record a legacy reseed comparison if shadow registry is active.
pub fn note_shadow_legacy_reseed(surface_id: u32, plane: u32, legacy_modified_bytes: bool) {
    with_shadow(|reg| {
        reg.record_legacy_reseed(surface_id, plane, legacy_modified_bytes);
    });
}
