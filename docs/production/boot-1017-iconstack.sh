#!/usr/bin/env bash
# boot-1017-iconstack.sh - copy of boot-1017-icons-step2.sh for qemu-system-x86_64-1017-iconstack.exe (8f454f3d + C-line icon stack, no C33-C35 perf). The icon-stack REIMS_VGPU_* switches come from Start-1017-IconStack.ps1 via the inherited environment; this script does not set or unset them. Step 1/2 ICONS_SRGB_* exports removed. Written 2026-10-27 by Grok Bot.
# boot-1017-icons-step2.sh - copy of boot-1017-icons-step1.sh for qemu-system-x86_64-1017-cursor-icons2.exe; also passes through REIMS_VGPU_ICONS_SRGB_SAMPLED (default 1). Written 2026-10-27 by Grok Bot.
# boot-1017-icons-step1.sh - copy of boot-1017-icons.sh; passes through REIMS_VGPU_ICONS_SRGB_STORAGE and REIMS_VGPU_M2V_NATURAL_AGG_ALIGN from Start-1017-Icons-Step1.ps1 (default 1). Written 2026-10-27 by Grok Bot.
# boot-1017-icons.sh - copy of boot-1017-cursor-rebuild.sh for qemu-system-x86_64-1017-cursor-icons2.exe, exports REIMS_VGPU_M2V_NATURAL_AGG_ALIGN=1. Written 2026-10-27 by Grok Bot.
# boot-1017-cursor-rebuild.sh - copy of boot-1017-cursor.sh pointing at qemu-system-x86_64-1017-fmaf-cursor-rebuild.exe (rebuild of the deleted cursor exe from 2ada8816). Written 2026-10-27 by Grok Bot.
# boot-1017-cursor.sh - copy of boot-1017-stopfence-drag.sh for qemu-system-x86_64-1017-fmaf-cursor.exe (stop fence + issue #3 cursor port), drag hook on, REIMS_VGPU_LAB_CURSOR_LOCKFREE=on, CURSOR_EPOCH_SKIP unset. Written 2026-10-19 by Grok Bot.
# boot-1017-stopfence-drag.sh - copy of boot-1017-stopfence.sh with REIMS_VGPU_LAB_WIN32_DRAG_HOOK=on. Everything else identical. Written 2026-10-19 by Grok Bot.
# boot-1017-stopfence.sh - copy of boot-1017-fmaf-fmdefaults-signed-usbextra.sh for qemu-system-x86_64-1017-fmaf-stopfence.exe (vendor/qemu 48848b20 + uncommitted STOP_FENCE/DIAG patch), QEMU_XHCI_ISO_STOP_FENCE=1 + QEMU_XHCI_ISO_DIAG=1. Everything else identical. Written 2026-10-19 by Grok Bot.
# boot-1017-fmaf-isosigned-video.sh - copy of boot-1017-fullmetal-audio-fps.sh for the 1017-fmaf-isosigned-video build (reims-1017 2889b2a8: Full Metal + 030755cd runtime port; vendor/qemu 26a315fe: iso resync + dirty stats unlocked + iso signed scheduling).
# Differences from boot-tahoe.sh: no REIMS_VGPU_LAB_* exports (LAB_DIRECT_REDRAW, LAB_DIRTY_HASH dropped), and the
# QEMU binary is fixed to qemu-system-x86_64-1017-fullmetal.exe. Same disks, OVMF, devices and audio (SDL). Written 2026-10-18 by Grok Bot.
# 2026-10-19 (Grok Bot): USB audio removed (virtio-sound-pci at 1b.0 is used). The empty xhciaudio controller (was 00:02.0) is removed too;
# qemu-xhci (xhci), ich9-ahci (sata) and vmxnet3 (net0) are pinned to their previous auto-assigned slots 01.0 / 03.0 / 04.0 so disk and en0 paths do not move. Slot 02 is left empty.
set -euo pipefail
export PATH="/ucrt64/bin:/usr/bin:$PATH"
export REIMS_VGPU_BACKEND=vulkan
export REIMS_VGPU_RAIL=vulkan
export REIMS_VGPU_WINDOW=on
export REIMS_VGPU_INPUT_CAPTURE=off
export REIMS_VGPU_GUEST_IMPORT=off
export REIMS_VGPU_GUEST_IMPORT_SURFACE_WRITES=off
export QEMU_WIN_PAGE_ALIAS=off
export REIMS_VGPU_PAGE_GUARDS=off
export REIMS_VGPU_DIRTY_OD_PREFETCH=on
export QEMU_XHCI_NOBQL=on
export QEMU_XHCI_IMOD_MIN_US=1000
export QEMU_SDL_OUT_PREFILL_MS=25
unset QEMU_XHCI_ISO_RESYNC   # off, like the original Full Metal launcher
unset REIMS_VGPU_LAB_DIRTY_STATS_UNLOCKED   # off, like the original Full Metal launcher
export QEMU_XHCI_ISO_SIGNED=1   # signed iso ON (FM defaults + signed test)
unset REIMS_VGPU_LAB_VIDEO_GATE   # off, like the original Full Metal launcher
unset REIMS_VGPU_LAB_RELEASE_GUARDS   # off, like the original Full Metal launcher
unset REIMS_VGPU_LAB_FRONTBUFFER_LATCH   # off, like the original Full Metal launcher
# QEMU_XHCI_ISO_UNDERRUN_ONCE not set (off)
export QEMU_XHCI_ISO_STOP_FENCE=1   # stop fence ON (vendor/qemu hcd-xhci.c, iso endpoints)
export QEMU_XHCI_ISO_DIAG=1   # xhci-isodiag lines every 2 s
export REIMS_VGPU_LAB_DIRTY_COMBO=off
export REIMS_VGPU_LAB_WIN32_DRAG_HOOK=on   # Win32 modal move/size loop keeps presenting (drag launcher)
export REIMS_VGPU_M2V_NATURAL_AGG_ALIGN="${REIMS_VGPU_M2V_NATURAL_AGG_ALIGN:-1}"   # colour fix; launcher sets 1 or 0 (-NoColour)
export REIMS_VGPU_LAB_CURSOR_LOCKFREE=on   # issue #3 Step B: 0x1220 cursor doorbell on the lock-free write rail (Rust)
# REIMS_VGPU_LAB_CURSOR_EPOCH_SKIP deliberately NOT set (Step A, QEMU): alone it caused stale menus in the old tree;
# its polled-work companion here is REIMS_VGPU_LAB_DIRTY_HOLE=tail (off). Keep unset until that pairing is verified.
unset REIMS_VGPU_LAB_CURSOR_EPOCH_SKIP
export REIMS_VGPU_DRAIN_PROFILE="${REIMS_VGPU_DRAIN_PROFILE:-off}"
export REIMS_VGPU_RESOLVE_PROFILE="${REIMS_VGPU_RESOLVE_PROFILE:-off}"


