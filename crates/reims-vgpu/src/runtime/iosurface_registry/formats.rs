//! IOSurface format mapping table, capabilities, and conversion fallbacks.
//!
//! Clean-room implementation based on public Apple Metal/IOSurface documentation
//! and Khronos Vulkan specifications.
//! Appendix A format mappings, block geometries, sRGB pairings, and decompression/unpack fallbacks.

#[cfg(feature = "backend-vulkan")]
use ash::vk;

use super::census;

// Canonical Metal pixel format ordinals (from Apple MTLPixelFormat reference).
pub const MTL_FORMAT_R8_UNORM: u32 = 10;
pub const MTL_FORMAT_R8_SNORM: u32 = 11;
pub const MTL_FORMAT_R8_SRGB: u32 = 13;
pub const MTL_FORMAT_R16_SNORM: u32 = 22;
pub const MTL_FORMAT_RG8_UNORM: u32 = 30;
pub const MTL_FORMAT_RG8_SNORM: u32 = 31;
pub const MTL_FORMAT_RG8_SRGB: u32 = 32;
pub const MTL_FORMAT_B5G6R5_UNORM: u32 = 40;
pub const MTL_FORMAT_A1BGR5_UNORM: u32 = 41;
pub const MTL_FORMAT_ABGR4_UNORM: u32 = 42;
pub const MTL_FORMAT_BGR5A1_UNORM: u32 = 43;
pub const MTL_FORMAT_RGBA8_UNORM: u32 = 70;
pub const MTL_FORMAT_RGBA8_UNORM_SRGB: u32 = 71;
pub const MTL_FORMAT_RGBA8_SNORM: u32 = 72;
pub const MTL_FORMAT_BGRA8_UNORM: u32 = 80;
pub const MTL_FORMAT_BGRA8_UNORM_SRGB: u32 = 81;
pub const MTL_FORMAT_RGB10A2_UNORM: u32 = 90;
pub const MTL_FORMAT_BGR10A2_UNORM: u32 = 94;
pub const MTL_FORMAT_RGBA16_FLOAT: u32 = 115;
pub const MTL_FORMAT_RGBA16_SNORM: u32 = 112;
pub const MTL_FORMAT_RG32_FLOAT: u32 = 105;
pub const MTL_FORMAT_RG32_UINT: u32 = 103;
pub const MTL_FORMAT_RG32_SINT: u32 = 104;
pub const MTL_FORMAT_BGRA10_XR: u32 = 552;
pub const MTL_FORMAT_BGR10_XR: u32 = 554;
pub const MTL_FORMAT_ETC2_RGB8: u32 = 180;
pub const MTL_FORMAT_ASTC_4X4_LDR: u32 = 186;

/// Descriptor characterizing format geometry, Vulkan target, and fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatDescriptor {
    pub metal_format: u32,
    pub name: &'static str,
    pub bytes_per_block: u32,
    pub block_width: u32,
    pub block_height: u32,
    pub component_count: u8,
    pub srgb_sibling: Option<u32>,
    pub is_srgb: bool,
    pub is_compressed: bool,
    #[cfg(feature = "backend-vulkan")]
    pub vk_target: vk::Format,
    #[cfg(feature = "backend-vulkan")]
    pub vk_fallback: Option<vk::Format>,
}

