# Start-1017-IconStack.ps1 - copy of Start-1017-Icons-Step2.ps1 for qemu-system-x86_64-1017-iconstack.exe (8f454f3d + C-line icon stack minus C33-C35 perf; vendor/qemu bad588cd26; frozen translator). boot-1017-iconstack.sh, logs 1017iconstack-*, same disks as the 1017 rebuild.
#   Default = minimal icon profile (exact C32 values). -Full adds DISPATCH_MEMO=1, DIRTY_BATCH=1, PIPELINE_CACHE_UNBOUNDED=on (DIRTY_NARROW/IOSURFACE_WP_FALLBACK never set: they need the vendor/qemu f26b7c8f bump, not in this exe). -OnDispatchMemo / -OnDirtyBatch / -OnPipelineCacheUnbounded add one each. -NoColour sets M2V_NATURAL_AGG_ALIGN=0. No capture/census switches. Written 2026-10-27 by Grok Bot.
# Start-1017-Icons-Step2.ps1 - copy of Start-1017-Icons-Step1.ps1 for qemu-system-x86_64-1017-cursor-icons2.exe (step1 + step2 sRGB sampled). ICONS_SRGB_STORAGE, ICONS_SRGB_SAMPLED, M2V_NATURAL_AGG_ALIGN = 1 by default; -NoIconStorage / -NoIconSampled / -NoColour set each to 0. boot-1017-icons-step2.sh, logs 1017iconstack-*. Written 2026-10-27 by Grok Bot.
# Start-1017-Icons-Step1.ps1 - copy of Start-1017-Icons.ps1 for step 1 (sRGB compute storage, rebuild source + step1-srgb-storage.patch, frozen translator metal2vulkan-1017frozen). REIMS_VGPU_ICONS_SRGB_STORAGE=1 and REIMS_VGPU_M2V_NATURAL_AGG_ALIGN=1 by default; -NoIconStorage / -NoColour set each to 0. boot-1017-icons-step2.sh, logs 1017iconstack-*. Written 2026-10-27 by Grok Bot.
# Start-1017-Icons.ps1 - copy of Start-1017-Cursor-Rebuild.ps1 for qemu-system-x86_64-1017-cursor-icons2.exe (cursor rebuild source 2ada8816/bad588cd26 + metal2vulkan f758b86 with ONLY d49e399 natural aggregate alignment), REIMS_VGPU_M2V_NATURAL_AGG_ALIGN=1, boot-1017-icons.sh, logs 1017iconstack-*. Written 2026-10-27 by Grok Bot.
# Start-1017-Cursor-Rebuild.ps1 - copy of Start-1017-Cursor.ps1 for the REBUILT exe qemu-system-x86_64-1017-cursor-icons2.exe (superproject 2ada8816, vendor/qemu bad588cd26), boot-1017-cursor-rebuild.sh, logs 1017cursorrebuild-*. Written 2026-10-27 by Grok Bot.
# Start-1017-Cursor.ps1 - copy of Start-1017-StopFence-Drag.ps1 for qemu-system-x86_64-1017-cursor-icons2.exe, boot-1017-cursor.sh, logs 1017cursor-*, REIMS_VGPU_LAB_CURSOR_LOCKFREE=on, REIMS_VGPU_LAB_CURSOR_EPOCH_SKIP unset. Written 2026-10-19 by Grok Bot.
# Start-1017-StopFence-Drag.ps1 - copy of Start-1017-StopFence.ps1 with REIMS_VGPU_LAB_WIN32_DRAG_HOOK=on, boot-1017-stopfence-drag.sh, logs 1017stopfencedrag-*. Written 2026-10-19 by Grok Bot.
# Start-1017-StopFence.ps1 - copy of Start-1017-Fmaf-FmDefaults-Signed-UsbExtra.ps1 for qemu-system-x86_64-1017-fmaf-stopfence.exe, QEMU_XHCI_ISO_STOP_FENCE=1 + QEMU_XHCI_ISO_DIAG=1, logs 1017stopfence-*. Written 2026-10-19 by Grok Bot.
# Start-1017-Fmaf-FmDefaults-Signed-UsbExtra.ps1 - same as FmDefaults-Signed but MacExtra back on USB (xhci.0 port 3) like boot-tahoe.sh, not SATA. Written 2026-10-19 by Grok Bot.
# Start-1017-Fmaf-FmDefaults-Signed.ps1 - Full Metal defaults (XHCI NOBQL/IMOD on) + QEMU_XHCI_ISO_SIGNED=1 only. Written 2026-10-19 by Grok Bot.
# Start-1017-Fmaf-Dirty-Off.ps1 - 1017 fmaf + iso-signed + resync + stats unlocked + prefill 25 + video switches + Win32 drag hook=on, REIMS_VGPU_LAB_DIRTY_COMBO=off, for qemu-system-x86_64-1017-fmaf-dirty.exe (reims-vgpu 2fd9f6bf). Written 2026-10-18 by Grok Bot.
# Start-1017-Fmaf-Isv-Underrun2.ps1 - copy of Start-1017-Fmaf-Isv-Underrun.ps1 for qemu-system-x86_64-1017-fmaf-dirty.exe (vendor/qemu 68458c10, underrun once v2). Written 2026-10-18 by Grok Bot.
# Start-1017-Fmaf-Isv-Underrun.ps1 - copy of Start-1017-Fmaf-IsoSigned-Video.ps1 for qemu-system-x86_64-1017-fmaf-dirty.exe
#   (+ QEMU_XHCI_ISO_UNDERRUN_ONCE=1, vendor/qemu 68458c10 (underrun once v2)). Written 2026-10-18 by Grok Bot.
# Start-1017-Fmaf-IsoSigned-Video.ps1 - copy of Start-1017-FullMetal-Audio-Fps-Cushion.ps1 for qemu-system-x86_64-1017-fmaf-dirty.exe
#   (+ QEMU_XHCI_ISO_SIGNED=1 and REIMS_VGPU_LAB_VIDEO_GATE / RELEASE_GUARDS / FRONTBUFFER_LATCH = on). Written 2026-10-18 by Grok Bot.
# Base: boots Tahoe on the 1017-fullmetal-audio-fps build (= 1017-fullmetal + vendor/qemu 5f631597 xHCI iso ring resync
#   + df847199 dirty stats block off d->lock). Only switches vs Start-1017-FullMetal.ps1: QEMU_XHCI_ISO_RESYNC=1, REIMS_VGPU_LAB_DIRTY_STATS_UNLOCKED=on
#   (the QEMU code only accepts the literal value 'on'; '1' would leave it off).
#   MAX's reims-vgpu windows-2026-10-17 (3d9f37c5, vendor/qemu 9106e659, metal2vulkan f758b86)
#   + Nour's Full Metal commits only (branch nour/1017-fullmetal in D:\reims-lab\reims-1017).
# Modelled on Start-Upstream-Combo4b.ps1 / Combo2.ps1, but WITHOUT the lab env switches:
#   no QEMU_XHCI_ISO_*, no QEMU_ISO_HEARTBEAT, no REIMS_VGPU_LAB_* (incl. LAB_DIRECT_REDRAW, LAB_DIRTY_HASH,
#   LAB_LIVE_FILE, LAB_DIRTY_STATS_UNLOCKED), no combo icon switches. Any of those inherited from the calling
#   shell are removed before launch. Same VM disks / OVMF / devices / SDL audio via boot-1017-fullmetal.sh
#   (a copy of boot-tahoe.sh minus its two REIMS_VGPU_LAB_* exports). Written 2026-10-18 by Grok Bot.
# Does NOT modify boot-tahoe.sh, Start-Tahoe.*, or any existing exe.
param([ValidateSet('install', 'boot')][string]$Mode = 'boot', [switch]$Full, [switch]$OnDispatchMemo, [switch]$OnDirtyBatch, [switch]$OnPipelineCacheUnbounded, [switch]$NoColour)
$ErrorActionPreference = 'Stop'
Set-Location 'C:\ReimsVM\Tahoe'   # QEMU inherits this cwd: the vGPU log's drive-relative /tmp must be C:\tmp
$env:PATH = "C:\msys64\ucrt64\bin;C:\msys64\usr\bin;$env:PATH"

