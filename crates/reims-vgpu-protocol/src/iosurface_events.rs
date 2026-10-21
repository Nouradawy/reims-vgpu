//! Guest IOSurface lifecycle & synchronization event protocol (`CAP_IOSURFACE_EVENTS` and `REIMS_MSG_IOSURF_*`).
//!
//! Clean-room implementation based on public Apple IOSurface and Khronos Vulkan specifications.
//! `#![no_std]`.

use crate::endian::{ld32, ld64, st32, st64};

/// Capability bit in `DEVICE_INFO_KEY_SUPPORT_FLAGS_2025` (key 42) advertising
/// support for IOSurface lifecycle and synchronization messages (`0x41`..`0x47`).
pub const CAP_IOSURFACE_EVENTS: u32 = 1 << 3;

/// Maximum number of planes supported in an IOSurface create message.
pub const MAX_IOSURF_PLANES: usize = 4;

/// Lock option flags matching Apple IOSurface lock options.
pub const IOSURF_LOCK_READ_ONLY: u32 = 1 << 0;
pub const IOSURF_LOCK_AVOID_SYNC: u32 = 1 << 1;

/// Wire descriptor for a single IOSurface plane (32 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ReimsIosurfPlaneDesc {
    pub width: u32,
    pub height: u32,
    pub bytes_per_row: u32,
    pub bytes_per_element: u32,
    pub plane_offset: u64,
    pub plane_size: u64,
}

impl ReimsIosurfPlaneDesc {
    pub const LEN: usize = 32;

    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < Self::LEN {
            return None;
        }
        Some(Self {
            width: ld32(buf),
            height: ld32(&buf[4..]),
            bytes_per_row: ld32(&buf[8..]),
            bytes_per_element: ld32(&buf[12..]),
            plane_offset: ld64(&buf[16..]),
            plane_size: ld64(&buf[24..]),
        })
    }

    pub fn encode(&self, buf: &mut [u8]) -> bool {
        if buf.len() < Self::LEN {
            return false;
        }
        st32(buf, self.width);
        st32(&mut buf[4..], self.height);
        st32(&mut buf[8..], self.bytes_per_row);
        st32(&mut buf[12..], self.bytes_per_element);
        st64(&mut buf[16..], self.plane_offset);
        st64(&mut buf[24..], self.plane_size);
        true
    }
}

/// Message payload for `CHILD_OP_IOSURF_CREATE` (0x41).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReimsMsgIosurfCreate {
    pub surface_id: u32,
    pub pixel_format: u32,
    pub total_alloc_size: u64,
    pub plane_count: u32,
    pub seed: u32,
    pub planes: [ReimsIosurfPlaneDesc; MAX_IOSURF_PLANES],
}

impl ReimsMsgIosurfCreate {
    pub const HEADER_LEN: usize = 24;
    pub const TOTAL_LEN: usize = Self::HEADER_LEN + MAX_IOSURF_PLANES * ReimsIosurfPlaneDesc::LEN;

    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < Self::HEADER_LEN {
            return None;
        }
        let surface_id = ld32(buf);
        let pixel_format = ld32(&buf[4..]);
        let total_alloc_size = ld64(&buf[8..]);
        let plane_count = ld32(&buf[16..]);
        let seed = ld32(&buf[20..]);

        let mut planes = [ReimsIosurfPlaneDesc::default(); MAX_IOSURF_PLANES];
        let available_planes = (buf.len() - Self::HEADER_LEN) / ReimsIosurfPlaneDesc::LEN;
        let read_count = (plane_count as usize).min(MAX_IOSURF_PLANES).min(available_planes);

        for i in 0..read_count {
            let offset = Self::HEADER_LEN + i * ReimsIosurfPlaneDesc::LEN;
            if let Some(desc) = ReimsIosurfPlaneDesc::decode(&buf[offset..]) {
                planes[i] = desc;
            }
        }

