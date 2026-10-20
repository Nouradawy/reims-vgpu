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
    let next = shadow.on_event(sid, plane, PlaneEvent::CpuWriteEvidence { dirty_rect: false });
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