#[cfg(feature = "backend-vulkan")]
pub static FORMAT_TABLE: &[FormatDescriptor] = &[
    FormatDescriptor {
        metal_format: MTL_FORMAT_R8_UNORM,
        name: "R8Unorm",
        bytes_per_block: 1,
        block_width: 1,
        block_height: 1,
        component_count: 1,
        srgb_sibling: Some(MTL_FORMAT_R8_SRGB),
        is_srgb: false,
        is_compressed: false,
        vk_target: vk::Format::R8_UNORM,
        vk_fallback: None,
    },
    FormatDescriptor {
        metal_format: MTL_FORMAT_R8_SRGB,
        name: "R8Unorm_sRGB",
        bytes_per_block: 1,
        block_width: 1,
        block_height: 1,
        component_count: 1,
        srgb_sibling: Some(MTL_FORMAT_R8_UNORM),
        is_srgb: true,
        is_compressed: false,
        vk_target: vk::Format::R8_SRGB,
        vk_fallback: Some(vk::Format::R8_UNORM),
    },
    FormatDescriptor {
        metal_format: MTL_FORMAT_R8_SNORM,
        name: "R8Snorm",
        bytes_per_block: 1,
        block_width: 1,
        block_height: 1,
        component_count: 1,
        srgb_sibling: None,
        is_srgb: false,
        is_compressed: false,
        vk_target: vk::Format::R8_SNORM,
        vk_fallback: Some(vk::Format::R16_SFLOAT),
    },
    FormatDescriptor {
        metal_format: MTL_FORMAT_RG8_UNORM,
        name: "RG8Unorm",
        bytes_per_block: 2,
        block_width: 1,
        block_height: 1,
        component_count: 2,
        srgb_sibling: Some(MTL_FORMAT_RG8_SRGB),
        is_srgb: false,
        is_compressed: false,
        vk_target: vk::Format::R8G8_UNORM,
        vk_fallback: None,
    },
    FormatDescriptor {
        metal_format: MTL_FORMAT_RG8_SRGB,
        name: "RG8Unorm_sRGB",
        bytes_per_block: 2,
        block_width: 1,
        block_height: 1,
        component_count: 2,
        srgb_sibling: Some(MTL_FORMAT_RG8_UNORM),
        is_srgb: true,
        is_compressed: false,
        vk_target: vk::Format::R8G8_SRGB,
        vk_fallback: Some(vk::Format::R8G8_UNORM),
    },
    FormatDescriptor {
        metal_format: MTL_FORMAT_RG8_SNORM,
        name: "RG8Snorm",
        bytes_per_block: 2,
        block_width: 1,
        block_height: 1,
        component_count: 2,
        srgb_sibling: None,
        is_srgb: false,
        is_compressed: false,
        vk_target: vk::Format::R8G8_SNORM,
        vk_fallback: Some(vk::Format::R16G16_SFLOAT),
    },
    FormatDescriptor {
        metal_format: MTL_FORMAT_RGBA8_UNORM,
        name: "RGBA8Unorm",
        bytes_per_block: 4,
        block_width: 1,
        block_height: 1,
        component_count: 4,
        srgb_sibling: Some(MTL_FORMAT_RGBA8_UNORM_SRGB),
        is_srgb: false,
        is_compressed: false,
        vk_target: vk::Format::R8G8B8A8_UNORM,
        vk_fallback: None,
    },
    FormatDescriptor {
        metal_format: MTL_FORMAT_RGBA8_UNORM_SRGB,
        name: "RGBA8Unorm_sRGB",
        bytes_per_block: 4,
        block_width: 1,
        block_height: 1,
        component_count: 4,
        srgb_sibling: Some(MTL_FORMAT_RGBA8_UNORM),
        is_srgb: true,
        is_compressed: false,
        vk_target: vk::Format::R8G8B8A8_SRGB,
        vk_fallback: Some(vk::Format::R8G8B8A8_UNORM),
    },
    FormatDescriptor {
        metal_format: MTL_FORMAT_RGBA8_SNORM,
        name: "RGBA8Snorm",
        bytes_per_block: 4,
        block_width: 1,
        block_height: 1,
        component_count: 4,
        srgb_sibling: None,
        is_srgb: false,
        is_compressed: false,
        vk_target: vk::Format::R8G8B8A8_SNORM,
        vk_fallback: Some(vk::Format::R16G16B16A16_SFLOAT),
    },
    FormatDescriptor {
        metal_format: MTL_FORMAT_BGRA8_UNORM,
        name: "BGRA8Unorm",
        bytes_per_block: 4,
        block_width: 1,
        block_height: 1,
        component_count: 4,
        srgb_sibling: Some(MTL_FORMAT_BGRA8_UNORM_SRGB),
        is_srgb: false,
        is_compressed: false,
        vk_target: vk::Format::B8G8R8A8_UNORM,
        vk_fallback: None,
    },
    FormatDescriptor {
        metal_format: MTL_FORMAT_BGRA8_UNORM_SRGB,
        name: "BGRA8Unorm_sRGB",
        bytes_per_block: 4,
        block_width: 1,
        block_height: 1,
        component_count: 4,
        srgb_sibling: Some(MTL_FORMAT_BGRA8_UNORM),
        is_srgb: true,
        is_compressed: false,
        vk_target: vk::Format::B8G8R8A8_SRGB,
        vk_fallback: Some(vk::Format::B8G8R8A8_UNORM),
    },
    FormatDescriptor {
        metal_format: MTL_FORMAT_RGB10A2_UNORM,
        name: "RGB10A2Unorm",
        bytes_per_block: 4,
        block_width: 1,
        block_height: 1,
        component_count: 4,
        srgb_sibling: None,
        is_srgb: false,
        is_compressed: false,
        vk_target: vk::Format::A2B10G10R10_UNORM_PACK32,
        vk_fallback: None,
    },
    FormatDescriptor {
        metal_format: MTL_FORMAT_BGR10A2_UNORM,
        name: "BGR10A2Unorm",
        bytes_per_block: 4,
        block_width: 1,
        block_height: 1,
        component_count: 4,
        srgb_sibling: None,
        is_srgb: false,
        is_compressed: false,
        vk_target: vk::Format::A2R10G10B10_UNORM_PACK32,
        vk_fallback: None,
    },
    FormatDescriptor {
        metal_format: MTL_FORMAT_RGBA16_FLOAT,
        name: "RGBA16Float",
        bytes_per_block: 8,
        block_width: 1,
        block_height: 1,
        component_count: 4,
        srgb_sibling: None,
        is_srgb: false,
        is_compressed: false,
        vk_target: vk::Format::R16G16B16A16_SFLOAT,
        vk_fallback: None,
    },
    FormatDescriptor {
        metal_format: MTL_FORMAT_RG32_FLOAT,
        name: "RG32Float",
        bytes_per_block: 8,
        block_width: 1,
        block_height: 1,
        component_count: 2,
        srgb_sibling: None,
        is_srgb: false,
        is_compressed: false,
        vk_target: vk::Format::R32G32_SFLOAT,
        vk_fallback: None,
    },
    FormatDescriptor {
        metal_format: MTL_FORMAT_RG32_UINT,
        name: "RG32Uint",
        bytes_per_block: 8,
        block_width: 1,
        block_height: 1,
        component_count: 2,
        srgb_sibling: None,
        is_srgb: false,
        is_compressed: false,
        vk_target: vk::Format::R32G32_UINT,
        vk_fallback: None,
    },
    FormatDescriptor {
        metal_format: MTL_FORMAT_RG32_SINT,
        name: "RG32Sint",
        bytes_per_block: 8,
        block_width: 1,
        block_height: 1,
        component_count: 2,
        srgb_sibling: None,
        is_srgb: false,
        is_compressed: false,
        vk_target: vk::Format::R32G32_SINT,
        vk_fallback: None,
    },
    FormatDescriptor {
        metal_format: MTL_FORMAT_ETC2_RGB8,
        name: "ETC2_RGB8",
        bytes_per_block: 8,
        block_width: 4,
        block_height: 4,
        component_count: 3,
        srgb_sibling: None,
        is_srgb: false,
        is_compressed: true,
        vk_target: vk::Format::ETC2_R8G8B8_UNORM_BLOCK,
        vk_fallback: Some(vk::Format::R8G8B8A8_UNORM),
    },
    FormatDescriptor {
        metal_format: MTL_FORMAT_ASTC_4X4_LDR,
        name: "ASTC_4x4_LDR",
        bytes_per_block: 16,
        block_width: 4,
        block_height: 4,
        component_count: 4,
        srgb_sibling: None,
        is_srgb: false,
        is_compressed: true,
        vk_target: vk::Format::ASTC_4X4_UNORM_BLOCK,
        vk_fallback: Some(vk::Format::R8G8B8A8_UNORM),
    },
];