        Some(Self {
            surface_id,
            pixel_format,
            total_alloc_size,
            plane_count,
            seed,
            planes,
        })
    }

    pub fn encode(&self, buf: &mut [u8]) -> bool {
        if buf.len() < Self::TOTAL_LEN {
            return false;
        }
        st32(buf, self.surface_id);
        st32(&mut buf[4..], self.pixel_format);
        st64(&mut buf[8..], self.total_alloc_size);
        st32(&mut buf[16..], self.plane_count);
        st32(&mut buf[20..], self.seed);
        for i in 0..MAX_IOSURF_PLANES {
            let offset = Self::HEADER_LEN + i * ReimsIosurfPlaneDesc::LEN;
            self.planes[i].encode(&mut buf[offset..offset + ReimsIosurfPlaneDesc::LEN]);
        }
        true
    }
}

/// Message payload for `CHILD_OP_IOSURF_DESTROY` (0x42).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ReimsMsgIosurfDestroy {
    pub surface_id: u32,
    pub reserved: u32,
}

impl ReimsMsgIosurfDestroy {
    pub const LEN: usize = 8;

    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < Self::LEN {
            return None;
        }
        Some(Self {
            surface_id: ld32(buf),
            reserved: ld32(&buf[4..]),
        })
    }

    pub fn encode(&self, buf: &mut [u8]) -> bool {
        if buf.len() < Self::LEN {
            return false;
        }
        st32(buf, self.surface_id);
        st32(&mut buf[4..], self.reserved);
        true
    }
}

/// Message payload for `CHILD_OP_IOSURF_USECOUNT` (0x43).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ReimsMsgIosurfUsecount {
    pub surface_id: u32,
    pub usecount: u32,
    pub delta: i32,
    pub reserved: u32,
}

impl ReimsMsgIosurfUsecount {
    pub const LEN: usize = 16;

    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < Self::LEN {
            return None;
        }
        Some(Self {
            surface_id: ld32(buf),
            usecount: ld32(&buf[4..]),
            delta: ld32(&buf[8..]) as i32,
            reserved: ld32(&buf[12..]),
        })
    }

    pub fn encode(&self, buf: &mut [u8]) -> bool {
        if buf.len() < Self::LEN {
            return false;
        }
        st32(buf, self.surface_id);
        st32(&mut buf[4..], self.usecount);
        st32(&mut buf[8..], self.delta as u32);
        st32(&mut buf[12..], self.reserved);
        true
    }
}

/// Message payload for `CHILD_OP_IOSURF_LOCK` (0x44).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ReimsMsgIosurfLock {
    pub surface_id: u32,
    pub options: u32,
    pub seed: u32,
    pub reserved: u32,
}

impl ReimsMsgIosurfLock {
    pub const LEN: usize = 16;

    pub fn is_read_only(&self) -> bool {
        (self.options & IOSURF_LOCK_READ_ONLY) != 0
    }

    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < Self::LEN {
            return None;
        }
        Some(Self {
            surface_id: ld32(buf),
            options: ld32(&buf[4..]),
            seed: ld32(&buf[8..]),
            reserved: ld32(&buf[12..]),
        })
    }

    pub fn encode(&self, buf: &mut [u8]) -> bool {
        if buf.len() < Self::LEN {
            return false;
        }
        st32(buf, self.surface_id);
        st32(&mut buf[4..], self.options);
        st32(&mut buf[8..], self.seed);
        st32(&mut buf[12..], self.reserved);
        true
    }
}

/// Message payload for `CHILD_OP_IOSURF_UNLOCK` (0x45).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ReimsMsgIosurfUnlock {
    pub surface_id: u32,
    pub options: u32,
    pub seed: u32,
    pub dirty_rect_valid: u32,
    pub dirty_x: u32,
    pub dirty_y: u32,
    pub dirty_width: u32,
    pub dirty_height: u32,
}

