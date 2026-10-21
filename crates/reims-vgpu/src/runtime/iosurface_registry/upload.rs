//! IOSurface upload planning and stride handling.
//!
//! Clean-room implementation based on public Apple IOSurface and Khronos Vulkan specifications.
//! Rule R2: CPU-written surfaces are uploaded once per seed change.
//! Computes bufferRowLength and dirty-rect copy bounds for VkBufferImageCopy.

use super::census;
use reims_vgpu_protocol::iosurface_events::ReimsIosurfPlaneDesc;

/// Bounding region within a plane to upload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UploadRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Vulkan row stride mode for buffer-to-image copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrideMode {
    /// Stride divides evenly by bytes_per_element and meets alignment requirements.
    /// In Vulkan, bufferRowLength is specified in texels.
    Direct { buffer_row_length: u32 },
    /// Stride cannot be expressed directly as bufferRowLength; repack required.
    RepackRequired,
}

/// The outcome of evaluating an upload request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UploadDecision {
    /// Skip upload because the current seed matches the already-uploaded seed.
    SkipSameSeed { seed: u32 },
    /// Execute an upload with the computed plan.
    Execute(UploadPlan),
}

/// Computed upload parameters for a plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UploadPlan {
    pub seed: u32,
    pub region: UploadRegion,
    pub stride_mode: StrideMode,
    pub is_partial: bool,
}

