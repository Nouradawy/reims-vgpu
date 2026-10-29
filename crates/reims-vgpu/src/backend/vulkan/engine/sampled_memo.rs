//! Candidate 33: GPU-resident compute sampled inputs.
//!
//! `REIMS_VGPU_COMPUTE_RESIDENT_INPUTS=1` (default off). A compute pass's
//! sampled mapper-ref (IOSurface) input whose guest pages provably have not
//! changed since this device last uploaded them is bound from the image that
//! upload filled, instead of re-reading the guest pages and staging the same
//! bytes again.
//!
//! # When a binding is served from the retained image
//!
//! All of these, decided by the runtime at staging time:
//!
//! * the switch is on, the binding is sampled (never storage), single-level,
//!   and no other source (storage resident, persistent GPU target) claimed it;
//! * the hypervisor witness answers `WatchedAndUnwritten` for the window right
//!   now (`NoStamp` never qualifies);
//! * the version token equals the one recorded at upload. The token folds every
//!   writer this device knows of: the mapping's `content_generation` (guest
//!   driver statements and device writes), `surface_content_epoch`,
//!   `map_generation` (page-list churn), and the dirty-tracking stamp fields
//!   (token, stamped generation, stated generation), so a re-stamp that would
//!   reset the witness also retires the retained copy;
//! * the residency key (mapping, window, bpr, extent, format) is identical, so
//!   format and extent match by construction; the engine re-checks the shape.
//!
//! Anything else stages bytes exactly as before. Pyramids never reach here:
//! the mapper-ref rail is single-level by Metal rule, and the linear rail is
//! untouched.
//!
//! # Verify
//!
//! `REIMS_VGPU_COMPUTE_RESIDENT_VERIFY=1` re-reads the guest bytes on one in N
//! would-be hits (`REIMS_VGPU_COMPUTE_RESIDENT_VERIFY_EVERY`, default 16) and
//! compares their hash with the hash recorded at upload. A mismatch is logged
//! (`c33_resident_mismatch ...`), counted, and the fresh bytes are uploaded and
//! retained in place of the stale copy.
//!
//! Counters ride `store_routes` once a second: `c33_ri_hits`, `c33_ri_hit_bytes`,
//! `c33_ri_bytes_uploads`, `c33_ri_upload_bytes`, `c33_ri_mismatch`,
//! `c33_ri_verified`, `c33_ri_refused_currency`, `c33_ri_absent`,
//! `c33_ri_inserted`, `c33_ri_evicted`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::model::ComputeStorageResidencyKey;

/// The retained image a binding wants, at the version it was staged against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SampledMemoTag {
    pub key: ComputeStorageResidencyKey,
    pub version: u64,
}

/// What the runtime decided for one eligible binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampledMemoUse {
    /// Bind the retained image; no bytes were read.
    Hit(SampledMemoTag),
    /// Bytes were read; upload them and retain the image under this tag.
    Remember(SampledMemoTag),
}

impl SampledMemoUse {
    pub fn tag(&self) -> SampledMemoTag {
        match self {
            Self::Hit(t) | Self::Remember(t) => *t,
        }
    }
}

fn env_on(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| v.trim() == "1")
}

/// `REIMS_VGPU_COMPUTE_RESIDENT_INPUTS=1`.
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| env_on("REIMS_VGPU_COMPUTE_RESIDENT_INPUTS"))
}

/// One in how many would-be hits is verified, or 0 when verify is off.
pub fn verify_every() -> u64 {
    static N: OnceLock<u64> = OnceLock::new();
    *N.get_or_init(|| {
        if !env_on("REIMS_VGPU_COMPUTE_RESIDENT_VERIFY") {
            return 0;
        }
        std::env::var("REIMS_VGPU_COMPUTE_RESIDENT_VERIFY_EVERY")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(16)
    })
}

/// Retained bytes cap, `REIMS_VGPU_COMPUTE_RESIDENT_INPUTS_MB` (default 512).
pub fn cap_bytes() -> u64 {
    static B: OnceLock<u64> = OnceLock::new();
    *B.get_or_init(|| {
        std::env::var("REIMS_VGPU_COMPUTE_RESIDENT_INPUTS_MB")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(512)
            .saturating_mul(1 << 20)
    })
}