/// Look up format descriptor by Metal format identifier.
#[cfg(feature = "backend-vulkan")]
pub fn lookup_format(metal_format: u32) -> Option<&'static FormatDescriptor> {
    FORMAT_TABLE.iter().find(|d| d.metal_format == metal_format)
}

/// Resolves a Metal format to a Vulkan format, querying device capability and falling back if necessary.
#[cfg(feature = "backend-vulkan")]
pub fn resolve_format_with_capabilities(
    metal_format: u32,
    is_vk_supported: impl Fn(vk::Format) -> bool,
) -> Result<vk::Format, &'static str> {
    let Some(desc) = lookup_format(metal_format) else {
        census::note_format_unsupported(&format!("unmapped_{metal_format:#x}"));
        return Err("format not mapped");
    };

    if is_vk_supported(desc.vk_target) {
        Ok(desc.vk_target)
    } else if let Some(fallback) = desc.vk_fallback {
        if is_vk_supported(fallback) {
            census::note_format_fallback(desc.name, "fallback");
            Ok(fallback)
        } else {
            census::note_format_unsupported(desc.name);
            Err("fallback unsupported")
        }
    } else {
        census::note_format_unsupported(desc.name);
        Err("target unsupported")
    }
}

/// Convert IEEE 754 32-bit float to 16-bit half-float.
pub fn f32_to_f16(val: f32) -> u16 {
    let bits = val.to_bits();
    let sign = (bits >> 16) & 0x8000;
    let exp = ((bits >> 23) & 0xff) as i32;
    let frac = bits & 0x7f_ffff;

    if exp == 0xff {
        return (sign | 0x7c00 | if frac != 0 { (frac >> 13).max(1) } else { 0 }) as u16;
    }
    let new_exp = exp - 127 + 15;
    if new_exp >= 31 {
        return (sign | 0x7c00) as u16;
    }
    if new_exp <= 0 {
        if new_exp < -10 {
            return sign as u16;
        }
        let frac = frac | 0x80_0000;
        let shift = 14 - new_exp;
        return (sign | (frac >> shift)) as u16;
    }
    (sign | ((new_exp as u32) << 10) | (frac >> 13)) as u16
}