$vmRoot = 'C:\ReimsVM\Tahoe'
$candidate = Join-Path $vmRoot 'qemu-system-x86_64-1017-iconstack.exe'
$bootScript = Join-Path $vmRoot 'boot-1017-iconstack.sh'
if (-not (Test-Path -LiteralPath $candidate)) { throw "Missing 1017-fullmetal executable: $candidate" }
if (-not (Test-Path -LiteralPath $bootScript)) { throw "Missing boot script: $bootScript" }
$hashFile = "$candidate.sha256"   # written by Build-1017-IconStack.ps1 after the build
if (-not (Test-Path -LiteralPath $hashFile)) { throw "Missing $hashFile; the icons exe has not been built/recorded yet." }
$expectedHash = (Get-Content -LiteralPath $hashFile -TotalCount 1).Trim().ToLower()
if ((Get-FileHash -LiteralPath $candidate -Algorithm SHA256).Hash.ToLower() -ne $expectedHash) { throw 'Icons exe hash does not match its recorded build.' }

# Refuse duplicate sessions
$vmProcesses = @(Get-Process -Name 'qemu*' -ErrorAction SilentlyContinue)
if ($vmProcesses.Count -gt 0) {
    throw 'A QEMU instance is already running. Cleanly shut down Tahoe and wait for exit before launching.'
}
$qmpSocket = [System.Net.Sockets.TcpClient]::new()
$portBusy = $false
try { $qmpSocket.Connect('127.0.0.1', 4444); $portBusy = $true } catch { } finally { $qmpSocket.Dispose() }
if ($portBusy) { throw 'QMP port 4444 is still occupied; refusing to launch.' }

