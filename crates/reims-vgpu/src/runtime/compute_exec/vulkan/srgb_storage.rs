//! Transfer between guest sRGB texels and a linear float storage intermediate.
//! The intermediate is never exported as a native guest resident.

#[derive(Clone, Copy, Debug)]
pub(crate) enum SrgbStorage {
    Rgba,
    Bgra,
}

impl SrgbStorage {
    pub(super) fn for_format(format: u16) -> Option<Self> {
        use reims_vgpu_protocol::pixel_format::*;
        match format {
            MTL_FORMAT_RGBA8_UNORM_SRGB => Some(Self::Rgba),
            MTL_FORMAT_BGRA8_UNORM_SRGB => Some(Self::Bgra),
            _ => None,
        }
    }

    fn order(self) -> [usize; 4] {
        match self {
            Self::Rgba => [0, 1, 2, 3],
            Self::Bgra => [2, 1, 0, 3],
        }
    }

    pub(super) fn decode(self, bytes: &[u8]) -> Option<Vec<u8>> {
        if bytes.len() % 4 != 0 {
            return None;
        }
        let mut result = Vec::with_capacity(bytes.len().checked_mul(4)?);
        for pixel in bytes.chunks_exact(4) {
            for (channel, index) in self.order().into_iter().enumerate() {
                let encoded = f32::from(pixel[index]) / 255.0;
                let linear = if channel == 3 {
                    encoded
                } else if encoded <= 0.04045 {
                    encoded / 12.92
                } else {
                    ((encoded + 0.055) / 1.055).powf(2.4)
                };
                result.extend_from_slice(&linear.to_le_bytes());
            }
        }
        Some(result)
    }

    pub(super) fn encode(self, bytes: &[u8]) -> Option<Vec<u8>> {
        if bytes.len() % 16 != 0 {
            return None;
        }
        let mut result = Vec::with_capacity(bytes.len() / 4);
        for pixel in bytes.chunks_exact(16) {
            let mut encoded = [0; 4];
            for (channel, index) in self.order().into_iter().enumerate() {
                let value =
                    f32::from_le_bytes(pixel[channel * 4..channel * 4 + 4].try_into().ok()?)
                        .clamp(0.0, 1.0);
                let value = if channel == 3 {
                    value
                } else if value <= 0.0031308 {
                    12.92 * value
                } else {
                    1.055 * value.powf(1.0 / 2.4) - 0.055
                };
                encoded[index] = (value * 255.0).round() as u8;
            }
            result.extend_from_slice(&encoded);
        }
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_guest_codes_roundtrip_in_both_channel_orders_with_linear_alpha() {
        let bytes: Vec<u8> = (0..=255u8).flat_map(|v| [v, 255 - v, v / 2, v]).collect();
        for format in [SrgbStorage::Rgba, SrgbStorage::Bgra] {
            assert_eq!(
                format.encode(&format.decode(&bytes).unwrap()).unwrap(),
                bytes
            );
        }
        let half: Vec<u8> = [0.5f32, 0.25, 0.0, 0.5]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        assert_eq!(
            SrgbStorage::Rgba.encode(&half),
            Some(vec![188, 137, 0, 128])
        );
        assert_eq!(
            SrgbStorage::Bgra.encode(&half),
            Some(vec![0, 137, 188, 128])
        );
        assert!(SrgbStorage::Bgra.encode(&[0; 15]).is_none());
        assert!(SrgbStorage::Rgba.decode(&[0; 3]).is_none());
    }
}
