//! IOSurface subresource views, sub-rect aliasing, and validation.
//!
//! Clean-room implementation based on public Apple IOSurface and Khronos Vulkan specifications.
//! Covers Phase 5.1: relaxed sampled-size vs render-target size equality checks for sampling,
//! sub-rect alias validation, and persistent alias synchronization.

use std::collections::HashMap;
use super::census;

/// Key uniquely identifying an offset or sub-rect view alias over an IOSurface plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SubrectViewKey {
    pub surface_id: u32,
    pub plane_index: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub row_stride: u32,
    pub format: u32,
}

/// Reason why an IOSurface view or sub-rect alias was declined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewDecline {
    /// Requested view extent or offset extends beyond the plane boundary.
    Geometry,
    /// Requested row stride is below tight row byte requirements or invalid.
    Stride,
    /// Format reinterpretation across incompatible texel layouts.
    Format,
    /// Offset/stride configuration cannot be represented on this Vulkan target without copying.
    SubrectUnsupported,
    /// Parent surface plane has been released or destroyed.
    Released,
}

impl ViewDecline {
    /// Emit the appropriate census slug for this decline.
    pub fn emit_census(self) {
        match self {
            Self::Geometry => census::note_view_declined("geometry"),
            Self::Stride => census::note_view_declined("stride"),
            Self::Format => census::note_view_declined("format"),
            Self::SubrectUnsupported => census::note_view_subrect_unsupported(),
            Self::Released => census::note_view_declined("released"),
        }
    }
}

/// Validates whether a sub-rect view fits within the parent plane boundaries and has valid stride.
pub fn validate_subrect(
    plane_w: u32,
    plane_h: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    bpe: u32,
    row_stride: u32,
) -> Result<(), ViewDecline> {
    if w == 0 || h == 0 {
        return Err(ViewDecline::Geometry);
    }
    let end_x = x.checked_add(w).ok_or(ViewDecline::Geometry)?;
    let end_y = y.checked_add(h).ok_or(ViewDecline::Geometry)?;
    if end_x > plane_w || end_y > plane_h {
        return Err(ViewDecline::Geometry);
    }
    if bpe > 0 {
        let tight_bytes = w.checked_mul(bpe).ok_or(ViewDecline::Stride)?;
        if row_stride < tight_bytes {
            return Err(ViewDecline::Stride);
        }
    }
    Ok(())
}

/// Tracking record for a persistent alias image representing a sub-rect view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewCacheEntry {
    pub parent_generation: u64,
    pub synced_generation: u64,
    pub dirty_from_alias: bool,
}

impl ViewCacheEntry {
    /// Returns true if the parent image has updated and the alias requires resynchronization.
    pub fn needs_sync_from_parent(&self, current_parent_gen: u64) -> bool {
        self.synced_generation != current_parent_gen
    }

    /// Mark the alias image as synchronized with parent generation.
    pub fn mark_synchronized(&mut self, parent_gen: u64) {
        self.parent_generation = parent_gen;
        self.synced_generation = parent_gen;
        self.dirty_from_alias = false;
    }
}

/// Registry cache managing persistent sub-rect alias images.
#[derive(Default, Debug)]
pub struct ViewCache {
    aliases: HashMap<SubrectViewKey, ViewCacheEntry>,
}

impl ViewCache {
    pub fn new() -> Self {
        Self {
            aliases: HashMap::new(),
        }
    }

    /// Look up or register an alias image entry.
    pub fn get_or_register(&mut self, key: SubrectViewKey, parent_gen: u64) -> &mut ViewCacheEntry {
        self.aliases.entry(key).or_insert_with(|| ViewCacheEntry {
            parent_generation: parent_gen,
            synced_generation: 0,
            dirty_from_alias: false,
        })
    }

    /// Invalidate all aliases associated with a specific surface and plane.
    pub fn invalidate_plane(&mut self, surface_id: u32, plane_index: u32) {
        self.aliases.retain(|k, _| !(k.surface_id == surface_id && k.plane_index == plane_index));
    }

    /// Count of tracked alias images.
    pub fn len(&self) -> usize {
        self.aliases.len()
    }

    pub fn is_empty(&self) -> bool {
        self.aliases.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_plane_view_validates() {
        assert_eq!(
            validate_subrect(384, 448, 0, 0, 384, 448, 4, 384 * 4),
            Ok(())
        );
    }

    #[test]
    fn subrect_within_bounds_validates() {
        assert_eq!(
            validate_subrect(465, 285, 10, 20, 384, 200, 4, 384 * 4),
            Ok(())
        );
    }

    #[test]
    fn subrect_exceeding_bounds_fails_geometry() {
        assert_eq!(
            validate_subrect(465, 285, 100, 20, 384, 200, 4, 384 * 4),
            Err(ViewDecline::Geometry)
        );
        assert_eq!(
            validate_subrect(465, 285, 0, 0, 466, 285, 4, 466 * 4),
            Err(ViewDecline::Geometry)
        );
    }

    #[test]
    fn zero_extent_fails_geometry() {
        assert_eq!(
            validate_subrect(465, 285, 0, 0, 0, 285, 4, 400),
            Err(ViewDecline::Geometry)
        );
    }

    #[test]
    fn row_stride_below_tight_fails_stride() {
        assert_eq!(
            validate_subrect(465, 285, 0, 0, 100, 100, 4, 399),
            Err(ViewDecline::Stride)
        );
    }

    #[test]
    fn view_cache_sync_lifecycle() {
        let mut cache = ViewCache::new();
        let key = SubrectViewKey {
            surface_id: 101,
            plane_index: 0,
            x: 0,
            y: 0,
            width: 64,
            height: 64,
            row_stride: 256,
            format: 0x51,
        };

        let entry = cache.get_or_register(key, 1);
        assert!(entry.needs_sync_from_parent(1));

        entry.mark_synchronized(1);
        assert!(!entry.needs_sync_from_parent(1));
        assert!(entry.needs_sync_from_parent(2));

        cache.invalidate_plane(101, 0);
        assert_eq!(cache.len(), 0);
    }
}
