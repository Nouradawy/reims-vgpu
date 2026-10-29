//! Reuse only a completed dispatch whose exact mapped outputs still exist.
//!
//! A texture-list reference is task-local and reusable. The shadow registry's
//! broad authority state cannot prove that a particular dispatch's output is
//! still present. The storage mirror and engine must agree on its generation.

use super::*;
use crate::backend::vulkan::engine as vk_engine;
use crate::model::ComputeStorageResidencyKey;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
enum SampleContents {
    Bytes(Vec<u8>),
    Resident(ComputeStorageResidencyKey, u32),
    /// Candidate 33: a retained sampled input; its bytes were not read.
    Retained(ComputeStorageResidencyKey, u64),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Sample {
    binding: u32,
    array_element: u32,
    descriptor_count: u32,
    format: u16,
    width: u32,
    height: u32,
    mip_levels: u32,
    contents: SampleContents,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Shape {
    pub task_id: u32,
    pub shader: Arc<Vec<u32>>,
    pub grid: [u32; 3],
    pub workgroup: [u32; 3],
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Key {
    shape: Shape,
    buffers: Vec<(u32, Vec<u8>)>,
    samples: Vec<Sample>,
    outputs: Vec<(u32, u32, u32, ComputeStorageResidencyKey)>,
}

pub(super) struct Candidate {
    key: Key,
    current_generations: Option<Vec<u32>>,
}

impl Candidate {
    /// The caller admits only write-only images, readonly buffers and constexpr
    /// samplers. Other dispatches execute normally rather than guessing their
    /// side effects or dynamic sampler state.
    pub(super) fn capture(
        shape: Shape,
        buffers: &[StagedBuffer],
        textures: &[StagedTexture<VulkanStage>],
    ) -> Option<Self> {
        let mut outputs = Vec::new();
        let mut generations = Vec::new();
        let mut samples = Vec::new();
        for texture in textures {
            if texture.is_storage {
                let TextureWriteback::MapperRefTexture { mapping_id, .. } = texture.writeback
                else {
                    // Linear texture refs and mapping ids are different domains.
                    return None;
                };
                let resident = texture.rail.residency?;
                if mapping_id == 0 || resident.key.mapping_id != mapping_id {
                    return None;
                }
                outputs.push((
                    texture.binding,
                    texture.rail.array_element,
                    texture.rail.descriptor_count,
                    resident.key,
                ));
                generations.push(texture.rail.serve.and_then(ResidentServe::seed_generation));
            } else {
                if texture.rail.multisample_target.is_some() {
                    return None;
                }
                // Staging can carry zero placeholder bytes for a resident.
                // Its generation must take priority over those bytes.
                let contents = match texture.rail.serve {
                    Some(ResidentServe::Sample(key, generation)) => {
                        SampleContents::Resident(key, generation)
                    }
                    Some(ResidentServe::Seed(_)) => return None,
                    None => match texture.rail.sampled_memo {
                        Some(vk_engine::sampled_memo::SampledMemoUse::Hit(tag)) => {
                            SampleContents::Retained(tag.key, tag.version)
                        }
                        _ => SampleContents::Bytes(texture.bytes.clone()),
                    },
                };
                samples.push(Sample {
                    binding: texture.binding,
                    array_element: texture.rail.array_element,
                    descriptor_count: texture.rail.descriptor_count,
                    format: texture.pixel_format,
                    width: texture.width,
                    height: texture.height,
                    mip_levels: texture.mip_levels,
                    contents,
                });
            }
        }
        if outputs.is_empty() {
            return None;
        }
        Some(Self {
            key: Key {
                shape,
                buffers: buffers
                    .iter()
                    .map(|b| (b.bind.index, b.bytes.clone()))
                    .collect(),
                samples,
                outputs,
            },
            current_generations: generations.into_iter().collect(),
        })
    }

    pub(super) fn completed_generations(&self, state: &DeviceState) -> Option<Vec<u32>> {
        self.key
            .outputs
            .iter()
            .map(|(_, _, _, key)| {
                let mirror = state.compute_storage_residency.get(key).copied()?;
                (vk_engine::compute_resident_storage_generation(key) == Some(mirror))
                    .then_some(mirror)
            })
            .collect()
    }
}

/// The previous successful eligible dispatch, owned by the device lifetime.
/// Forgetting an entry costs execution, never authoritative resource content.
#[derive(Debug, Default)]
pub(crate) struct DispatchMemo {
    last: Option<(Key, Vec<u32>)>,
}

impl DispatchMemo {
    pub(super) fn matches(&self, candidate: &Candidate) -> bool {
        self.last.as_ref().is_some_and(|(key, generations)| {
            key == &candidate.key && candidate.current_generations.as_ref() == Some(generations)
        })
    }

    pub(super) fn clear(&mut self) {
        self.last = None;
    }

    pub(super) fn record(&mut self, candidate: Option<Candidate>, generations: Option<Vec<u32>>) {
        self.last = candidate
            .zip(generations)
            .map(|(candidate, generations)| (candidate.key, generations));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapped(generation: Option<u32>) -> StagedTexture<VulkanStage> {
        let key = ComputeStorageResidencyKey {
            mapping_id: 7,
            map_generation: 3,
            surface_offset: 0,
            surface_bpr: 8,
            span_end: 8,
            width: 2,
            height: 1,
            pixel_format: 0x46,
            texture_ref: 0,
        };
        StagedTexture {
            binding: 480,
            pixel_format: key.pixel_format,
            storage_selector: None,
            width: 2,
            height: 1,
            mip_levels: 1,
            bytes: vec![0; 8],
            is_storage: true,
            writeback: TextureWriteback::MapperRefTexture {
                mapping_id: 7,
                surface_offset: 0,
                surface_bpr: 8,
                span_end: 8,
                width: 2,
                height: 1,
                format: key.pixel_format,
            },
            rail: VulkanStage {
                array_element: 0,
                descriptor_count: 1,
                residency: Some(ComputeStorageResidencyCandidate {
                    key,
                    seed_generation: generation.unwrap_or(0),
                }),
                serve: generation.map(ResidentServe::Seed),
                multisample_target: None,
                srgb_storage: None,
                sampled_memo: None,
            },
        }
    }

    fn shape() -> Shape {
        Shape {
            task_id: 2,
            shader: Arc::new(vec![1, 2, 3]),
            grid: [1; 3],
            workgroup: [1; 3],
        }
    }

    #[test]
    fn unchanged_output_hits_but_missing_or_changed_generation_executes() {
        let mut memo = DispatchMemo::default();
        memo.record(
            Candidate::capture(shape(), &[], &[mapped(None)]),
            Some(vec![4]),
        );
        assert!(memo.matches(&Candidate::capture(shape(), &[], &[mapped(Some(4))]).unwrap()));
        assert!(!memo.matches(&Candidate::capture(shape(), &[], &[mapped(Some(5))]).unwrap()));
        assert!(!memo.matches(&Candidate::capture(shape(), &[], &[mapped(None)]).unwrap()));
    }

    #[test]
    fn reused_refs_cannot_cross_task_shader_or_mapping_incarnation() {
        let mut memo = DispatchMemo::default();
        memo.record(
            Candidate::capture(shape(), &[], &[mapped(None)]),
            Some(vec![4]),
        );
        let mut other_task = shape();
        other_task.task_id += 1;
        assert!(!memo.matches(&Candidate::capture(other_task, &[], &[mapped(Some(4))]).unwrap()));
        let mut other_shader = shape();
        other_shader.shader = Arc::new(vec![1, 2, 4]);
        assert!(!memo.matches(&Candidate::capture(other_shader, &[], &[mapped(Some(4))]).unwrap()));
        let mut other_lifetime = mapped(Some(4));
        other_lifetime
            .rail
            .residency
            .as_mut()
            .unwrap()
            .key
            .map_generation += 1;
        assert!(!memo.matches(&Candidate::capture(shape(), &[], &[other_lifetime]).unwrap()));
    }

    #[test]
    fn linear_texture_ref_never_impersonates_a_registry_surface() {
        let mut texture = mapped(Some(4));
        texture.writeback = TextureWriteback::Linear {
            texture_ref: 7,
            gva: 4096,
            pixel_format: 0x46,
            row_stride: 8,
            width: 2,
            height: 1,
            bpp: 4,
            pages: StoreTargetPages::empty(),
        };
        assert!(Candidate::capture(shape(), &[], &[texture]).is_none());
    }

    #[test]
    fn sampled_resident_currency_outranks_placeholder_bytes() {
        let mut sample = mapped(Some(4));
        sample.is_storage = false;
        let key = sample.rail.residency.unwrap().key;
        sample.rail.serve = Some(ResidentServe::Sample(key, 4));
        let mut memo = DispatchMemo::default();
        memo.record(
            Candidate::capture(shape(), &[], &[mapped(None), sample]),
            Some(vec![8]),
        );
        let mut newer_sample = mapped(Some(4));
        newer_sample.is_storage = false;
        newer_sample.rail.serve = Some(ResidentServe::Sample(key, 5));
        assert!(!memo
            .matches(&Candidate::capture(shape(), &[], &[mapped(Some(8)), newer_sample]).unwrap()));
    }
}