impl ReimsMsgIosurfUnlock {
    pub const LEN: usize = 32;

    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < Self::LEN {
            return None;
        }
        Some(Self {
            surface_id: ld32(buf),
            options: ld32(&buf[4..]),
            seed: ld32(&buf[8..]),
            dirty_rect_valid: ld32(&buf[12..]),
            dirty_x: ld32(&buf[16..]),
            dirty_y: ld32(&buf[20..]),
            dirty_width: ld32(&buf[24..]),
            dirty_height: ld32(&buf[28..]),
        })
    }

    pub fn encode(&self, buf: &mut [u8]) -> bool {
        if buf.len() < Self::LEN {
            return false;
        }
        st32(buf, self.surface_id);
        st32(&mut buf[4..], self.options);
        st32(&mut buf[8..], self.seed);
        st32(&mut buf[12..], self.dirty_rect_valid);
        st32(&mut buf[16..], self.dirty_x);
        st32(&mut buf[20..], self.dirty_y);
        st32(&mut buf[24..], self.dirty_width);
        st32(&mut buf[28..], self.dirty_height);
        true
    }
}

/// Message payload for `CHILD_OP_IOSURF_READBACK_REQ` (0x46).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ReimsMsgIosurfReadbackReq {
    pub surface_id: u32,
    pub plane: u32,
    pub sequence: u64,
}

impl ReimsMsgIosurfReadbackReq {
    pub const LEN: usize = 16;

    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < Self::LEN {
            return None;
        }
        Some(Self {
            surface_id: ld32(buf),
            plane: ld32(&buf[4..]),
            sequence: ld64(&buf[8..]),
        })
    }

    pub fn encode(&self, buf: &mut [u8]) -> bool {
        if buf.len() < Self::LEN {
            return false;
        }
        st32(buf, self.surface_id);
        st32(&mut buf[4..], self.plane);
        st64(&mut buf[8..], self.sequence);
        true
    }
}

/// Message payload for `CHILD_OP_IOSURF_READBACK_DONE` (0x47).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ReimsMsgIosurfReadbackDone {
    pub surface_id: u32,
    pub plane: u32,
    pub sequence: u64,
    pub status: u32,
    pub reserved: u32,
}

impl ReimsMsgIosurfReadbackDone {
    pub const LEN: usize = 24;

    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < Self::LEN {
            return None;
        }
        Some(Self {
            surface_id: ld32(buf),
            plane: ld32(&buf[4..]),
            sequence: ld64(&buf[8..]),
            status: ld32(&buf[16..]),
            reserved: ld32(&buf[20..]),
        })
    }

    pub fn encode(&self, buf: &mut [u8]) -> bool {
        if buf.len() < Self::LEN {
            return false;
        }
        st32(buf, self.surface_id);
        st32(&mut buf[4..], self.plane);
        st64(&mut buf[8..], self.sequence);
        st32(&mut buf[16..], self.status);
        st32(&mut buf[20..], self.reserved);
        true
    }
}

