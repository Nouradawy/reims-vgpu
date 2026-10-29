//! Phase 1 Shadow Registry & Phase 4 Dirty Tracking Narrowing:
//! shadow validation alongside the legacy pipeline, and selective dirty-query narrowing
//! for GPU-authoritative IOSurfaces.

use super::census;
use super::state::{transition, Action, PlaneEvent, PlaneState};
use crate::runtime::host::HostOps;
use reims_vgpu_core::fasthash::FxHashMap;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShadowKey {
    pub surface_id: u32,
    pub plane: u32,
}

pub struct ShadowRegistry {
    planes: FxHashMap<ShadowKey, PlaneState>,
    tokens: FxHashMap<ShadowKey, u64>,
    wp_fallbacks: FxHashMap<ShadowKey, bool>,
}

/// Whether a registry transition must leave the guest-write witness armed.
///
/// A Phase-5 resident is a host-side copy.  `GpuAuthoritative` says only that
/// the last device Store wrote it; it cannot make the shared IOSurface backing
/// immutable to guest CPU stores.
fn keep_dirty_tracking(next: PlaneState, persistent_gpu_on: bool) -> bool {
    next != PlaneState::GpuAuthoritative || persistent_gpu_on
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
            tokens: FxHashMap::default(),
            wp_fallbacks: FxHashMap::default(),
        }
    }

    pub fn register_surface(&mut self, surface_id: u32, plane: u32) {
        self.planes
            .insert(ShadowKey { surface_id, plane }, PlaneState::Created);
    }

    pub fn register_token(&mut self, surface_id: u32, plane: u32, token: u64, wp_fallback: bool) {
        let key = ShadowKey { surface_id, plane };
        self.tokens.insert(key, token);
        if wp_fallback {
            self.wp_fallbacks.insert(key, true);
        }
    }

    pub fn get_token(&self, surface_id: u32, plane: u32) -> Option<u64> {
        self.tokens.get(&ShadowKey { surface_id, plane }).copied()
    }

    pub fn is_wp_fallback(&self, surface_id: u32, plane: u32) -> bool {
        self.wp_fallbacks
            .get(&ShadowKey { surface_id, plane })
            .copied()
            .unwrap_or(false)
    }

    pub fn on_event(
        &mut self,
        surface_id: u32,
        plane: u32,
        event: PlaneEvent,
    ) -> Option<PlaneState> {
        self.on_event_with_host(surface_id, plane, event, None)
    }

    pub fn on_event_with_host(
        &mut self,
        surface_id: u32,
        plane: u32,
        event: PlaneEvent,
        mut host: Option<&mut dyn HostOps>,
    ) -> Option<PlaneState> {
        let key = ShadowKey { surface_id, plane };
        let current = self
            .planes
            .get(&key)
            .copied()
            .unwrap_or(PlaneState::Created);
        match transition(current, event) {
            Ok((next, actions)) => {
                let token = self.tokens.get(&key).copied().unwrap_or(0);
                let is_wp = self.wp_fallbacks.get(&key).copied().unwrap_or(false);

                if let Some(h) = host.as_deref_mut() {
                    if token != 0 {
                        let narrow_on = crate::config::read(crate::config::LAB_DIRTY_NARROW).0
                            == crate::config::Switch::On;
                        if narrow_on {
                            match next {
                                PlaneState::GpuAuthoritative => {
                                    // GPU authority describes the last device Store, not
                                    // exclusive ownership of the guest pages.  Those pages
                                    // are still legal CPU write targets (notably the
                                    // SpringBoard settled-icon cache), and a resident may
                                    // only be reused while the guest-write witness remains
                                    // armed.  Disabling it here made a later CPU repaint
                                    // invisible and let an old persistent image win.
                                    let persistent_gpu_on = crate::config::read(
                                        crate::config::LAB_IOSURFACE_PERSISTENT_GPU,
                                    )
                                    .0 == crate::config::Switch::On;
                                    // The narrowing experiment still applies to
                                    // non-persistent rails.  A Phase-5 resident,
                                    // however, is precisely a host-side copy
                                    // that needs this witness before reuse.
                                    h.set_dirty_tracking(
                                        token,
                                        keep_dirty_tracking(next, persistent_gpu_on),
                                    );
                                    if is_wp {
                                        h.set_wp_fallback(token, true);
                                    }
                                }
                                PlaneState::CpuDirty
                                | PlaneState::Created
                                | PlaneState::Syncing => {
                                    // Guest CPU may write or upload needed; ensure active tracking.
                                    h.set_dirty_tracking(token, true);
                                    if is_wp {
                                        h.set_wp_fallback(token, true);
                                    }
                                }
                                PlaneState::Released => {
                                    h.set_dirty_tracking(token, false);
                                    h.set_wp_fallback(token, false);
                                }
                            }
                        }
                    }
                }

                if actions.contains(Action::FreeImage) {
                    self.planes.remove(&key);
                    self.tokens.remove(&key);
                    self.wp_fallbacks.remove(&key);
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
    pub fn record_legacy_reseed(
        &mut self,
        surface_id: u32,
        plane: u32,
        legacy_modified_bytes: bool,
    ) {
        let key = ShadowKey { surface_id, plane };
        let shadow_state = self
            .planes
            .get(&key)
            .copied()
            .unwrap_or(PlaneState::Created);

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
    if crate::config::read(crate::config::LAB_IOSURFACE_REGISTRY_SHADOW).0
        != crate::config::Switch::On
    {
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

/// Record a shadow event with optional host ops notification if shadow registry is active.
pub fn note_shadow_event_with_host(
    surface_id: u32,
    plane: u32,
    event: PlaneEvent,
    host: Option<&mut dyn HostOps>,
) {
    with_shadow(|reg| {
        reg.on_event_with_host(surface_id, plane, event, host);
    });
}

/// Associate a dirty tracking token and WP fallback flag with a surface plane in shadow registry.
pub fn note_shadow_token(
    surface_id: u32,
    plane: u32,
    token: u64,
    wp_fallback: bool,
    mut host: Option<&mut dyn HostOps>,
) {
    with_shadow(|reg| {
        reg.register_token(surface_id, plane, token, wp_fallback);
        if let Some(h) = host.as_deref_mut() {
            let narrow_on =
                crate::config::read(crate::config::LAB_DIRTY_NARROW).0 == crate::config::Switch::On;
            if narrow_on && token != 0 {
                if wp_fallback {
                    h.set_wp_fallback(token, true);
                }
                let st = reg.query_state(surface_id, plane);
                // See the GpuAuthoritative branch above: the registry state
                // does not make its guest backing immutable.  A Phase-5
                // persistent copy keeps its token armed from registration so
                // every persistent-copy consumer can apply its currency gate.
                let persistent_gpu_on =
                    crate::config::read(crate::config::LAB_IOSURFACE_PERSISTENT_GPU).0
                        == crate::config::Switch::On;
                h.set_dirty_tracking(token, keep_dirty_tracking(st, persistent_gpu_on));
            }
        }
    });
}

/// Record a surface creation in shadow registry if active.
pub fn note_shadow_create(surface_id: u32, plane: u32) {
    with_shadow(|reg| {
        reg.register_surface(surface_id, plane);
    });
}

/// Record a legacy reseed comparison if shadow registry is active.
pub fn note_shadow_legacy_reseed(surface_id: u32, plane: u32, legacy_modified_bytes: bool) {
    with_shadow(|reg| {
        reg.record_legacy_reseed(surface_id, plane, legacy_modified_bytes);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistent_gpu_authority_keeps_its_guest_write_witness_armed() {
        assert!(keep_dirty_tracking(PlaneState::GpuAuthoritative, true));
        assert!(!keep_dirty_tracking(PlaneState::GpuAuthoritative, false));
        for state in [
            PlaneState::Created,
            PlaneState::CpuDirty,
            PlaneState::Syncing,
            PlaneState::Released,
        ] {
            assert!(keep_dirty_tracking(state, false));
            assert!(keep_dirty_tracking(state, true));
        }
    }
}
