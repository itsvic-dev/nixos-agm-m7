# GPU

## Hardware

- PowerVR Rogue GE8100 (Series 8XE), BVNC 22.49.21.16, MIPS firmware processor.
- Stock driver: DDK 1.9@4893595, OpenGL ES 3.2.
- Stock DT:
  - `mfg_auckland@13000000`, size 0x80000, SPI 226 level, 570 MHz.
  - Clocks: `CLK_TOP_MFG` (topckgen 45, `mfg_sel`), `CLK_TOP_MFG_OFF`, `CLK_TOP_MFG_ON` (topckgen 32).
  - Power: scpsys MFG0 and MFG1.
  - `mfg_cfg@13ffe000`: HW APM registers (0x24, 0x504..0x528), set by `MTKInitHWAPM`.
- Core config (`rgxconfig_km_22.V.21.16.h`): 1 cluster, 1 raster pipe, 2 ISP pipes, 40-bit GPU VA, 32-bit physical bus, SLC 8 KiB.

## Upstream status

- Kernel `drivers/gpu/drm/imagination` supports 33.15.11.3 and 36.53.104.796. 36.52.104.182 is experimental. Other cores need `exp_hw_support=1`.
- The kernel reads the feature list from the firmware file. Imagination publishes no `rogue_22.49.21.16_v1.fw`.
- Mesa `src/imagination` has GE8300 (22.67.54.30, 22.68.54.30, 22.102.54.38) as unsupported. GE8100 is not listed.

## Vendor firmware

- `/vendor/firmware/rgx.fw.signed` is a plain MIPS32 ELF with symbols and DWARF. There is no signature wrapper.
- Its firmware interface (FWIF) is DDK 1.9 and does not match mainline:

| Struct | DDK 1.9 | Mainline |
|---|---|---|
| init | 608 (`RGXFWIF_INIT`) | 544 sysinit + 104 osinit |
| kccb_cmd | 144 | 88 |
| ccb_ctl | 16 | 144 |
| ccb_cmd_header | 56 | 40 |
| fwcommoncontext | 160 | 152 |
| hwrtdata | 328 | 384 |
| tracebuf | 728 | 864 |
| runtime_cfg | 20 | 72 |
| compchecks_bvnc | 24 | 16 |
| fwmemcontext | 24 | 32 |
| freelist | 64 | 64 |

## Reference source

The matching DDK 1.9 kernel side, with MT6739 platform code, is public:
github.com/LgPWNd/Alcatel_mt6739_5059R_and_5041C_universal_kernel-4.4.95-,
`kernel-4.4.95/drivers/misc/mediatek/gpu/gpu_rgx/m1.9ED4893595/`.

- `services/system/rgx_mtk/mt6739/mtk_mfgsys.c`: power and clock sequence, HW APM.
- `hwdefs/km/cores/rgxcore_km_22.49.21.16.h`: BRNs 60084, 61450, 63027, 65273, 63553.

## Stage 1 (done)

- MFG0 (`MFG_PWR_CON` 0x338, status bit 4, bus protect bits 21 and 22) and MFG1 (`MFG_CORE0_PWR_CON` 0x33c, status bit 31) in `mt6739-pm-domains.h`. MFG1 is a subdomain of MFG0.
- `gpu@13000000` uses `mfg_sel` as its core clock. LK leaves MFGPLL at 570 MHz and Vcore at 1.15 V.
- `insmod powervr.ko exp_hw_support=1` reads BVNC 22.49.21.16, then fails on the missing `powervr/rogue_22.49.21.16_v1.fw`.
- genpd keeps all domains of a provider on until every consumer has probed. The GPU never probes yet, so CONN stayed on and the CONSYS ROM hung at PC 0x98d4. CONN now has `MTK_SCPD_NO_STAY_ON`.
- The vendor MFG sequence also changes `INFRA_TOPAXI_SI0_CTL` (0x10001200; bit 7 way_en, bit 1 bypass) around the bus protect. The mainline driver does not do that yet.

## Stage 2 (done)

- DT compatible `mediatek,mt6739-gpu` selects the DDK 1.9 path (`pvr_ddk19.c`). The rest of the driver is unchanged.
- Firmware: `/vendor/firmware/rgx.fw.signed`, linked as `powervr/rogue_22.49.21.16_ddk19.fw` by `nixos/vendor.nix`.
- The driver synthesises the layout table and the feature set (from `rgxconfig_km_22.V.21.16.h`) instead of reading a mainline firmware trailer.
- MIPS: 32 MiB heap at 0xC0000000, one 32 KiB contiguous page table, boot data (regs, page table, stack, `RGXFWIF_INIT`) in the boot/NMI data page, wrapper config with the physical register bank address, no trampoline.
- `pvr_ddk19_fwif.h` has the 1.9 structs. Offsets come from the firmware DWARF (`tools/fwif-dwarf.py`) and are checked with `static_assert`.
- HW APM writes to `mfg_cfg` after every power-on, as `MTKInitHWAPM` does.
- Result: `firmware_started` is set, compatibility checks report BVNC 22.49.21.16, DDK 1.9 build 4893595, MIPS. The firmware trace works (decode with `rgx_fwif_sf.h`: id = bits 0-11, group = bits 12-15, argument count = bits 16-19).
- The probe then stops on purpose with `-ENODEV` and halts the firmware. `powervr` is blacklisted; load it with `modprobe powervr`.