/// An entry asked for or used this recently is never evicted, so a request
/// staged against it cannot find it gone.
pub const EVICT_MIN_AGE_MS: u64 = 2_000;

pub fn now_ms() -> u64 {
    static T0: OnceLock<std::time::Instant> = OnceLock::new();
    T0.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64
}

/// Fold the version parts into one token.
pub fn version_token(parts: &[u64]) -> u64 {
    use std::hash::Hasher as _;
    let mut h = reims_vgpu_core::fasthash::FxHasher::default();
    for p in parts {
        h.write_u64(*p);
    }
    // 0 is reserved for "no version".
    h.finish().max(1)
}

pub fn hash_bytes(bytes: &[u8]) -> u64 {
    use std::hash::Hasher as _;
    let mut h = reims_vgpu_core::fasthash::FxHasher::default();
    h.write_usize(bytes.len());
    h.write(bytes);
    h.finish()
}

/// Inputs to the per-binding decision, all already read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecisionInput {
    pub enabled: bool,
    pub is_storage: bool,
    pub mip_levels: u32,
    /// Another source (storage resident, persistent GPU target) already serves it.
    pub other_source: bool,
    /// `surface_currency(..).serves(WatchedAndUnwritten)` right now.
    pub watched_clean: bool,
    /// The committed entry's version for this key, if any.
    pub retained_version: Option<u64>,
    /// The version this key was staged at the previous time it was asked.
    /// A copy is only retained once its version has been seen twice, so a
    /// window that changes every frame does not allocate an image per frame.
    pub previous_version: Option<u64>,
    pub version: u64,
    /// One in N would-be hits re-reads (0 = never).
    pub verify_every: u64,
    /// Monotonic would-be-hit counter, for the 1-in-N pick.
    pub hit_tick: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Not eligible: stage bytes, retain nothing (the baseline path).
    Bytes,
    /// Eligible but the witness cannot vouch: stage bytes, retain nothing.
    RefusedCurrency,
    /// Eligible, but this version is new: stage bytes, retain nothing yet.
    FirstSight,
    /// Eligible, no current copy, version settled: stage bytes and retain them.
    Remember,
    /// Bind the retained copy without reading.
    Hit,
    /// Would hit; read the bytes anyway and compare hashes.
    VerifyHit,
}

pub fn decide(i: DecisionInput) -> Decision {
    if !i.enabled || i.is_storage || i.mip_levels != 1 || i.other_source || i.version == 0 {
        return Decision::Bytes;
    }
    if !i.watched_clean {
        return Decision::RefusedCurrency;
    }
    if i.retained_version != Some(i.version) {
        if i.previous_version != Some(i.version) {
            return Decision::FirstSight;
        }
        return Decision::Remember;
    }
    if i.verify_every != 0 && i.hit_tick % i.verify_every == 0 {
        return Decision::VerifyHit;
    }
    Decision::Hit
}

/// Whether a retained entry may be evicted now.
pub fn evictable(last_use_ms: u64, last_query_ms: u64, now_ms: u64, used_this_request: bool) -> bool {
    !used_this_request && now_ms.saturating_sub(last_use_ms.max(last_query_ms)) >= EVICT_MIN_AGE_MS
}

#[derive(Clone, Copy, Debug)]
struct IndexEntry {
    version: u64,
    hash: u64,
    last_query_ms: u64,
}

/// What committed entries the engine holds, readable without the engine lock.
fn index() -> &'static Mutex<HashMap<ComputeStorageResidencyKey, IndexEntry>> {
    static I: OnceLock<Mutex<HashMap<ComputeStorageResidencyKey, IndexEntry>>> = OnceLock::new();
    I.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The committed version and upload hash for `key`, marking it asked-for.
pub fn lookup(key: &ComputeStorageResidencyKey) -> Option<(u64, u64)> {
    let mut g = index().lock().ok()?;
    let e = g.get_mut(key)?;
    e.last_query_ms = now_ms();
    Some((e.version, e.hash))
}

pub(crate) fn last_query_ms(key: &ComputeStorageResidencyKey) -> u64 {
    index()
        .lock()
        .ok()
        .and_then(|g| g.get(key).map(|e| e.last_query_ms))
        .unwrap_or(0)
}

