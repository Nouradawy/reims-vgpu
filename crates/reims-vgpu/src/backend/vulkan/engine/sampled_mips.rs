//! Upload placement for complete byte-backed 2D or cube mip chains. The same
//! plan validates every level and records every copy, including block/offset alignment.

use super::types::{SampledImageResource, SampledSource};
use ash::vk;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MipUploadDecline {
    Shape,
    LevelCount,
    Footprint,
    LevelLength {
        level: u32,
        actual: usize,
        expected: usize,
    },
    SizeOverflow,
}

pub(super) fn copies(
    resource: &SampledImageResource,
    levels: &[std::sync::Arc<Vec<u8>>],
) -> Result<Vec<vk::BufferImageCopy>, MipUploadDecline> {
    use MipUploadDecline as R;
    let layer_count = match resource.kind {
        reims_vgpu_core::texture_shape::TextureKind::D2 if resource.layers == 1 => 1,
        reims_vgpu_core::texture_shape::TextureKind::Cube
            if resource.layers == 6 && resource.width == resource.height =>
        {
            6
        }
        _ => return Err(R::Shape),
    };
    if resource.multisampled {
        return Err(R::Shape);
    }
    let max_levels = u32::BITS - resource.width.max(resource.height).leading_zeros();
    if levels.is_empty() || levels.len() > max_levels as usize {
        return Err(R::LevelCount);
    }
    let block =
        super::super::translate::pixel::vk_block_geometry(resource.format).ok_or(R::Footprint)?;
    let mut divisor = u64::from(block.bytes);
    let mut other = 4;
    while other != 0 {
        (divisor, other) = (other, divisor % other);
    }
    let alignment = u64::from(block.bytes) / divisor * 4;
    let mut offset = 0u64;
    let mut copies = Vec::with_capacity(levels.len());
    for (level, bytes) in levels.iter().enumerate() {
        let width = reims_vgpu_protocol::extent::mip_extent(resource.width, level as u32);
        let height = reims_vgpu_protocol::extent::mip_extent(resource.height, level as u32);
        let expected = reims_vgpu_protocol::extent::tight_layered_block_bytes(
            width,
            height,
            layer_count,
            block,
        )
        .ok_or(R::SizeOverflow)?;
        if bytes.len() != expected {
            return Err(R::LevelLength {
                level: level as u32,
                actual: bytes.len(),
                expected,
            });
        }
        offset = offset.checked_add(alignment - 1).ok_or(R::SizeOverflow)? / alignment * alignment;
        copies.push(
            vk::BufferImageCopy::default()
                .buffer_offset(offset)
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: level as u32,
                    base_array_layer: 0,
                    layer_count,
                })
                .image_extent(vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                }),
        );
        offset = offset.checked_add(expected as u64).ok_or(R::SizeOverflow)?;
        usize::try_from(offset).map_err(|_| R::SizeOverflow)?;
    }
    Ok(copies)
}

pub(super) fn level_count(source: &SampledSource) -> u32 {
    match source {
        SampledSource::MipmappedBytes(levels) => levels.len() as u32,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn every_level_has_its_own_extent_and_aligned_copy() {
        let resource = SampledImageResource {
            binding: 32,
            array_element: 0,
            descriptor_count: 1,
            width: 5,
            height: 3,
            layers: 1,
            kind: reims_vgpu_core::texture_shape::TextureKind::D2,
            multisampled: false,
            source: SampledSource::Bytes(Arc::new(vec![])),
            byte_origin: Default::default(),
            format: vk::Format::R8_UNORM,
            identity: None,
            swizzle: Default::default(),
        };
        let levels = vec![
            Arc::new(vec![1; 15]),
            Arc::new(vec![2; 2]),
            Arc::new(vec![3; 1]),
        ];
        let plan = copies(&resource, &levels).unwrap();
        assert_eq!(
            plan.iter()
                .map(|copy| (
                    copy.buffer_offset,
                    copy.image_subresource.mip_level,
                    copy.image_extent.width,
                    copy.image_extent.height
                ))
                .collect::<Vec<_>>(),
            [(0, 0, 5, 3), (16, 1, 2, 1), (20, 2, 1, 1)]
        );
        assert!(matches!(
            copies(&resource, &levels[..0]),
            Err(MipUploadDecline::LevelCount)
        ));
        let mut short = levels.clone();
        short[1] = Arc::new(vec![2]);
        assert!(matches!(
            copies(&resource, &short),
            Err(MipUploadDecline::LevelLength { level: 1, .. })
        ));
    }

    #[test]
    fn cube_mips_copy_all_six_faces_at_each_level() {
        let resource = SampledImageResource {
            binding: 32,
            array_element: 0,
            descriptor_count: 1,
            width: 8,
            height: 8,
            layers: 6,
            kind: reims_vgpu_core::texture_shape::TextureKind::Cube,
            multisampled: false,
            source: SampledSource::Bytes(Arc::new(vec![])),
            byte_origin: Default::default(),
            format: vk::Format::R8_UNORM,
            identity: None,
            swizzle: Default::default(),
        };
        let levels = vec![
            Arc::new(vec![1; 8 * 8 * 6]),
            Arc::new(vec![2; 4 * 4 * 6]),
            Arc::new(vec![3; 2 * 2 * 6]),
            Arc::new(vec![4; 6]),
        ];
        let plan = copies(&resource, &levels).expect("a complete cube pyramid uploads");
        assert_eq!(
            plan.iter()
                .map(|copy| (
                    copy.image_subresource.mip_level,
                    copy.image_subresource.base_array_layer,
                    copy.image_subresource.layer_count,
                    copy.image_extent.width,
                    copy.image_extent.height,
                ))
                .collect::<Vec<_>>(),
            [
                (0, 0, 6, 8, 8),
                (1, 0, 6, 4, 4),
                (2, 0, 6, 2, 2),
                (3, 0, 6, 1, 1),
            ]
        );
        let mut missing_face = levels.clone();
        missing_face[1] = Arc::new(vec![2; 4 * 4 * 5]);
        assert!(matches!(
            copies(&resource, &missing_face),
            Err(MipUploadDecline::LevelLength { level: 1, .. })
        ));
    }
}
