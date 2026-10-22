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

/// Repack rows from a strided source buffer into a tightly packed destination buffer.
///
/// Used as a fallback when `desc.bytes_per_row` is not a multiple of `bytes_per_element`
/// or when host alignment constraints require tightly packed rows before `VkBufferImageCopy`.
pub fn repack_strided_rows(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    height: usize,
    src_stride: usize,
    bpe: usize,
) -> Result<(), &'static str> {
    if width == 0 || height == 0 || bpe == 0 {
        return Ok(());
    }
    let tight_row_bytes = width.checked_mul(bpe).ok_or("repack: tight_row_bytes overflow")?;
    let required_dst = tight_row_bytes.checked_mul(height).ok_or("repack: dst overflow")?;
    if dst.len() < required_dst {
        return Err("repack: dst buffer too small");
    }
    for row in 0..height {
        let src_off = row.checked_mul(src_stride).ok_or("repack: src_off overflow")?;
        let src_end = src_off.checked_add(tight_row_bytes).ok_or("repack: src_end overflow")?;
        if src.len() < src_end {
            return Err("repack: src buffer too small for row");
        }
        let dst_off = row.checked_mul(tight_row_bytes).ok_or("repack: dst_off overflow")?;
        dst[dst_off..dst_off + tight_row_bytes].copy_from_slice(&src[src_off..src_end]);
    }
    Ok(())
}

/// Extent and index of an individual mipmap level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MipLevelExtent {
    pub level: u32,
    pub width: u32,
    pub height: u32,
}

/// Plan describing the sequence of downsample blits required for mip generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MipGenerationPlan {
    pub base_width: u32,
    pub base_height: u32,
    pub level_count: u32,
    pub levels: Vec<MipLevelExtent>,
}

/// Computes the maximum valid mip levels for a given 2D extent, clamped by requested levels.
pub fn compute_mip_level_count(width: u32, height: u32, requested_levels: u32) -> u32 {
    if width == 0 || height == 0 || requested_levels == 0 {
        return 1;
    }
    let max_dim = width.max(height);
    // 32 - leading_zeros gives floor(log2(max_dim)) + 1
    let full_chain_levels = 32 - max_dim.leading_zeros();
    requested_levels.min(full_chain_levels).max(1)
}