# Per-session logs: QEMU stderr to its own file, and a fresh reims-vgpu-fail.log
$logRoot = Join-Path $vmRoot 'logs'
New-Item -ItemType Directory -Force -Path $logRoot | Out-Null
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$stderrLog = Join-Path $logRoot "1017iconstack-stderr-$stamp.log"
$stdoutLog = Join-Path $logRoot "1017iconstack-stdout-$stamp.log"
$failLog = 'C:\tmp\reims-vgpu-fail.log'
if (Test-Path -LiteralPath $failLog) {
    Copy-Item -LiteralPath $failLog -Destination (Join-Path $logRoot "reims-vgpu-fail-before-1017iconstack-$stamp.log") -Force
    Clear-Content -LiteralPath $failLog
}

# Strip every lab / combo switch that could be inherited from the calling shell.
$strip = @('QEMU_ISO_HEARTBEAT', 'REIMS_QEMU_BINARY', 'REIMS_VGPU_DIRTY_CURSOR_EPOCH', 'REIMS_VGPU_AIR_CAPTURE',
    'REIMS_VGPU_NO_FRAGMENT_STUB', 'REIMS_VGPU_MRT_SECONDARY_COLD_CLEAR', 'REIMS_VGPU_SEED_NATIVE_TAG',
    'REIMS_VGPU_COMPUTE_WRITEONLY_SEED_SKIP', 'REIMS_VGPU_ALIAS_CLEAR_NATIVE', 'REIMS_VGPU_LOAD_RESIDENT_FALLBACK',
    'REIMS_VGPU_PIPELINE_CACHE_PERSIST', 'REIMS_VGPU_PIPELINE_CACHE_UNBOUNDED', 'REIMS_VGPU_INCARNATION_WRITES', 'REIMS_VGPU_UNBOUND_BUFFER_ZERO',
    'REIMS_VGPU_ICONS_SRGB_STORAGE', 'REIMS_VGPU_ICONS_SRGB_SAMPLED', 'REIMS_VGPU_COMPUTE_RESIDENT_INPUTS', 'REIMS_VGPU_COMPUTE_RESIDENT_VERIFY')