## Stage 3a (done)

- The firmware keeps running after probe. `fw_dev.initialised` is set, the DRM device registers.
- Kernel CCB: 128 commands of 144 bytes, MTS kick on DM GP. Completion through sync words (`SYNC` command), no return slots.
- Firmware CCB: processed in the IRQ thread. Commands are only logged for now.
- Runtime PM: `FORCED_IDLE_REQ`, then `OFF_REQ` (forced), then `pvr_fw_stop()`, then MFG power off. Resume restarts the firmware and waits for `firmware_started`. A forced power off without forced idle first makes the firmware assert (`rgxfw_irq.c:693`).
- MMU cache flush uses `MMUCACHE` with the mainline flag values (they match 1.9).
- Job ioctls (VM and job contexts, free lists, HWRT datasets, submit) return `-EOPNOTSUPP`.
- Debugfs `dri/13000000.gpu/ddk19/`: `state` (power state, CCB offsets, assert, trace) and `sync` (write to send a `SYNC`). Decode the trace with `tools/fwtrace-decode.py rgx_fwif_sf.h < state`.

## Stage 3b (done)

- The first 24 bytes of the mainline FW memory context match `RGXFWIF_FWMEMCONTEXT`, so the mainline VM code works unchanged. The MMU definitions of both are the same.
- `tools/pvr-vmtest.c`: VM context, BO, map, unmap. Map and unmap send `MMUCACHE` through the firmware.

## Stage 3c (compute, kernel side done)

- Client CCB: 56-byte headers, 1.9 command types (task bit 31, no magic). Size estimates use the larger header.
- `KICK` carries the context's DM (CDM = 4). With DM 0 the firmware queues the work on GP and nothing runs.
- Compute context: 160-byte `RGXFWIF_FWCOMMONCONTEXT`, then the mainline static compute state for our own use. The context state object holds the 8-byte `COMPUTECTX_STATE` and a 16-byte RF command. The firmware dereferences the RF command pointer once BRN 65273 is set up, so it must not be 0.
- BRN 65273: before the first compute kick after power-up, `rgxfw_framework_kick_cdm` loads the context PC into all 8 BIF catalogue bases and runs dummy tiling passes into `0xF800000000` (INIA) and `0xF900000000` (INIB). Every VM context maps 64 KiB there. The vendor userspace does the same ("BRN65273_RgnHeader").
- MFG1 (dusts): HW APM switches it through the SPM firmware, which mainline does not load. MFG1 is a normal genpd domain under the GPU, off by default; HW APM is off (`powervr.hw_apm`).
- `tools/pvr-computetest.c`: a compute job whose control stream is only `STREAM_TERMINATE` (0x80000000). Runs without HWR.
- Not done: `PDS_CTRL`, `TPU`, `TPU_ARRAYS_CDM`, `COMPUTE_THREAD_BARRIER` and the YUV coefficients are 0 in the compute command. Mesa does not send them. Read the vendor values on LineageOS before running real shaders.
- Heaps: with BRN 65273 the PDS, USC and VisTest heaps must sit in one 1 GiB PC entry each (1.9: PDS 0xA800000000, USC 0xBA00000000). The kernel still reports the mainline heaps.
- Debugging: `ddk19/state` lists HWR entries (type, DM, BIF/MMU status, PC address); `ddk19/nmi` takes an NMI and prints the MIPS EPC, cause and bad address. Disassemble with `mipsel-linux-gnu-objdump -d -l -M micromips`; the firmware has symbols and line info.

## Stage 4 (Mesa, started)

- `nixos/gpu.nix`: Mesa overlay with only the `imagination` Vulkan driver (no OpenGL, EGL, GBM or LLVM; about 13 MiB) and `vulkan-tools`. Sway stays on pixman.
- `nixos/patches/mesa-ge8100.patch`: device entry for 22.49.21.16. The features are the GE8300 ones for now.
- Kernel: free lists (1.9 adds a HWR counter at offset 40), render contexts (TA and 3D common contexts back to back, 328 bytes) and transfer contexts (one 3D common context). Each queue state object is followed by its RF command.
- `PVR_I_WANT_A_BROKEN_VULKAN_DRIVER=1 vulkaninfo` lists "PowerVR Rogue GE8100", Vulkan 1.2. Device creation runs about 56 firmware commands without HWR.
- Fixed: MMU flush, cleanup and `SYNC` must hold a runtime PM reference. Without it the firmware read one slot past the KCCB write offset and later kicks failed, so `vulkaninfo` hung.

## Plan

1. Platform: MFG power domains, clocks and DT node. Read the core ID register. Done.
2. Firmware boot: repack the ELF for the mainline loader, add a DDK 1.9 init path, pass the compatibility check. Done.
3. Jobs: DDK 1.9 command queues, geometry and 3D commands, render targets.
4. Mesa: GE8100 device info, vkcube, then Zink.