pub(crate) fn publish(key: ComputeStorageResidencyKey, version: u64, hash: u64) {
    if let Ok(mut g) = index().lock() {
        g.insert(
            key,
            IndexEntry {
                version,
                hash,
                last_query_ms: now_ms(),
            },
        );
    }
}

pub(crate) fn unpublish(key: &ComputeStorageResidencyKey) {
    if let Ok(mut g) = index().lock() {
        g.remove(key);
    }
    shadow_drop(key);
}

pub(crate) fn unpublish_all() {
    if let Ok(mut g) = index().lock() {
        g.clear();
    }
}

/// Record `version` as the latest staged for `key`, returning the previous one.
pub fn observe_version(key: ComputeStorageResidencyKey, version: u64) -> Option<u64> {
    static SEEN: OnceLock<Mutex<HashMap<ComputeStorageResidencyKey, u64>>> = OnceLock::new();
    let mut g = SEEN.get_or_init(|| Mutex::new(HashMap::new())).lock().ok()?;
    if g.len() > 8192 {
        g.clear();
    }
    g.insert(key, version)
}

pub(crate) fn note(route: &'static str, n: u64) {
    if n != 0 {
        crate::runtime::drain::note_store_route_n(route, n);
    }
}

// ---------------------------------------------------------------------------
// Candidate 34: content-keyed resident inputs.
//
// `REIMS_VGPU_COMPUTE_RESIDENT_KEY=content` (with `RESIDENT_INPUTS=1`) keys the
// retained copy on the residency key (mapping/ref window, bpr, extent, format;
// single-level) plus a hash of the guest bytes this binding already reads. The
// bytes are always read; only the upload is skipped when the hash equals the
// committed copy's. Correct by construction up to hash collision, which
// `VERIFY` guards with a 1-in-N full byte compare against a CPU shadow.
//
// Counters: `c34_content_hits`, `c34_content_misses`, `c34_hash_us`,
// `c34_bytes_skipped`, `c34_content_verified`, `c34_content_mismatch`,
// `c34_content_verify_no_shadow`; version-token diagnosis `c34_vdiff_<field>`.
// ---------------------------------------------------------------------------

/// `REIMS_VGPU_COMPUTE_RESIDENT_KEY=content`, only meaningful with [`enabled`].
pub fn content_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        enabled()
            && std::env::var("REIMS_VGPU_COMPUTE_RESIDENT_KEY")
                .is_ok_and(|v| v.trim().eq_ignore_ascii_case("content"))
    })
}

