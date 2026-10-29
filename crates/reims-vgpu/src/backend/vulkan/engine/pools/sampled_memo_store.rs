//! Candidate 33: the engine half of resident compute sampled inputs. See
//! [`crate::backend::vulkan::engine::sampled_memo`].
//!
//! Each entry owns one sampled-only image (popped off the transient live list
//! at acquire, so the ring never recycles it) that a single staging upload
//! filled and left in `SHADER_READ_ONLY_OPTIMAL`. An entry is never written
//! again after that upload: a new version allocates a new image and the old
//! one goes through [`ResourcePools::dispose`], which parks it until every
//! submission that could still read it has retired.
//!
//! An entry only becomes servable at [`ResourcePools::sampled_memo_commit`],
//! called after the dispatch that fills it was submitted. One whose dispatch
//! failed before submit stays uncommitted, is never published to the runtime's
//! index, and is replaced by the next upload of its key.

use super::*;
use crate::backend::vulkan::engine::sampled_memo::{self, SampledMemoTag};

pub(crate) struct SampledMemoEntry {
    pub(crate) slot: StorageImageSlot,
    pub(crate) version: u64,
    pub(crate) hash: u64,
    pub(crate) committed: bool,
    pub(crate) bytes: u64,
    pub(crate) last_use_ms: u64,
}

#[derive(Default)]
pub(crate) struct SampledMemoStore {
    pub(crate) entries: HashMap<ComputeStorageResidencyKey, SampledMemoEntry>,
    pub(crate) bytes: u64,
}

impl ResourcePools {
    /// The committed image for `tag`, if it is still held at that version and
    /// shape.
    pub(crate) fn sampled_memo_get(
        &mut self,
        tag: &SampledMemoTag,
        key: StorageImageKey,
    ) -> Option<StorageImageSlot> {
        let e = self.sampled_memo.entries.get_mut(&tag.key)?;
        if !e.committed || e.version != tag.version || e.slot.key != key {
            return None;
        }
        e.last_use_ms = sampled_memo::now_ms();
        Some(e.slot)
    }

    /// A fresh image to fill for `tag`, owned by the store (uncommitted).
    /// Any previous entry for the key is disposed through the graveyard.
    pub(crate) unsafe fn sampled_memo_prepare(
        &mut self,
        ctx: &DeviceContext,
        tag: SampledMemoTag,
        key: StorageImageKey,
        hash: u64,
        bytes: u64,
        counters: &EngineCounters,
    ) -> Result<StorageImageSlot, DrawError> {
        if let Some(old) = self.sampled_memo.entries.remove(&tag.key) {
            self.sampled_memo.bytes = self.sampled_memo.bytes.saturating_sub(old.bytes);
            sampled_memo::unpublish(&tag.key);
            self.dispose(
                &ctx.device,
                DeferredHandle::Image {
                    image: old.slot.image,
                    view: old.slot.view,
                    memory: old.slot.memory,
                },
            );
        }
        let slot = self.acquire_storage_image(ctx, key, counters)?;
        // Detach from the transient live list: the store owns it now.
        match self.entry.storage_image_live.last() {
            Some(live) if live.image == slot.image => {
                self.entry.storage_image_live.pop();
            }
            _ => {
                if let Some(at) = self
                    .entry
                    .storage_image_live
                    .iter()
                    .rposition(|live| live.image == slot.image)
                {
                    self.entry.storage_image_live.remove(at);
                }
            }
        }
        self.sampled_memo.entries.insert(
            tag.key,
            SampledMemoEntry {
                slot,
                version: tag.version,
                hash,
                committed: false,
                bytes,
                last_use_ms: sampled_memo::now_ms(),
            },
        );
        self.sampled_memo.bytes = self.sampled_memo.bytes.saturating_add(bytes);
        Ok(slot)
    }

    /// Mark the entries filled by a submitted dispatch servable, then trim to
    /// the byte cap. `touched` names every key this request used or filled;
    /// none of those is evicted.
    pub(crate) unsafe fn sampled_memo_commit(
        &mut self,
        ctx: &DeviceContext,
        filled: &[SampledMemoTag],
        touched: &[ComputeStorageResidencyKey],
    ) {
        for tag in filled {
            if let Some(e) = self.sampled_memo.entries.get_mut(&tag.key) {
                if e.version == tag.version && !e.committed {
                    e.committed = true;
                    sampled_memo::publish(tag.key, e.version, e.hash);
                    sampled_memo::note("c33_ri_inserted", 1);
                }
            }
        }
        let cap = sampled_memo::cap_bytes();
        if self.sampled_memo.bytes <= cap {
            return;
        }
        let now = sampled_memo::now_ms();
        // Candidate 34: the LRU pick is the pure, unit-tested `pick_victims`.
        let candidates: Vec<(ComputeStorageResidencyKey, u64, u64, u64, bool)> = self
            .sampled_memo
            .entries
            .iter()
            .map(|(k, e)| {
                (
                    *k,
                    e.bytes,
                    e.last_use_ms,
                    sampled_memo::last_query_ms(k),
                    touched.contains(k),
                )
            })
            .collect();
        let victims = sampled_memo::pick_victims(&candidates, self.sampled_memo.bytes, cap, now);
        let mut evicted = 0u64;
        for k in victims {
            if let Some(old) = self.sampled_memo.entries.remove(&k) {
                self.sampled_memo.bytes = self.sampled_memo.bytes.saturating_sub(old.bytes);
                sampled_memo::unpublish(&k);
                self.dispose(
                    &ctx.device,
                    DeferredHandle::Image {
                        image: old.slot.image,
                        view: old.slot.view,
                        memory: old.slot.memory,
                    },
                );
                evicted += 1;
            }
        }
        sampled_memo::note("c33_ri_evicted", evicted);
    }

    /// Device teardown: destroy every retained image now.
    pub(crate) unsafe fn sampled_memo_destroy_all(&mut self, device: &ash::Device) {
        for (_, e) in self.sampled_memo.entries.drain() {
            device.destroy_image_view(e.slot.view, None);
            device.destroy_image(e.slot.image, None);
            device.free_memory(e.slot.memory, None);
        }
        self.sampled_memo.bytes = 0;
        sampled_memo::unpublish_all();
    }
}
