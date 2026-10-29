//! Comprehensive unit tests for IOSurface registry and shadow tracking.

use super::shadow::ShadowRegistry;
use super::state::{transition, Action, PlaneEvent, PlaneState};

#[test]
fn test_exhaustive_state_event_matrix() {
    let states = [
        PlaneState::Created,
        PlaneState::GpuAuthoritative,
        PlaneState::CpuDirty,
        PlaneState::Syncing,
        PlaneState::Released,
    ];

    let events = [
        PlaneEvent::GpuWrite,
        PlaneEvent::CpuWriteEvidence { dirty_rect: false },
        PlaneEvent::CpuWriteEvidence { dirty_rect: true },
        PlaneEvent::GpuReadRequest,
        PlaneEvent::UploadCompleted,
        PlaneEvent::ReadbackCompleted,
        PlaneEvent::GuestDestroy,
        PlaneEvent::GpuDrained,
    ];

    for &s in &states {
        for &e in &events {
            let res = transition(s, e);
            match s {
                PlaneState::Released => {
                    if e == PlaneEvent::GpuDrained {
                        assert!(res.is_ok());
                        let (next, actions) = res.unwrap();
                        assert_eq!(next, PlaneState::Released);
                        assert!(actions.contains(Action::FreeImage));
                    } else {
                        // Any other event on released plane must fail with typed refusal
                        assert!(res.is_err());
                    }
                }
                PlaneState::Created => {
                    if e == PlaneEvent::ReadbackCompleted {
                        assert!(res.is_err());
                    } else {
                        assert!(res.is_ok());
                    }
                }
                PlaneState::CpuDirty => {
                    if e == PlaneEvent::ReadbackCompleted {
                        assert!(res.is_err());
                    } else {
                        assert!(res.is_ok());
                    }
                }
                _ => {
                    assert!(res.is_ok(), "Expected ok transition for {:?} x {:?}", s, e);
                }
            }
        }
    }
}

#[test]
fn test_shadow_registry_lifecycle_and_agreement() {
    let mut shadow = ShadowRegistry::new();
    let sid = 42;
    let plane = 0;

    assert_eq!(shadow.query_state(sid, plane), PlaneState::Created);

    // 1. GPU writes surface 42 plane 0
    let next = shadow.on_event(sid, plane, PlaneEvent::GpuWrite);
    assert_eq!(next, Some(PlaneState::GpuAuthoritative));
    assert_eq!(shadow.query_state(sid, plane), PlaneState::GpuAuthoritative);

    // 2. Sampling the plane: shadow predicts reseed is NOT needed.
    // If legacy reseed says modified=false, they agree!
    shadow.record_legacy_reseed(sid, plane, false);

    // 3. CPU writes to surface (seed bump)
    let next = shadow.on_event(
        sid,
        plane,
        PlaneEvent::CpuWriteEvidence { dirty_rect: false },
    );
    assert_eq!(next, Some(PlaneState::CpuDirty));
    assert_eq!(shadow.query_state(sid, plane), PlaneState::CpuDirty);

    // 4. GPU reads: shadow moves to Syncing
    let next = shadow.on_event(sid, plane, PlaneEvent::GpuReadRequest);
    assert_eq!(next, Some(PlaneState::Syncing));

    // 5. Upload completes: moves to GpuAuthoritative
    let next = shadow.on_event(sid, plane, PlaneEvent::UploadCompleted);
    assert_eq!(next, Some(PlaneState::GpuAuthoritative));

    // 6. Surface destroyed
    let next = shadow.on_event(sid, plane, PlaneEvent::GuestDestroy);
    assert_eq!(next, Some(PlaneState::Released));

    // 7. GPU drained
    let next = shadow.on_event(sid, plane, PlaneEvent::GpuDrained);
    assert_eq!(next, Some(PlaneState::Released));

    // After release + drain, entry is cleaned up
    assert_eq!(shadow.query_state(sid, plane), PlaneState::Created);
}