/// Plans mip generation under `REIMS_VGPU_LAB_MIPGEN` for a CPU-uploaded sampled image.
///
/// Returns `Some(MipGenerationPlan)` with N levels if enabled, requested_levels > 1,
/// and the format supports blit filtering.
/// Returns `None` if disabled or single-level.
/// Emits `format_mipgen_unsupported` if requested but format cannot be blitted.
pub fn plan_mip_generation(
    width: u32,
    height: u32,
    requested_levels: u32,
    format_supports_blit: bool,
    enabled_by_switch: bool,
) -> Option<MipGenerationPlan> {
    if !enabled_by_switch || requested_levels <= 1 || width == 0 || height == 0 {
        return None;
    }

    if !format_supports_blit {
        census::note_mipgen_unsupported();
        return None;
    }

    let level_count = compute_mip_level_count(width, height, requested_levels);
    if level_count <= 1 {
        return None;
    }

    let mut levels = Vec::with_capacity(level_count as usize);
    for lvl in 0..level_count {
        let w = (width >> lvl).max(1);
        let h = (height >> lvl).max(1);
        levels.push(MipLevelExtent {
            level: lvl,
            width: w,
            height: h,
        });
    }

    Some(MipGenerationPlan {
        base_width: width,
        base_height: height,
        level_count,
        levels,
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
    fn stride_calculation_tight() {
        // Tight stride: width 64, bpe 4 -> bytes_per_row is exactly 256
        let desc = sample_desc(64, 64, 256, 4);
        let decision = compute_upload_plan(&desc, 1, None, None);
        match decision {
            UploadDecision::Execute(plan) => {
                assert_eq!(plan.stride_mode, StrideMode::Direct { buffer_row_length: 64 });
            }
            _ => panic!("expected execute"),
        }
    }

    #[test]
    fn stride_calculation_direct_padded_64_and_256() {
        // Padded to 64: width 50, bpe 4 (tight 200), bpr padded to 256 (divisible by 4 and 64)
        let desc_64 = sample_desc(50, 50, 256, 4);
        let decision = compute_upload_plan(&desc_64, 1, None, None);
        match decision {
            UploadDecision::Execute(plan) => {
                assert_eq!(plan.stride_mode, StrideMode::Direct { buffer_row_length: 64 });
            }
            _ => panic!("expected execute"),
        }

        // Padded to 256: width 100, bpe 4 (tight 400), bpr padded to 512 (divisible by 4 and 256)
        let desc_256 = sample_desc(100, 100, 512, 4);
        let decision = compute_upload_plan(&desc_256, 1, None, None);
        match decision {
            UploadDecision::Execute(plan) => {
                assert_eq!(plan.stride_mode, StrideMode::Direct { buffer_row_length: 128 });
            }
            _ => panic!("expected execute"),
        }
    }

    #[test]
    fn stride_calculation_repack_not_divisible() {
        // Stride 510 bytes with 4 bytes per element does not divide evenly -> repack
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

    #[test]
    fn repack_strided_rows_round_trip() {
        // Source: 3 rows of 4 texels (16 bytes) with 24-byte stride (8 bytes padding per row)
        let mut src = vec![0u8; 24 * 3];
        // Row 0
        src[0..16].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
        // Row 1
        src[24..40].copy_from_slice(&[21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36]);
        // Row 2
        src[48..64].copy_from_slice(&[41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56]);

        let mut dst = vec![0u8; 16 * 3];
        assert_eq!(repack_strided_rows(&src, &mut dst, 4, 3, 24, 4), Ok(()));

        assert_eq!(&dst[0..16], &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
        assert_eq!(&dst[16..32], &[21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36]);
        assert_eq!(&dst[32..48], &[41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56]);
    }

    #[test]
    fn mip_generation_planning_64x64() {
        let plan = plan_mip_generation(64, 64, 10, true, true)
            .expect("should produce plan when enabled with blit support");

        assert_eq!(plan.level_count, 7);
        assert_eq!(plan.levels[0], MipLevelExtent { level: 0, width: 64, height: 64 });
        assert_eq!(plan.levels[1], MipLevelExtent { level: 1, width: 32, height: 32 });
        assert_eq!(plan.levels[2], MipLevelExtent { level: 2, width: 16, height: 16 });
        assert_eq!(plan.levels[3], MipLevelExtent { level: 3, width: 8, height: 8 });
        assert_eq!(plan.levels[4], MipLevelExtent { level: 4, width: 4, height: 4 });
        assert_eq!(plan.levels[5], MipLevelExtent { level: 5, width: 2, height: 2 });
        assert_eq!(plan.levels[6], MipLevelExtent { level: 6, width: 1, height: 1 });
    }

    #[test]
    fn mip_generation_planning_non_square() {
        let plan = plan_mip_generation(64, 16, 4, true, true)
            .expect("should produce 4 levels");

        assert_eq!(plan.level_count, 4);
        assert_eq!(plan.levels[0], MipLevelExtent { level: 0, width: 64, height: 16 });
        assert_eq!(plan.levels[1], MipLevelExtent { level: 1, width: 32, height: 8 });
        assert_eq!(plan.levels[2], MipLevelExtent { level: 2, width: 16, height: 4 });
        assert_eq!(plan.levels[3], MipLevelExtent { level: 3, width: 8, height: 2 });
    }

    #[test]
    fn mip_generation_disabled_or_unsupported() {
        // Disabled by switch
        assert_eq!(plan_mip_generation(64, 64, 4, true, false), None);

        // Single level requested
        assert_eq!(plan_mip_generation(64, 64, 1, true, true), None);

        // Blit not supported emits slug and returns None
        assert_eq!(plan_mip_generation(64, 64, 4, false, true), None);
    }
}
