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

## Plan

1. Platform: MFG power domains, clocks and DT node. Read the core ID register. Done.
2. Firmware boot: repack the ELF for the mainline loader, add a DDK 1.9 init path, pass the compatibility check.
3. Jobs: DDK 1.9 command queues, geometry and 3D commands, render targets.
4. Mesa: GE8100 device info, vkcube, then Zink.
