//! TEMP (lab, Nour 030755cd port): the three default-off switches that gate
//! video format gating, surface shutdown guards and frontbuffer swap latching.
//! See `reims-vgpu-config` (`LAB_VIDEO_GATE`, `LAB_RELEASE_GUARDS`,
//! `LAB_FRONTBUFFER_LATCH`).

use std::sync::OnceLock;

fn read_once(cell: &'static OnceLock<bool>, name: &'static str, slug: &'static str) -> bool {
    *cell.get_or_init(|| {
        let on = matches!(crate::config::read(name).0, crate::config::Switch::On);
        if on {
            crate::observe::fail(format!("{slug} ({name}=on)"));
        }
        on
    })
}

/// `REIMS_VGPU_LAB_VIDEO_GATE`, read once (default off).
pub(crate) fn video_gate() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    read_once(&ON, crate::config::LAB_VIDEO_GATE, "lab_video_gate_on")
}

/// `REIMS_VGPU_LAB_RELEASE_GUARDS`, read once (default off).
pub(crate) fn release_guards() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    read_once(
        &ON,
        crate::config::LAB_RELEASE_GUARDS,
        "lab_release_guards_on",
    )
}

/// `REIMS_VGPU_LAB_FRONTBUFFER_LATCH`, read once (default off).
pub(crate) fn frontbuffer_latch() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    read_once(
        &ON,
        crate::config::LAB_FRONTBUFFER_LATCH,
        "lab_frontbuffer_latch_on",
    )
}