/// Unpack Apple BGRA10_XR (64 bpp: 16-bit per component, reference 1.0 = 512) into RGBA16Float (8 bytes/texel).
pub fn unpack_bgra10_xr_to_rgba16f(src: &[u8], dst: &mut [u8], pixel_count: usize) -> Result<(), &'static str> {
    if src.len() < pixel_count * 8 {
        return Err("src too short for BGRA10_XR");
    }
    if dst.len() < pixel_count * 8 {
        return Err("dst too short for RGBA16Float");
    }

    for i in 0..pixel_count {
        let src_idx = i * 8;
        let dst_idx = i * 8;

        // BGRA order: 16-bit integers
        let b_raw = u16::from_le_bytes([src[src_idx], src[src_idx + 1]]);
        let g_raw = u16::from_le_bytes([src[src_idx + 2], src[src_idx + 3]]);
        let r_raw = u16::from_le_bytes([src[src_idx + 4], src[src_idx + 5]]);
        let a_raw = u16::from_le_bytes([src[src_idx + 6], src[src_idx + 7]]);

        // Reference 1.0 = 512.0
        let r_f32 = (r_raw as f32) / 512.0;
        let g_f32 = (g_raw as f32) / 512.0;
        let b_f32 = (b_raw as f32) / 512.0;
        let a_f32 = (a_raw as f32) / 512.0;

        let r_f16 = f32_to_f16(r_f32);
        let g_f16 = f32_to_f16(g_f32);
        let b_f16 = f32_to_f16(b_f32);
        let a_f16 = f32_to_f16(a_f32);

        // Destination RGBA16Float
        dst[dst_idx..dst_idx + 2].copy_from_slice(&r_f16.to_le_bytes());
        dst[dst_idx + 2..dst_idx + 4].copy_from_slice(&g_f16.to_le_bytes());
        dst[dst_idx + 4..dst_idx + 6].copy_from_slice(&b_f16.to_le_bytes());
        dst[dst_idx + 6..dst_idx + 8].copy_from_slice(&a_f16.to_le_bytes());
    }
    census::note_format_fallback("bgra10_xr", "rgba16f");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_table_srgb_pairing_symmetry() {
        for desc in FORMAT_TABLE {
            if let Some(sibling_id) = desc.srgb_sibling {
                let sibling = lookup_format(sibling_id)
                    .unwrap_or_else(|| panic!("missing srgb sibling for {}", desc.name));
                assert_eq!(
                    sibling.srgb_sibling,
                    Some(desc.metal_format),
                    "srgb sibling asymmetry between {} and {}",
                    desc.name,
                    sibling.name
                );
                assert_ne!(
                    desc.is_srgb, sibling.is_srgb,
                    "srgb flag agreement between siblings"
                );
            }
        }
    }

    #[test]
    fn format_table_block_invariants() {
        for desc in FORMAT_TABLE {
            assert!(desc.block_width >= 1, "invalid block width for {}", desc.name);
            assert!(desc.block_height >= 1, "invalid block height for {}", desc.name);
            assert!(desc.bytes_per_block >= 1, "invalid bytes per block for {}", desc.name);
            assert!(desc.component_count >= 1 && desc.component_count <= 4, "invalid components for {}", desc.name);
        }
    }

    #[test]
    fn resolve_supported_and_fallback_formats() {
        // Supported format resolves directly
        let res = resolve_format_with_capabilities(MTL_FORMAT_RGBA8_UNORM, |_| true);
        assert_eq!(res, Ok(vk::Format::R8G8B8A8_UNORM));

        // Format where native target unsupported falls back to fallback
        let res_fallback = resolve_format_with_capabilities(MTL_FORMAT_ASTC_4X4_LDR, |fmt| {
            fmt == vk::Format::R8G8B8A8_UNORM
        });
        assert_eq!(res_fallback, Ok(vk::Format::R8G8B8A8_UNORM));

        // Format where neither is supported errors cleanly
        let res_unsupported = resolve_format_with_capabilities(MTL_FORMAT_ASTC_4X4_LDR, |_| false);
        assert!(res_unsupported.is_err());
    }

    #[test]
    fn f32_to_f16_known_values() {
        assert_eq!(f32_to_f16(0.0), 0x0000);
        assert_eq!(f32_to_f16(1.0), 0x3c00);
        assert_eq!(f32_to_f16(-1.0), 0xbc00);
        assert_eq!(f32_to_f16(2.0), 0x4000);
    }

    #[test]
    fn bgra10_xr_unpack_roundtrip() {
        // Pixel: B=512 (1.0), G=256 (0.5), R=512 (1.0), A=512 (1.0)
        let mut src = [0u8; 8];
        src[0..2].copy_from_slice(&512u16.to_le_bytes()); // B
        src[2..4].copy_from_slice(&256u16.to_le_bytes()); // G
        src[4..6].copy_from_slice(&512u16.to_le_bytes()); // R
        src[6..8].copy_from_slice(&512u16.to_le_bytes()); // A

        let mut dst = [0u8; 8];
        assert_eq!(unpack_bgra10_xr_to_rgba16f(&src, &mut dst, 1), Ok(()));

        // Output RGBA16F:
        // R = 1.0 (0x3c00)
        let r_out = u16::from_le_bytes([dst[0], dst[1]]);
        assert_eq!(r_out, 0x3c00);

        // G = 0.5 (0x3800)
        let g_out = u16::from_le_bytes([dst[2], dst[3]]);
        assert_eq!(g_out, 0x3800);

        // B = 1.0 (0x3c00)
        let b_out = u16::from_le_bytes([dst[4], dst[5]]);
        assert_eq!(b_out, 0x3c00);

        // A = 1.0 (0x3c00)
        let a_out = u16::from_le_bytes([dst[6], dst[7]]);
        assert_eq!(a_out, 0x3c00);
    }
}