/// Computes the upload plan for a plane given its description, current seed,
/// last uploaded seed, and an optional dirty rectangle.
///
/// If `dirty_rect` is `Some((x, y, w, h))` and `w > 0 && h > 0`, the upload
/// region is clamped to the plane's width and height. If the clamped region
/// is smaller than the full plane, `is_partial` is true.
///
/// If `dirty_rect` is `None` or specifies `(0, 0, 0, 0)`, the full plane is uploaded.
pub fn compute_upload_plan(
    desc: &ReimsIosurfPlaneDesc,
    current_seed: u32,
    uploaded_seed: Option<u32>,
    dirty_rect: Option<(u32, u32, u32, u32)>,
) -> UploadDecision {
    if let Some(up_seed) = uploaded_seed {
        if up_seed == current_seed {
            census::note_upload_skipped_same_seed();
            return UploadDecision::SkipSameSeed { seed: current_seed };
        }
    }

    let plane_w = desc.width;
    let plane_h = desc.height;

    let (region, is_partial) = match dirty_rect {
        Some((dx, dy, dw, dh)) if dw > 0 && dh > 0 => {
            let clamped_x = dx.min(plane_w);
            let clamped_y = dy.min(plane_h);
            let clamped_w = dw.min(plane_w.saturating_sub(clamped_x));
            let clamped_h = dh.min(plane_h.saturating_sub(clamped_y));

            let is_full = clamped_x == 0
                && clamped_y == 0
                && clamped_w == plane_w
                && clamped_h == plane_h;

            (
                UploadRegion {
                    x: clamped_x,
                    y: clamped_y,
                    width: clamped_w,
                    height: clamped_h,
                },
                !is_full,
            )
        }
        _ => (
            UploadRegion {
                x: 0,
                y: 0,
                width: plane_w,
                height: plane_h,
            },
            false,
        ),
    };

    let stride_mode = if desc.bytes_per_element > 0
        && desc.bytes_per_row % desc.bytes_per_element == 0
        && (desc.bytes_per_row / desc.bytes_per_element) >= plane_w
    {
        StrideMode::Direct {
            buffer_row_length: desc.bytes_per_row / desc.bytes_per_element,
        }
    } else {
        census::note_upload_stride_repack();
        StrideMode::RepackRequired
    };

    UploadDecision::Execute(UploadPlan {
        seed: current_seed,
        region,
        stride_mode,
        is_partial,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_desc(w: u32, h: u32, bpr: u32, bpe: u32) -> ReimsIosurfPlaneDesc {
        ReimsIosurfPlaneDesc {
            width: w,
            height: h,
            bytes_per_row: bpr,
            bytes_per_element: bpe,
            plane_offset: 0,
            plane_size: (bpr * h) as u64,
        }
    }

    #[test]
    fn same_seed_skips_upload() {
        let desc = sample_desc(100, 100, 400, 4);
        let decision = compute_upload_plan(&desc, 42, Some(42), None);
        assert_eq!(decision, UploadDecision::SkipSameSeed { seed: 42 });
    }

    #[test]
    fn new_seed_schedules_upload() {
        let desc = sample_desc(100, 100, 400, 4);
        let decision = compute_upload_plan(&desc, 43, Some(42), None);
        match decision {
            UploadDecision::Execute(plan) => {
                assert_eq!(plan.seed, 43);
                assert_eq!(plan.region.x, 0);
                assert_eq!(plan.region.y, 0);
                assert_eq!(plan.region.width, 100);
                assert_eq!(plan.region.height, 100);
                assert!(!plan.is_partial);
                assert_eq!(plan.stride_mode, StrideMode::Direct { buffer_row_length: 100 });
            }
            _ => panic!("expected execute"),
        }
    }

    #[test]
    fn no_prior_seed_schedules_upload() {
        let desc = sample_desc(64, 64, 256, 4);
        let decision = compute_upload_plan(&desc, 1, None, None);
        match decision {
            UploadDecision::Execute(plan) => {
                assert_eq!(plan.seed, 1);
                assert!(!plan.is_partial);
            }
            _ => panic!("expected execute"),
        }
    }

    #[test]
    fn dirty_rect_clipped_to_bounds() {
        let desc = sample_desc(100, 100, 400, 4);
        // Dirty rect partially outside plane bounds: (80, 80, 50, 50)
        let decision = compute_upload_plan(&desc, 10, None, Some((80, 80, 50, 50)));
        match decision {
            UploadDecision::Execute(plan) => {
                assert_eq!(plan.region.x, 80);
                assert_eq!(plan.region.y, 80);
                assert_eq!(plan.region.width, 20); // 100 - 80
                assert_eq!(plan.region.height, 20); // 100 - 80
                assert!(plan.is_partial);
            }
            _ => panic!("expected execute"),
        }
    }

    #[test]
    fn dirty_rect_zero_means_full_plane() {
        let desc = sample_desc(100, 100, 400, 4);
        let decision = compute_upload_plan(&desc, 10, None, Some((0, 0, 0, 0)));
        match decision {
            UploadDecision::Execute(plan) => {
                assert_eq!(plan.region.x, 0);
                assert_eq!(plan.region.y, 0);
                assert_eq!(plan.region.width, 100);
                assert_eq!(plan.region.height, 100);
                assert!(!plan.is_partial);
            }
            _ => panic!("expected execute"),
        }
    }

    #[test]
    fn stride_calculation_direct() {
        // Padded stride: width 100, bytes_per_element 4 -> 400 bytes, but bytes_per_row is 512
        let desc = sample_desc(100, 100, 512, 4);
        let decision = compute_upload_plan(&desc, 1, None, None);
        match decision {
            UploadDecision::Execute(plan) => {
                assert_eq!(plan.stride_mode, StrideMode::Direct { buffer_row_length: 128 });
            }
            _ => panic!("expected execute"),
        }
    }

    #[test]
    fn stride_calculation_repack_not_divisible() {
        // Stride 510 bytes with 4 bytes per element does not divide evenly
        let desc = sample_desc(100, 100, 510, 4);
        let decision = compute_upload_plan(&desc, 1, None, None);
        match decision {
            UploadDecision::Execute(plan) => {
                assert_eq!(plan.stride_mode, StrideMode::RepackRequired);
            }
            _ => panic!("expected execute"),
        }
    }

    #[test]
    fn stride_calculation_repack_row_length_less_than_width() {
        // Stride smaller than width * bytes_per_element
        let desc = sample_desc(100, 100, 200, 4);
        let decision = compute_upload_plan(&desc, 1, None, None);
        match decision {
            UploadDecision::Execute(plan) => {
                assert_eq!(plan.stride_mode, StrideMode::RepackRequired);
            }
            _ => panic!("expected execute"),
        }
    }
}
