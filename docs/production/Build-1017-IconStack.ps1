# Build-1017-IconStack.ps1 - builds qemu-system-x86_64-1017-iconstack.exe from D:\reims-lab\rebuild-1017-iconstack
# (superproject 8f454f3d + C-line dirty tree minus C33-C35 perf/measurement; was 2ada8816, vendor/qemu bad588cd26, Cargo [patch] -> ..\metal2vulkan-1017frozen = frozen copy of shared d0414a2 + 14 dirty files; no step patches).
# Refuses to run while any qemu* process exists. BelowNormal, ninja -j 6, all output on D:. Never overwrites the target exe.
# Written 2026-10-27 by Grok Bot. Icon-stack copy.
$ErrorActionPreference = 'Stop'
if (@(Get-Process qemu* -ErrorAction SilentlyContinue).Count) { throw 'A QEMU process is running; not building.' }
$wt  = 'D:\reims-lab\rebuild-1017-iconstack'
$out = 'D:\reims-lab\rebuild-1017-icons-artifacts'
$dst = 'C:\ReimsVM\Tahoe\qemu-system-x86_64-1017-iconstack.exe'
if (Test-Path -LiteralPath $dst) { throw "Target exists, not overwriting: $dst" }
if (-not (Test-Path -LiteralPath "$wt\vendor\qemu\configure")) { throw 'qemu submodule is not checked out in the worktree.' }
$env:CARGO_TARGET_DIR = "$wt\target"   # explicit: meson copies <repo>\target\release\libreims_vgpu.a. Never inherit a stray value.
$env:CARGO_BUILD_JOBS = '6'; $env:MSYSTEM = 'UCRT64'; $env:CHERE_INVOKING = '1'
$sh = 'export CARGO_TARGET_DIR=D:/reims-lab/rebuild-1017-iconstack/target; ' +
      'mkdir -p vendor/qemu/build && cd vendor/qemu/build && ' +
      '( [ -f build.ninja ] || ../configure --target-list=x86_64-softmmu --disable-hvf --disable-cocoa --disable-docs --disable-bsd-user --disable-linux-user --disable-tools -Dreims_vgpu_backend=vulkan ) && ' +
      'ninja -j 6 qemu-system-x86_64.exe; echo NINJA_EXIT=$?'
$p = Start-Process C:\msys64\usr\bin\bash.exe -ArgumentList '-l', '-c', "`"$sh`"" -WorkingDirectory $wt -WindowStyle Hidden -PassThru `
    -RedirectStandardOutput "$out\buildstack.out" -RedirectStandardError "$out\buildstack.err"
Start-Sleep 20   # let configure/ninja spawn, then lower every child
function Lower { Get-Process ninja, cargo, rustc, cc1, cc, bash, python -ErrorAction SilentlyContinue | ForEach-Object { try { $_.PriorityClass = 'BelowNormal' } catch {} } }
while (-not $p.HasExited) { Lower; Start-Sleep 20 }
if (-not (Select-String -LiteralPath "$out\buildstack.out" -Pattern '^NINJA_EXIT=0' -Quiet)) { throw "Build failed; see $out\buildstack.out / buildstack.err" }
if (@(Get-Process qemu* -ErrorAction SilentlyContinue).Count) { throw 'A QEMU process appeared; not installing the exe.' }
$exe = "$wt\vendor\qemu\build\qemu-system-x86_64.exe"
Copy-Item -LiteralPath $exe -Destination $dst
$h = (Get-FileHash -LiteralPath $dst -Algorithm SHA256).Hash.ToLower()
Set-Content -LiteralPath "$dst.sha256" -Value $h
$s = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($dst))
"sha256 $h size $((Get-Item $dst).Length) natural_agg_align_string=$($s.Contains('REIMS_VGPU_M2V_NATURAL_AGG_ALIGN')) iosurface_shadow_string=$($s.Contains('REIMS_VGPU_LAB_IOSURFACE_REGISTRY_SHADOW'))"
'Next: .\Start-1017-IconStack.ps1 (in C:\ReimsVM\Tahoe) once the VM is shut down.'