/// The content version for a byte hash (0 stays reserved for "no version").
pub fn content_version(hash: u64) -> u64 {
    hash.max(1)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContentInput {
    /// Switch on, sampled, single-level, no other source claimed it.
    pub eligible: bool,
    pub retained_version: Option<u64>,
    pub version: u64,
    pub verify_every: u64,
    pub hit_tick: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentDecision {
    /// Not eligible: the baseline byte path, nothing retained.
    Bytes,
    /// Hash differs from (or no) committed copy: upload and retain.
    Miss,
    /// Hash matches: skip the upload, bind the retained copy.
    Hit,
    /// Hash matches and this is the 1-in-N pick: full-compare first.
    VerifyHit,
}

pub fn decide_content(i: ContentInput) -> ContentDecision {
    if !i.eligible || i.version == 0 {
        return ContentDecision::Bytes;
    }
    if i.retained_version != Some(i.version) {
        return ContentDecision::Miss;
    }
    if i.verify_every != 0 && i.hit_tick % i.verify_every == 0 {
        return ContentDecision::VerifyHit;
    }
    ContentDecision::Hit
}

/// LRU victims to bring `total` under `cap`. Each candidate is
/// `(key, bytes, last_use_ms, last_query_ms, used_this_request)`; entries that
/// are not [`evictable`] are never picked. Oldest `last_use_ms` goes first.
pub fn pick_victims<K: Copy>(
    candidates: &[(K, u64, u64, u64, bool)],
    total: u64,
    cap: u64,
    now_ms: u64,
) -> Vec<K> {
    if total <= cap {
        return Vec::new();
    }
    let mut order: Vec<&(K, u64, u64, u64, bool)> = candidates
        .iter()
        .filter(|c| evictable(c.2, c.3, now_ms, c.4))
        .collect();
    order.sort_by_key(|c| c.2);
    let mut left = total;
    let mut out = Vec::new();
    for c in order {
        if left <= cap {
            break;
        }
        left = left.saturating_sub(c.1);
        out.push(c.0);
    }
    out
}

/// CPU shadows of retained bytes, kept only while VERIFY is on, for the
/// content mode's full compare. Bounded by [`cap_bytes`]; dropped wholesale
/// when it would overflow (a later verify then counts `no_shadow`).
fn shadows() -> &'static Mutex<(HashMap<ComputeStorageResidencyKey, (u64, std::sync::Arc<Vec<u8>>)>, u64)> {
    static S: OnceLock<Mutex<(HashMap<ComputeStorageResidencyKey, (u64, std::sync::Arc<Vec<u8>>)>, u64)>> =
        OnceLock::new();
    S.get_or_init(|| Mutex::new((HashMap::new(), 0)))
}

pub fn shadow_store(key: ComputeStorageResidencyKey, version: u64, bytes: &[u8]) {
    if verify_every() == 0 {
        return;
    }
    let len = bytes.len() as u64;
    let cap = cap_bytes();
    if len > cap {
        return;
    }
    if let Ok(mut g) = shadows().lock() {
        if let Some((_, old)) = g.0.remove(&key) {
            g.1 = g.1.saturating_sub(old.len() as u64);
        }
        if g.1.saturating_add(len) > cap {
            g.0.clear();
            g.1 = 0;
        }
        g.0.insert(key, (version, std::sync::Arc::new(bytes.to_vec())));
        g.1 += len;
    }
}

/// `Some(equal)` when a shadow exists at `version`, else `None`.
pub fn shadow_compare(key: &ComputeStorageResidencyKey, version: u64, bytes: &[u8]) -> Option<bool> {
    let g = shadows().lock().ok()?;
    let (v, stored) = g.0.get(key)?;
    (*v == version).then(|| full_compare(stored, bytes))
}

pub(crate) fn shadow_drop(key: &ComputeStorageResidencyKey) {
    if let Ok(mut g) = shadows().lock() {
        if let Some((_, old)) = g.0.remove(key) {
            g.1 = g.1.saturating_sub(old.len() as u64);
        }
    }
}

pub fn full_compare(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a == b
}

/// The C33 version-token parts, in `version_token` order.
pub const VERSION_FIELDS: [&str; 7] = [
    "content_generation",
    "surface_content_epoch",
    "map_generation",
    "guest_write_token",
    "guest_write_token_gen",
    "guest_write_gen_at_store",
    "guest_stated_gen",
];

/// Bit `i` set when part `i` differs.
pub fn diff_parts(prev: &[u64; 7], now: &[u64; 7]) -> u8 {
    let mut m = 0u8;
    for i in 0..7 {
        if prev[i] != now[i] {
            m |= 1 << i;
        }
    }
    m
}

/// How many distinct keys get a one-shot `c34_version_diff` line,
/// `REIMS_VGPU_COMPUTE_RESIDENT_DIFF_N` (default 32).
pub fn diff_log_limit() -> usize {
    static N: OnceLock<usize> = OnceLock::new();
    *N.get_or_init(|| {
        std::env::var("REIMS_VGPU_COMPUTE_RESIDENT_DIFF_N")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(32)
    })
}

/// Record `parts` for `key`; returns the previous parts and whether this key
/// should get its one-shot log line (first change seen, under the limit).
pub fn observe_parts(key: ComputeStorageResidencyKey, parts: [u64; 7]) -> Option<([u64; 7], bool)> {
    struct Seen {
        last: HashMap<ComputeStorageResidencyKey, [u64; 7]>,
        logged: std::collections::HashSet<ComputeStorageResidencyKey>,
    }
    static S: OnceLock<Mutex<Seen>> = OnceLock::new();
    let mut g = S
        .get_or_init(|| {
            Mutex::new(Seen {
                last: HashMap::new(),
                logged: std::collections::HashSet::new(),
            })
        })
        .lock()
        .ok()?;
    if g.last.len() > 8192 {
        g.last.clear();
    }
    let prev = g.last.insert(key, parts)?;
    if prev == parts {
        return Some((prev, false));
    }
    let log = g.logged.len() < diff_log_limit() && g.logged.insert(key);
    Some((prev, log))
}

/// `name:a->b,...` for the changed parts.
pub fn describe_diff(prev: &[u64; 7], now: &[u64; 7]) -> String {
    let m = diff_parts(prev, now);
    (0..7)
        .filter(|i| m & (1 << i) != 0)
        .map(|i| format!("{}:{:#x}->{:#x}", VERSION_FIELDS[i], prev[i], now[i]))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod c34_tests {
    use super::*;

    fn ci() -> ContentInput {
        ContentInput {
            eligible: true,
            retained_version: Some(content_version(42)),
            version: content_version(42),
            verify_every: 0,
            hit_tick: 1,
        }
    }

    #[test]
    fn content_hit_when_hash_matches_the_committed_copy() {
        assert_eq!(decide_content(ci()), ContentDecision::Hit);
    }

    #[test]
    fn content_miss_when_hash_differs_or_nothing_retained() {
        assert_eq!(decide_content(ContentInput { version: 43, ..ci() }), ContentDecision::Miss);
        assert_eq!(decide_content(ContentInput { retained_version: None, ..ci() }), ContentDecision::Miss);
    }

    #[test]
    fn content_ineligible_or_unversioned_is_bytes() {
        assert_eq!(decide_content(ContentInput { eligible: false, ..ci() }), ContentDecision::Bytes);
        assert_eq!(decide_content(ContentInput { version: 0, ..ci() }), ContentDecision::Bytes);
    }

    #[test]
    fn content_verify_picks_one_in_n_hits_only() {
        let v = ContentInput { verify_every: 4, ..ci() };
        assert_eq!(decide_content(ContentInput { hit_tick: 8, ..v }), ContentDecision::VerifyHit);
        assert_eq!(decide_content(ContentInput { hit_tick: 9, ..v }), ContentDecision::Hit);
        // A miss is never "verified": it uploads anyway.
        assert_eq!(decide_content(ContentInput { hit_tick: 8, version: 7, ..v }), ContentDecision::Miss);
    }

    #[test]
    fn content_version_never_zero_and_tracks_bytes() {
        assert_eq!(content_version(0), 1);
        let a = content_version(hash_bytes(&[1, 2, 3, 4]));
        let b = content_version(hash_bytes(&[1, 2, 3, 5]));
        assert_ne!(a, b);
        assert_eq!(a, content_version(hash_bytes(&[1, 2, 3, 4])));
        // Length is part of the hash: a zero tail is not the same content.
        assert_ne!(hash_bytes(&[0; 8]), hash_bytes(&[0; 16]));
    }

    #[test]
    fn full_compare_catches_same_length_difference() {
        assert!(full_compare(&[1, 2, 3], &[1, 2, 3]));
        assert!(!full_compare(&[1, 2, 3], &[1, 2, 4]));
        assert!(!full_compare(&[1, 2, 3], &[1, 2]));
    }

    #[test]
    fn evict_lru_until_under_cap_skipping_young_and_touched() {
        let now = 100_000;
        let old = now - 10_000;
        let c = [
            ('a', 40, old + 3, 0, false),
            ('b', 40, old + 1, 0, false),  // oldest
            ('c', 40, old + 2, 0, true),   // used this request
            ('d', 40, now - 100, 0, false), // too young
            ('e', 40, old + 4, now - 10, false), // queried just now
        ];
        assert_eq!(pick_victims(&c, 200, 120, now), vec!['b', 'a']);
        assert!(pick_victims(&c, 120, 120, now).is_empty());
        // Cannot reach the cap: evicts every evictable entry and stops.
        assert_eq!(pick_victims(&c, 400, 0, now), vec!['b', 'a']);
    }

    #[test]
    fn version_diff_names_the_changed_fields() {
        let a = [1, 2, 3, 4, 5, 6, 7];
        let mut b = a;
        b[2] = 9;
        b[6] = 8;
        assert_eq!(diff_parts(&a, &b), 0b100_0100);
        assert_eq!(
            describe_diff(&a, &b),
            "map_generation:0x3->0x9,guest_stated_gen:0x7->0x8"
        );
        assert_eq!(diff_parts(&a, &a), 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> DecisionInput {
        DecisionInput {
            enabled: true,
            is_storage: false,
            mip_levels: 1,
            other_source: false,
            watched_clean: true,
            retained_version: Some(7),
            previous_version: Some(7),
            version: 7,
            verify_every: 0,
            hit_tick: 1,
        }
    }

    #[test]
    fn switch_off_is_always_bytes() {
        assert_eq!(decide(DecisionInput { enabled: false, ..base() }), Decision::Bytes);
    }

    #[test]
    fn storage_pyramid_and_claimed_bindings_stay_on_bytes() {
        assert_eq!(decide(DecisionInput { is_storage: true, ..base() }), Decision::Bytes);
        assert_eq!(decide(DecisionInput { mip_levels: 2, ..base() }), Decision::Bytes);
        assert_eq!(decide(DecisionInput { other_source: true, ..base() }), Decision::Bytes);
        assert_eq!(decide(DecisionInput { version: 0, ..base() }), Decision::Bytes);
    }

    #[test]
    fn an_unvouched_window_never_hits_even_at_the_same_version() {
        assert_eq!(
            decide(DecisionInput { watched_clean: false, ..base() }),
            Decision::RefusedCurrency
        );
    }

    #[test]
    fn a_version_change_or_absent_copy_re_uploads_and_retains_once_settled() {
        // New version, first sight: bytes, nothing retained.
        assert_eq!(decide(DecisionInput { version: 8, ..base() }), Decision::FirstSight);
        // Same new version seen again: retain.
        assert_eq!(
            decide(DecisionInput { version: 8, previous_version: Some(8), ..base() }),
            Decision::Remember
        );
        assert_eq!(
            decide(DecisionInput { retained_version: None, ..base() }),
            Decision::Remember
        );
        assert_eq!(
            decide(DecisionInput { retained_version: None, previous_version: None, ..base() }),
            Decision::FirstSight
        );
    }

    #[test]
    fn a_retained_copy_at_a_stale_version_never_hits() {
        for previous in [None, Some(7), Some(8)] {
            let d = decide(DecisionInput {
                retained_version: Some(7),
                version: 8,
                previous_version: previous,
                ..base()
            });
            assert_ne!(d, Decision::Hit);
            assert_ne!(d, Decision::VerifyHit);
        }
    }

    #[test]
    fn observe_version_returns_the_previous_staging() {
        let key = ComputeStorageResidencyKey {
            mapping_id: 0xC33,
            map_generation: 1,
            surface_offset: 0,
            surface_bpr: 16,
            span_end: 64,
            width: 4,
            height: 4,
            pixel_format: 80,
            texture_ref: 0,
        };
        assert_eq!(observe_version(key, 5), None);
        assert_eq!(observe_version(key, 5), Some(5));
        assert_eq!(observe_version(key, 6), Some(5));
    }

    #[test]
    fn same_version_and_clean_hits() {
        assert_eq!(decide(base()), Decision::Hit);
    }

    #[test]
    fn verify_picks_one_in_n() {
        let picks = (0..64)
            .filter(|t| {
                decide(DecisionInput { verify_every: 16, hit_tick: *t, ..base() })
                    == Decision::VerifyHit
            })
            .count();
        assert_eq!(picks, 4);
    }

    #[test]
    fn version_token_moves_with_every_part_and_is_never_zero() {
        let a = version_token(&[1, 2, 3, 4, 5, 6]);
        for i in 0..6 {
            let mut p = [1u64, 2, 3, 4, 5, 6];
            p[i] += 1;
            assert_ne!(version_token(&p), a, "part {i}");
        }
        assert_ne!(version_token(&[]), 0);
    }

    #[test]
    fn hash_distinguishes_one_byte() {
        let a = vec![0u8; 4096];
        let mut b = a.clone();
        b[4000] = 1;
        assert_ne!(hash_bytes(&a), hash_bytes(&b));
        assert_eq!(hash_bytes(&a), hash_bytes(&a.clone()));
    }

    #[test]
    fn eviction_respects_age_and_the_current_request() {
        assert!(!evictable(0, 0, 1_999, false));
        assert!(evictable(0, 0, 2_000, false));
        assert!(!evictable(0, 0, 10_000, true));
        assert!(!evictable(0, 9_000, 10_000, false));
    }
}