VM="C:/ReimsVM/Tahoe"
ROOT="C:/reims-lab/reims-vgpu"
QEMU="$VM/qemu-system-x86_64-1017-iconstack.exe"
EXTRA_DISK="D:/macOS Drive/macos_extra.qcow2"
MODE="${1:-boot}"
RECOVERY=()
case "$MODE" in
  install)
    RECOVERY=(
      -drive "id=Recovery,if=none,format=raw,file=$VM/recovery/BaseSystem.raw"
      -device ide-hd,bus=sata.3,drive=Recovery
    )
    ;;
  boot) ;;
  *) echo "Usage: boot-tahoe.sh [install|boot]" >&2; exit 2 ;;
esac
for required in "$QEMU" "$VM/OVMF_CODE_4M.fd" "$VM/OVMF_VARS.fd" \
                "$VM/OpenCore-tahoe.qcow2" "$VM/macos26.qcow2" "$VM/reims-vgpu-gop.rom" "$EXTRA_DISK"; do
  [[ -f "$required" ]] || { echo "Missing required file: $required" >&2; exit 1; }
done
if [[ "$MODE" == install && ! -f "$VM/recovery/BaseSystem.raw" ]]; then
  echo "The verified recovery media must be converted before launching." >&2
  exit 1
fi
mkdir -p "$VM/logs" /c/tmp
env | grep -E '^REIMS_VGPU_(LAB_)?(IOSURFACE|FORMATS_EXT|MIPGEN|INCARNATION|UNBOUND_BUFFER|PIPELINE_CACHE|BIND_FORMAT|SNAPSHOT_KEEP|DISPATCH_MEMO|DIRTY_BATCH|DIRTY_NARROW|M2V_NATURAL)' | sort | sed 's/^/iconstack /' >&2 || true
echo "Tahoe: Windows WHPX / Vulkan paravirtual GPU / 16 vCPUs / 12 GiB RAM / 300 GiB disk"
cd /c/ReimsVM/Tahoe   # vGPU log /tmp -> C:\tmp
exec "$QEMU" \
  -L "$ROOT/vendor/qemu/pc-bios" \
  -machine q35,kernel-irqchip=on \
  -accel whpx,intercept-msr-gp=on,ssd=off \
  -cpu EPYC-Rome-v4,family=23,model=113,stepping=0,level=13,check,-monitor,-svm,-wbnoinvd,-ibrs,-npt,-nrip-save,-arat \
  -smp 16,cores=16,sockets=1 -m 12288 \
  -uuid E19674D7-D209-4037-88A4-3D9DAB53E482 \
  -vga none -device pci-bridge,chassis_nr=5,id=pci.5,bus=pcie.0,addr=1e.0 \
  -device qemu-xhci,id=xhci,p2=8,p3=4,addr=01.0 \
  -device usb-kbd,bus=xhci.0,port=1 \
  -device usb-tablet,bus=xhci.0,port=2 \
  -audiodev sdl,id=HostAudio,out.frequency=48000,out.channels=2 \
