# Icon fix inventory: 1017 rebuild vs C32 (read-only, 2026-10-27, Cairo time)

## Why step 1 + step 2 showed no icons
The icon fix is not a single patch. In the C-line (C17 to C35), icons were drawn by the **whole IOSurface/artwork lab line**: 35 commits on branch `nour/1017-iosurface-p4-dirtynarrow` plus a large uncommitted tree in `D:\reims-lab\reims-1017`. That line is env-gated by about 18 `REIMS_VGPU_LAB_*` switches. The 1017 rebuild (2ada8816) contains none of that code. Steps 1 and 2 were only two leaf fixes from the top of that stack.

## Ancestry (verified)
- `2ada8816` (1017 rebuild base, 10-20 12:43) is a direct ancestor of `8f454f3d` (C-line HEAD). `2ada8816..8f454f3d` is 35 commits, with nothing on the other side. It is a clean fast-forward: 50 files, +7427/-298.
- The C-line working tree on top of 8f454f3d is uncommitted: 126 modified files (+7973/-2441) plus untracked modules. It represents the **C35** state. C32 was built from an earlier point of the same tree, and there is no snapshot of it.
- `d6f52d11`/`7f71cd60` (master) are only partly ported, through the dirty tree (c669e664, a5056cfd, bdb2e005).

## Switches the C32 run set (C21 boot script + C32 launcher)
None of these strings is in the 1017 `icons2` exe. All of them are in the C32 exe. The 1017 launcher sets none of them.
| Switch (REIMS_VGPU_LAB_*) | Value | Purpose |
|---|---|---|
| IOSURFACE_CENSUS | on (C32: off) | telemetry |
| IOSURFACE_REGISTRY_SHADOW | on | IOSurface state machine |
| IOSURFACE_PERSISTENT_GPU | on | persistent host images (icon atlases) |
| IOSURFACE_PERSISTENT_CPU | 1 | |
| IOSURFACE_EVENTS | 1 | guest lifecycle events 0x41..0x47 |
| IOSURFACE_WP_FALLBACK | 1 | needs a QEMU HostOps bump |
| IOSURFACE_SUBRECT_VIEWS | 1 | atlas sub-rect sampling |
| FORMATS_EXT | 1 | Appendix A formats incl. BGRA10_XR |
| MIPGEN | 1 | mip chains |
| DISPATCH_MEMO | 1 | compute memo (perf) |
| DIRTY_BATCH, DIRTY_NARROW | 1 | needs QEMU (perf) |
| PIPELINE_CACHE_PERSIST, _UNBOUNDED | on | fast artwork loading |
| INCARNATION_WRITES | on | |
| UNBOUND_BUFFER_ZERO | on | |
| BIND_FORMAT_GUARD | off (C32) | |
| SNAPSHOT_KEEP_RESIDENT_LAYOUT | off (C32) | |
| M2V_NATURAL_AGG_ALIGN | 0/1 | translator (already in the frozen m2v) |
| SPV_ALL, UBO/UBO_WINDOW/UBO_MAX | capture only | do not port |
Also: the C-line boots **overlay qcow2s** from the candidate folders, not macos26.qcow2.

## Change groups (committed 2ada8816..8f454f3d)
- **A. IOSurface registry (phases 0-4):** 69e6f468, 8366743d, 8602c141, e7914546, 917e12ef (protocol), f15fddc7, acfd6383 (drain dispatch, hot path), 86187c17 views, 4e8055f7 repack, cc43a833 formats, 4d899cc6 mipgen, d03ad45d config. About 3.5k lines.
- **B. Artwork/draw icon fixes:** c103c5a6 (fast artwork loading, unbounded pipeline cache, incarnation writes; +1752, hot path), a772a72b LOAD_SEED_RESCUE + MSAA_SRGB_RESOLVE (icon filter passes), a2b2cfae nil-fragment stub, 7d0ddcbb, 624a9800, 972dabc9 secondary texture binding via registry (draw hot path), 2b84cd90 R8 glyph masks, 9c82b019 2DArray slices, 8f454f3d squircle stencil loads, 2e20da48/28758d35/ffe621c4 MSAA resolve and stencil, 6c01a00c, cbe8f486.
- **C. Compute:** 2e9b1611 admits 0x51/0x47/0x73/0x5a/0x5e/0x0a sampled formats. This is what step 2 tried to recreate. Also 678c8989, f5f439e7 dispatch memo (perf).
- **D. QEMU/dirty tracking (perf, needs a vendor/qemu bump):** eded4870, plus DIRTY_* switches.
- **E. Writeback:** e304ab66, 27af9825.
- **F. TEMP lab census:** 629f417e, 53fb7014 (skip).

## Uncommitted C-line tree (C35 state) — the largest files
draw/vulkan.rs +1334/-463, engine/exec.rs +752/-257, drain/mod.rs +406/-165, engine/mod.rs +320/-94, compute_exec/mod.rs +285, render_writeback/vulkan.rs +258, exec/mod.rs +238, exec_compute.rs +228, pixel_format.rs +216, texture_view.rs +207, pipeline_cache_persist.rs, pcache_shard.rs, iosurface_registry/{upload,shadow,formats}.rs, spirv_bind.rs.
Untracked: pixel_format/srgb.rs, engine/exec/ (secondary_seed), sampled_mips.rs, compute_exec/vulkan/ (dispatch_memo, srgb_storage), secondary_store.rs, m2v_cache/ (texture_presence), iosurface_registry/budget.rs. **C33-C35 perf (exclude):** sampled_memo.rs, pools/sampled_memo_store.rs, draw_timers.rs, walk_memo/walk_ahead, par/*, lab_cores/lab_clock, cursor and drain timer edits.
Per-frame hot path: draw/vulkan.rs, draw_par.rs, sample_plan.rs, engine/exec.rs, drain/mod.rs, compute_exec dispatch, iosurface upload/shadow.

## Recommended single port plan
1. Make a new worktree at `8f454f3d` (fast-forward from 2ada8816, no conflicts). This brings groups A-C and E, all env-gated, so they are off by default.
2. Overlay the uncommitted C-line tree **minus** the C33-C35 perf files listed above. Because C32 has no snapshot, split the files by name/date first and review them.
3. Keep the frozen m2v. Build with `backend-vulkan,host-window,lab-tcache` into a new exe name.
4. Use a new launcher that sets exactly the C32 switch set (without capture vars). Then A/B the perf-heavy switches (DISPATCH_MEMO, DIRTY_*, PIPELINE_CACHE_UNBOUNDED) one at a time against 1017 FPS.
5. The QEMU-side group D (WP_FALLBACK/DIRTY_NARROW) needs the 1017 vendor/qemu bump, so leave those switches off unless needed.