#[test]
fn test_shadow_narrow_dirty_tracking_with_host() {
    use crate::runtime::host::FakeHost;
    std::env::set_var("REIMS_VGPU_LAB_DIRTY_NARROW", "1");

    let mut shadow = ShadowRegistry::new();
    let mut host = FakeHost::new();
    let sid = 42;
    let plane = 0;
    let token = 0x1234;

    shadow.register_surface(sid, plane);
    shadow.register_token(sid, plane, token, true);

    // 1. GPU writes surface 42 plane 0 -> transitions to GpuAuthoritative
    let next = shadow.on_event_with_host(sid, plane, PlaneEvent::GpuWrite, Some(&mut host));
    assert_eq!(next, Some(PlaneState::GpuAuthoritative));

    // When GPU is authoritative, dirty tracking is disabled (narrowed)
    assert_eq!(host.narrow_tracked.borrow().get(&token), Some(&false));
    // Since wp_fallback was registered as true, wp_fallback was enabled on host
    assert_eq!(host.wp_fallback.borrow().get(&token), Some(&true));

    // 2. CPU write evidence arrives -> transitions to CpuDirty
    let next = shadow.on_event_with_host(
        sid,
        plane,
        PlaneEvent::CpuWriteEvidence { dirty_rect: false },
        Some(&mut host),
    );
    assert_eq!(next, Some(PlaneState::CpuDirty));

    // Now dirty tracking is re-enabled on host
    assert_eq!(host.narrow_tracked.borrow().get(&token), Some(&true));

    // 3. Destroy surface
    shadow.on_event_with_host(sid, plane, PlaneEvent::GuestDestroy, Some(&mut host));
    // 4. Drain GPU
    shadow.on_event_with_host(sid, plane, PlaneEvent::GpuDrained, Some(&mut host));

    // Token and plane cleaned up
    assert_eq!(shadow.get_token(sid, plane), None);
    std::env::remove_var("REIMS_VGPU_LAB_DIRTY_NARROW");
}

#[test]
fn test_budget_manager_integration_with_state_machine_and_rules() {
    use super::budget::{
        calculate_plane_bytes, EvictionAction, Residency, RestoreDecision, SurfaceKey,
        VramBudgetManager,
    };

    // 1024x1024 4bpp = 4 MiB plane
    let bytes = calculate_plane_bytes(1024, 1024, 4, 1, 1, 1, 1);
    assert_eq!(bytes, 4 * 1024 * 1024);

    let mut mgr = VramBudgetManager::new(6 * 1024 * 1024); // 6 MiB limit (fits 1 plane, but 2 will exceed)
    let k1 = SurfaceKey::new(100, 0);
    let k2 = SurfaceKey::new(200, 0);

    // Register k1 at frame 10 (idle at frame 100)
    mgr.register_surface(k1, 1, bytes, false, 10);
    mgr.note_gpu_use(k1, 5, 10); // GPU authoritative, submission 5

    // Register k2 at frame 20 (idle at frame 100)
    mgr.register_surface(k2, 1, bytes, false, 20);
    mgr.note_gpu_use(k2, 8, 20); // GPU authoritative, submission 8

    // Total 8 MiB > 6 MiB budget
    assert!(mgr.is_over_budget());

    // Plan evictions at frame 100 with GPU timeline completed at 10:
    // k1 is older (frame 10 vs 20), both are drained (5 <= 10, 8 <= 10).
    // Rule R6: GpuAuthoritative k1 requires ScheduleWritebackAndEvict.
    let actions = mgr.plan_evictions(100, 10);
    assert_eq!(
        actions,
        vec![EvictionAction::ScheduleWritebackAndEvict { key: k1, bytes }]
    );

    // Now total is 4 MiB, within 6 MiB budget!
    assert!(!mgr.is_over_budget());
    assert_eq!(mgr.total_vram_bytes(), 4 * 1024 * 1024);

    // Accessing k1 before writeback completed is refused under Rule R4 (no cold clear)
    assert_eq!(
        mgr.evaluate_access(k1, 101),
        Ok(Some(RestoreDecision::ColdClearRefused {
            key: k1,
            slug: "iosurface.cold_clear.refused",
        }))
    );

    // After writeback completed:
    mgr.note_writeback_completed(k1);

    // Accessing k1 restores from guest pages without cold clear (Rule R4)
    assert_eq!(
        mgr.evaluate_access(k1, 102),
        Ok(Some(RestoreDecision::RestoreFromGuestPages { key: k1 }))
    );
    assert_eq!(mgr.total_vram_bytes(), 8 * 1024 * 1024);
}
