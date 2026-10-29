# rebuild-1017-iconstack notes (2026-10-27, Cairo time)
Worktree: D:\reims-lab\rebuild-1017-iconstack, detached at 8f454f3d (2ada8816 is an ancestor: 35 commits, fast-forward).
Overlay: iconstack-c35-dirty-full.patch (reims-1017 working tree vs HEAD, applied cleanly) + 10 untracked module paths copied. Then exclusions below. Tracked diff now:  118 files changed, 7774 insertions(+), 2376 deletions(-)
Cargo [patch] -> ../metal2vulkan-1017frozen. vendor/qemu = bad588cd26 source copied from rebuild-1017-icons (no .git, no build dir). 8f454f3d itself pins f26b7c8f (dirty batching/narrow only). Not bumped: the Rust side accepts ABI v28 hosts and the C side builds against the v29 header with the new HostOps callbacks left NULL.
## Excluded (C33-C35 perf/measurement)
- runtime/par/{ahead,mod,pool,reap,tests}.rs: reverted to 8f454f3d (C35 diff was rustfmt reflow only)
- runtime/exec/walk_ahead.rs, walk_memo.rs: reverted (mostly reflow; walk_ahead had a small witness change)
- lab_cores.rs, lab_clock.rs: reverted (reflow only)
- runtime/draw_timers.rs: replaced by an inert no-op stub with the same API (call sites in caches/context/engine mod/pipeline_resolve/device/draw/texture_view/census are left in place and do nothing)
## Kept but off by default (woven into icon-path files; cannot be removed without hand surgery)
- engine/sampled_memo.rs + pools/sampled_memo_store.rs (C34): only active with REIMS_VGPU_COMPUTE_RESIDENT_INPUTS=1, which the launcher strips
- compute_exec/vulkan/dispatch_memo.rs (C33 rev): only active with REIMS_VGPU_LAB_DISPATCH_MEMO=1 (-Full / -OnDispatchMemo)
- drain/mod.rs: the C35 diff is reflow plus the draw_timers scope (now a no-op)
## Compile: no errors, warnings only. No source fixes needed.
## Tests (cargo test -p reims-vgpu --no-default-features --features backend-vulkan,host-window,lab-tcache --lib)
- iosurface_registry: 53/53 ok
- draw:: + compute_exec serial: 398 ok / 5 fail; the same 5 pass when run alone (shared global counters between tests). The C35 tree baseline (separate target dir) fails 3 of these in the same serial run.
- Parallel runs: 27 draw + 1 compute fail (same shared-state issue)