// Compile-time assertions verifying exact sizes and field offsets.
const _: () = {
    use core::mem::{offset_of, size_of};

    assert!(size_of::<ReimsIosurfPlaneDesc>() == 32);
    assert!(offset_of!(ReimsIosurfPlaneDesc, width) == 0);
    assert!(offset_of!(ReimsIosurfPlaneDesc, height) == 4);
    assert!(offset_of!(ReimsIosurfPlaneDesc, bytes_per_row) == 8);
    assert!(offset_of!(ReimsIosurfPlaneDesc, bytes_per_element) == 12);
    assert!(offset_of!(ReimsIosurfPlaneDesc, plane_offset) == 16);
    assert!(offset_of!(ReimsIosurfPlaneDesc, plane_size) == 24);

    assert!(size_of::<ReimsMsgIosurfCreate>() == 152);
    assert!(offset_of!(ReimsMsgIosurfCreate, surface_id) == 0);
    assert!(offset_of!(ReimsMsgIosurfCreate, pixel_format) == 4);
    assert!(offset_of!(ReimsMsgIosurfCreate, total_alloc_size) == 8);
    assert!(offset_of!(ReimsMsgIosurfCreate, plane_count) == 16);
    assert!(offset_of!(ReimsMsgIosurfCreate, seed) == 20);
    assert!(offset_of!(ReimsMsgIosurfCreate, planes) == 24);

    assert!(size_of::<ReimsMsgIosurfDestroy>() == 8);
    assert!(offset_of!(ReimsMsgIosurfDestroy, surface_id) == 0);
    assert!(offset_of!(ReimsMsgIosurfDestroy, reserved) == 4);

    assert!(size_of::<ReimsMsgIosurfUsecount>() == 16);
    assert!(offset_of!(ReimsMsgIosurfUsecount, surface_id) == 0);
    assert!(offset_of!(ReimsMsgIosurfUsecount, usecount) == 4);
    assert!(offset_of!(ReimsMsgIosurfUsecount, delta) == 8);
    assert!(offset_of!(ReimsMsgIosurfUsecount, reserved) == 12);

    assert!(size_of::<ReimsMsgIosurfLock>() == 16);
    assert!(offset_of!(ReimsMsgIosurfLock, surface_id) == 0);
    assert!(offset_of!(ReimsMsgIosurfLock, options) == 4);
    assert!(offset_of!(ReimsMsgIosurfLock, seed) == 8);
    assert!(offset_of!(ReimsMsgIosurfLock, reserved) == 12);

    assert!(size_of::<ReimsMsgIosurfUnlock>() == 32);
    assert!(offset_of!(ReimsMsgIosurfUnlock, surface_id) == 0);
    assert!(offset_of!(ReimsMsgIosurfUnlock, options) == 4);
    assert!(offset_of!(ReimsMsgIosurfUnlock, seed) == 8);
    assert!(offset_of!(ReimsMsgIosurfUnlock, dirty_rect_valid) == 12);
    assert!(offset_of!(ReimsMsgIosurfUnlock, dirty_x) == 16);
    assert!(offset_of!(ReimsMsgIosurfUnlock, dirty_y) == 20);
    assert!(offset_of!(ReimsMsgIosurfUnlock, dirty_width) == 24);
    assert!(offset_of!(ReimsMsgIosurfUnlock, dirty_height) == 28);

    assert!(size_of::<ReimsMsgIosurfReadbackReq>() == 16);
    assert!(offset_of!(ReimsMsgIosurfReadbackReq, surface_id) == 0);
    assert!(offset_of!(ReimsMsgIosurfReadbackReq, plane) == 4);
    assert!(offset_of!(ReimsMsgIosurfReadbackReq, sequence) == 8);

    assert!(size_of::<ReimsMsgIosurfReadbackDone>() == 24);
    assert!(offset_of!(ReimsMsgIosurfReadbackDone, surface_id) == 0);
    assert!(offset_of!(ReimsMsgIosurfReadbackDone, plane) == 4);
    assert!(offset_of!(ReimsMsgIosurfReadbackDone, sequence) == 8);
    assert!(offset_of!(ReimsMsgIosurfReadbackDone, status) == 16);
    assert!(offset_of!(ReimsMsgIosurfReadbackDone, reserved) == 20);
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plane_desc_roundtrip() {
        let desc = ReimsIosurfPlaneDesc {
            width: 1920,
            height: 1080,
            bytes_per_row: 7680,
            bytes_per_element: 4,
            plane_offset: 0x4000,
            plane_size: 1920 * 1080 * 4,
        };
        let mut buf = [0u8; ReimsIosurfPlaneDesc::LEN];
        assert!(desc.encode(&mut buf));
        let decoded = ReimsIosurfPlaneDesc::decode(&buf).expect("decoded");
        assert_eq!(desc, decoded);
    }

    #[test]
    fn iosurf_create_roundtrip() {
        let mut create = ReimsMsgIosurfCreate {
            surface_id: 101,
            pixel_format: 0x42475241, // 'BGRA'
            total_alloc_size: 8294400,
            plane_count: 1,
            seed: 42,
            planes: [ReimsIosurfPlaneDesc::default(); MAX_IOSURF_PLANES],
        };
        create.planes[0] = ReimsIosurfPlaneDesc {
            width: 1920,
            height: 1080,
            bytes_per_row: 7680,
            bytes_per_element: 4,
            plane_offset: 0,
            plane_size: 8294400,
        };
        let mut buf = [0u8; ReimsMsgIosurfCreate::TOTAL_LEN];
        assert!(create.encode(&mut buf));
        let decoded = ReimsMsgIosurfCreate::decode(&buf).expect("decoded");
        assert_eq!(create, decoded);
    }

    #[test]
    fn iosurf_destroy_roundtrip() {
        let msg = ReimsMsgIosurfDestroy {
            surface_id: 202,
            reserved: 0,
        };
        let mut buf = [0u8; ReimsMsgIosurfDestroy::LEN];
        assert!(msg.encode(&mut buf));
        let decoded = ReimsMsgIosurfDestroy::decode(&buf).expect("decoded");
        assert_eq!(msg, decoded);
    }

    #[test]
    fn iosurf_usecount_roundtrip() {
        let msg = ReimsMsgIosurfUsecount {
            surface_id: 303,
            usecount: 5,
            delta: 1,
            reserved: 0,
        };
        let mut buf = [0u8; ReimsMsgIosurfUsecount::LEN];
        assert!(msg.encode(&mut buf));
        let decoded = ReimsMsgIosurfUsecount::decode(&buf).expect("decoded");
        assert_eq!(msg, decoded);
    }

    #[test]
    fn iosurf_lock_unlock_roundtrip() {
        let lock = ReimsMsgIosurfLock {
            surface_id: 404,
            options: IOSURF_LOCK_READ_ONLY,
            seed: 12,
            reserved: 0,
        };
        assert!(lock.is_read_only());
        let mut lock_buf = [0u8; ReimsMsgIosurfLock::LEN];
        assert!(lock.encode(&mut lock_buf));
        let decoded_lock = ReimsMsgIosurfLock::decode(&lock_buf).expect("decoded");
        assert_eq!(lock, decoded_lock);

        let unlock = ReimsMsgIosurfUnlock {
            surface_id: 404,
            options: 0,
            seed: 13,
            dirty_rect_valid: 1,
            dirty_x: 10,
            dirty_y: 20,
            dirty_width: 100,
            dirty_height: 200,
        };
        let mut unlock_buf = [0u8; ReimsMsgIosurfUnlock::LEN];
        assert!(unlock.encode(&mut unlock_buf));
        let decoded_unlock = ReimsMsgIosurfUnlock::decode(&unlock_buf).expect("decoded");
        assert_eq!(unlock, decoded_unlock);
    }

    #[test]
    fn iosurf_readback_roundtrip() {
        let req = ReimsMsgIosurfReadbackReq {
            surface_id: 505,
            plane: 0,
            sequence: 0x1234_5678_9abc_def0,
        };
        let mut req_buf = [0u8; ReimsMsgIosurfReadbackReq::LEN];
        assert!(req.encode(&mut req_buf));
        let decoded_req = ReimsMsgIosurfReadbackReq::decode(&req_buf).expect("decoded");
        assert_eq!(req, decoded_req);

        let done = ReimsMsgIosurfReadbackDone {
            surface_id: 505,
            plane: 0,
            sequence: 0x1234_5678_9abc_def0,
            status: 0,
            reserved: 0,
        };
        let mut done_buf = [0u8; ReimsMsgIosurfReadbackDone::LEN];
        assert!(done.encode(&mut done_buf));
        let decoded_done = ReimsMsgIosurfReadbackDone::decode(&done_buf).expect("decoded");
        assert_eq!(done, decoded_done);
    }
}