Get-ChildItem Env: | Where-Object { $_.Name -like 'REIMS_VGPU_LAB_*' -or $_.Name -like 'QEMU_XHCI_ISO_*' -or $strip -contains $_.Name } |
    ForEach-Object { Remove-Item -LiteralPath ("Env:" + $_.Name) -ErrorAction SilentlyContinue }
Remove-Item Env:QEMU_XHCI_ISO_RESYNC -ErrorAction SilentlyContinue   # off, like original Full Metal
Remove-Item Env:REIMS_VGPU_LAB_DIRTY_STATS_UNLOCKED -ErrorAction SilentlyContinue   # off, like original Full Metal
$env:QEMU_XHCI_ISO_SIGNED = '1'   # signed iso ON
$env:QEMU_XHCI_ISO_STOP_FENCE = '1'   # set AFTER the strip loop (it removes QEMU_XHCI_ISO_*); the .sh exports it too
$env:QEMU_XHCI_ISO_DIAG = '1'   # set AFTER the strip loop; the .sh exports it too
Remove-Item Env:REIMS_VGPU_LAB_VIDEO_GATE -ErrorAction SilentlyContinue   # off, like original Full Metal
Remove-Item Env:REIMS_VGPU_LAB_RELEASE_GUARDS -ErrorAction SilentlyContinue   # off, like original Full Metal
Remove-Item Env:REIMS_VGPU_LAB_FRONTBUFFER_LATCH -ErrorAction SilentlyContinue   # off, like original Full Metal
# QEMU_XHCI_ISO_UNDERRUN_ONCE deliberately not set (off; it did not help)
$env:REIMS_VGPU_LAB_DIRTY_COMBO = 'off'   # master switch (vendor/qemu 64ea9928+48848b20): dirty hash off, WP_HASHED Intel-only
$env:REIMS_VGPU_LAB_WIN32_DRAG_HOOK = 'on'   # set AFTER the strip loop; the .sh exports it too
$env:REIMS_VGPU_LAB_CURSOR_LOCKFREE = 'on'   # issue #3 Step B (Rust lock-free 0x1220); set AFTER the strip loop; the .sh exports it too
$env:REIMS_VGPU_M2V_NATURAL_AGG_ALIGN = $(if ($NoColour) { '0' } else { '1' })   # icon colour fix; set AFTER the strip loop; the .sh passes it through
# Icon stack, minimal profile: exact C32 values (boot-candidate-21.sh + Start-Candidate-32 + boot-candidate-32). Set AFTER the strip loop; the .sh inherits them.
$env:REIMS_VGPU_LAB_IOSURFACE_REGISTRY_SHADOW = 'on'
$env:REIMS_VGPU_LAB_IOSURFACE_PERSISTENT_GPU = 'on'
$env:REIMS_VGPU_LAB_IOSURFACE_PERSISTENT_CPU = '1'
$env:REIMS_VGPU_LAB_IOSURFACE_EVENTS = '1'
$env:REIMS_VGPU_LAB_IOSURFACE_SUBRECT_VIEWS = '1'
$env:REIMS_VGPU_LAB_FORMATS_EXT = '1'
$env:REIMS_VGPU_LAB_MIPGEN = '1'
$env:REIMS_VGPU_INCARNATION_WRITES = 'on'
$env:REIMS_VGPU_UNBOUND_BUFFER_ZERO = 'on'
$env:REIMS_VGPU_PIPELINE_CACHE_PERSIST = 'on'
$env:REIMS_VGPU_LAB_BIND_FORMAT_GUARD = 'off'
$env:REIMS_VGPU_LAB_SNAPSHOT_KEEP_RESIDENT_LAYOUT = 'off'
# Per-frame / perf switches C32 also used: only with -Full or the matching -On<Name>.
if ($Full -or $OnDispatchMemo) { $env:REIMS_VGPU_LAB_DISPATCH_MEMO = '1' }
if ($Full -or $OnDirtyBatch) { $env:REIMS_VGPU_LAB_DIRTY_BATCH = '1' }
if ($Full -or $OnPipelineCacheUnbounded) { $env:REIMS_VGPU_PIPELINE_CACHE_UNBOUNDED = 'on' }
# REIMS_VGPU_LAB_DIRTY_NARROW / REIMS_VGPU_LAB_IOSURFACE_WP_FALLBACK: never set; their QEMU side (vendor/qemu f26b7c8f) is not in this exe.
Remove-Item Env:REIMS_VGPU_LAB_CURSOR_EPOCH_SKIP -ErrorAction SilentlyContinue   # Step A deliberately OFF: alone it caused stale menus; needs REIMS_VGPU_LAB_DIRTY_HOLE=tail pairing verified first
$env:QEMU_SDL_OUT_PREFILL_MS = '25'   # same as combo4b (boot-1017-fmaf-fmdefaults-signed-usbextra.sh also exports 25, so this is a no-op safeguard)