-device virtio-sound-pci,id=MacVirtioSound,bus=pcie.0,addr=1b.0,audiodev=HostAudio \
  -device 'isa-applesmc,osk=ourhardworkbythesewordsguardedpleasedontsteal(c)AppleComputerInc' \
  -drive "if=pflash,format=raw,readonly=on,file=$VM/OVMF_CODE_4M.fd" \
  -drive "if=pflash,format=raw,file=$VM/OVMF_VARS.fd" -smbios type=2 \
  -device ich9-ahci,id=sata,addr=03.0 \
  -drive "id=OpenCoreBoot,if=none,format=qcow2,file=$VM/OpenCore-tahoe.qcow2" \
  -device ide-hd,bus=sata.2,drive=OpenCoreBoot \
  "${RECOVERY[@]}" \
  -drive "id=MacHDD,if=none,format=qcow2,file=$VM/macos26.qcow2" \
  -device ide-hd,bus=sata.4,drive=MacHDD \
  -drive "id=MacExtra,if=none,format=qcow2,file=$EXTRA_DISK" \
  -device usb-storage,drive=MacExtra,id=MacExtraUSB,bus=xhci.0,port=3,removable=on,serial=REIMS-MAC-EXTRA \
  -netdev user,id=net0,ipv6=off,hostfwd=tcp:127.0.0.1:2222-:22,hostfwd=tcp:127.0.0.1:5900-:5900 \
  -device vmxnet3,netdev=net0,id=net0,mac=00:0c:29:ea:5e:01,addr=04.0 \
  -serial "file:$VM/logs/serial.log" \
  -qmp tcp:127.0.0.1:4444,server=on,wait=off -action reboot=reset \
  -display none \
  -device "reims-vgpu-pci,id=reimsvgpu,romfile=$VM/reims-vgpu-gop.rom,rombar=1,bus=pci.5,addr=00.0"