Write-Host '==================================================' -ForegroundColor Cyan
Write-Host "  Starting macOS 26 Tahoe (1017 icon stack, profile=$(if ($Full) {'full'} else {'minimal'}) DISPATCH_MEMO=$env:REIMS_VGPU_LAB_DISPATCH_MEMO DIRTY_BATCH=$env:REIMS_VGPU_LAB_DIRTY_BATCH PIPELINE_CACHE_UNBOUNDED=$env:REIMS_VGPU_PIPELINE_CACHE_UNBOUNDED NATURAL_AGG_ALIGN=$env:REIMS_VGPU_M2V_NATURAL_AGG_ALIGN): stop fence + diag, drag hook on, CURSOR_LOCKFREE on, CURSOR_EPOCH_SKIP off)" -ForegroundColor Cyan
Write-Host "  Binary: $candidate" -ForegroundColor Cyan
Write-Host "  stderr: $stderrLog" -ForegroundColor Cyan
Write-Host "  vgpu:   $failLog (previous copy saved in logs\)" -ForegroundColor Cyan
Write-Host '==================================================' -ForegroundColor Cyan

$p = Start-Process -FilePath 'C:\msys64\usr\bin\bash.exe' -WindowStyle Hidden `
    -ArgumentList @('--noprofile', '--norc', '/c/ReimsVM/Tahoe/boot-1017-iconstack.sh', $Mode) `
    -RedirectStandardOutput $stdoutLog -RedirectStandardError $stderrLog -PassThru
Start-Sleep -Seconds 3
if ($p.HasExited) {
    Write-Host '[X] QEMU exited during startup:' -ForegroundColor Red
    Get-Content -LiteralPath $stderrLog -Tail 20 | Write-Host -ForegroundColor DarkRed
    throw "1017-fullmetal exited during startup; see $stderrLog"
}
$q = Get-Process -Name 'qemu-system*' -ErrorAction SilentlyContinue | Select-Object -First 1
if ($q -and $q.Path -ne $candidate) { Write-Host "[!] Running QEMU is $($q.Path), not $candidate" -ForegroundColor Red }
Write-Host "[+] Running (bash PID $($p.Id))." -ForegroundColor Green
$p.WaitForExit()
Write-Host "[i] QEMU exited with code $($p.ExitCode)." -ForegroundColor Yellow
