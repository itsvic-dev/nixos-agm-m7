# microSD (MSDC1) SDR104 errors: research

Symptom: under sustained load, `msdc_track_cmd_data: cmd=12 arg=00000000; host->error=0x00000002` (CMD12 response timeout), then `I/O error, dev mmcblk1 ... op 0x0:(READ)`. The same sectors read fine later. 36 lines in one boot. At 96 MHz (debugfs, no re-tune) there are fewer errors, but they do not stop.

Trees:
- Mainline: `linux/` (branch linux-7.2).
- Vendor: `ref/lumi-mt6739/drivers/mmc/host/mediatek/ComboA/` (below: `ComboA/`).
- Stock DT: `stock/stock.dts`.

## 0. What the log lines mean (verified in source)

- The `cmd=12` lines are a symptom, not the first error. `mmc_blk_mq_rw_recovery()` calls `mmc_blk_fix_state()`, which sends a standalone CMD12 with 5 retries (`drivers/mmc/core/block.c:1754-1781`, `1838-1876`; retry loop in `drivers/mmc/core/core.c:412-420`). Each failed try prints one line, so one recovery gives 6 lines. 36 lines = about 6 recovery events.
- CMD12 gets no response when the card is not in the `data`/`rcv` state (the card ignores an illegal command). So these timeouts are expected once the card has left the data state. The first error happens earlier.
- The first error is not logged. mmc1 uses HSQ (`mtk-sd.c:3157-3169`, enabled because of `no-sdio`). In `msdc_request_done()` the HSQ path returns before `msdc_track_cmd_data()` (`mtk-sd.c:1334-1339`). Data CRC or timeout errors on CMD18/CMD25 are reset silently. Only the non-HSQ CMD12 from recovery reaches the print.
- `err_stats` is zero because mtk-sd never calls `mmc_debugfs_err_stats_inc()`. The core only counts tuning errors (`drivers/mmc/core/core.c:959`). The zeros tell us nothing.
- Each recovery can also end in `mmc_blk_reset()` (power cycle, re-init, re-tune). This explains the long I/O stalls under load.

## 1. Likely causes, ranked

### 1.1 Delay lines and RX enhance are in the MSDC1 "top" block, which mainline does not map (strong, inferred)

- Stock MSDC1 has two reg ranges: `0x11240000` and the pad macro `0x11c40000` (`stock/stock.dts:1460`, `1775-1778`). Mainline mmc1 has only `0x11240000` and uses `mediatek,mt6779-mmc` (`mt6739.dtsi:572-580`). `mt6779_compat` has no `needs_top_base` (`mtk-sd.c:556-568`).
- Vendor: `msdc1_pad_dly_top = 1`, `msdc1_rx_enhance_top = 1` (`ComboA/mt6739/autok_cust.h:270,273`). With `base_top` set, autok writes CMD/DAT RX delay, CLK TX delay and RX enhance to the top block (`EMMC_TOP_CONTROL`, `EMMC_TOP_CMD`, `TOP_EMMC50_PAD_CTL0`) and not to `MSDC_PAD_TUNE0` (`ComboA/autok.c:2072-2110`, `2371-2392`). The vendor does not even save `PAD_TUNE0` when `base_top` exists (`ComboA/msdc_tune.c:106-125`).
- Mainline without `top_base` writes the delays to `PAD_TUNE0` (0xf0) and sets RX enhance in `SDC_ADV_CFG0` bit 20 (`mtk-sd.c:2003-2012`, `2241-2295`). On MT6739 these are probably not the active path. Then `msdc_tune_together()` only selects a sample edge. SDR104 at 192 MHz runs with no real delay tuning and no RX enhance.
- The top block layout in the vendor (`ComboA/msdc_reg.h:1056-1098`) is the same as mainline's `top_base` layout (`mtk-sd.c:95-98`, `323-338`). mt8183 uses it already.
- Same pattern on MSDC0: the 0x11cd0000 block. NOTES.md says the HS400 strobe scan in `PAD_DS_TUNE` (main block) finds no working value. That fits.

### 1.2 Pad input/output select at 1.8 V is not set (medium, inferred)

- Vendor, on the switch to 1.8 V: `TDSEL = 0`, `RDSEL = 0` for MSDC1 (`ComboA/mt6739/msdc_cust.c:194-199`). At 3.3 V: `TDSEL = 0xA`, `RDSEL = 0xC` (`msdc_cust.c:301-304`).
- Registers (iocfg_lb, 0x10002400): TDSEL at +0xb0 (DAT bits 7:4, CMD 11:8, CLK 15:12); RDSEL at +0x70 (CLK bits 11:6, CMD 17:12, DAT 23:18) (`msdc_cust.h:217-220`, `386-394`).
- Mainline `pinctrl-mt6739.c` has no TDSEL/RDSEL ranges (`mt6739_reg_cals`, line 512). Nothing writes them. If LK/preloader left the 3.3 V values, the input threshold is wrong for 1.8 V signals. This would cause errors that do not go away at a lower clock. Not checked on hardware.
- Vendor also turns on Schmitt trigger for MSDC1 CLK/CMD/DAT (`ComboA/sd.c:968`, `msdc_cust.c:841-850`). Mainline DT does not set `input-schmitt-enable`. The mainline SMT fields exist (`pinctrl-mt6739.c:36-38`).
- Drive strength matches stock: raw 3 on CLK/CMD/DAT in every state (`stock/stock.dts:1491-1557`; vendor dump expects `0xdb`, `msdc_cust.c:800-802`). Mainline DT uses `drive-strength = <3>` with raw drive (`mt6739-agm-m7.dts:378-397`). Field positions match (`pinctrl-mt6739.c:282-284`).
- Pulls: vendor uses 50 kOhm pull-up on CMD/DAT (R1=1, R0=0; `msdc_cust.c:1063-1068`). Mainline uses `MTK_PUPD_SET_R1R0_01`, which is 10 kOhm. CLK pull-down 50 kOhm matches. Small effect.

### 1.3 Patch-bit differences against vendor SDR104 settings (low to medium, verified in source, effect unknown)

Vendor (`ComboA/msdc_tune.c:565-581`, `ComboA/autok.c:3131-3152`, `3277-3283`, `mt6739/autok_cust.h:214-288`) vs mainline `msdc_init_hw()` (`mtk-sd.c:1874-2088`):

| Field | Vendor | Mainline (mt6779) |
|---|---|---|
| PB1 STOP_DLY_SEL | 3 | 3 (same) |
| PB2 POP_EN_CNT | 8 | not written (reset value) |
| PB1 bits 19/20 | 1/1 | 1/1 (same) |
| SDC_FIFO_CFG WR/RD_VALID_SEL | 0 | 0 (same) |
| RX enhance | top +0x0 bit 15 | ADV_CFG0 bit 20 |
| PB2 RESPSTSENSEL / CRCSTSENSEL | 0 / 0 | not written |
| PB1 CMD_RSP_TA / WRDAT_CRCS_TA | 0 / 0 | 1 / 1 |
| PB1 GET_BUSY_MA (bit 6) | 1 | 0 |
| PB0 CKGEN_MSDC_DLY_SEL | 0 | 1 |

`mt8189_compat` already has `stop_dly_sel = 3`, `pop_en_cnt = 8` (`mtk-sd.c:678-695`). That is the vendor "multi_sync" set.

### 1.4 CMD23 on the SD card (low, inferred)

- Vendor sets `MMC_CAP_CMD23` only for eMMC (`ComboA/sd.c:4966-4969`). SD reads/writes are open-ended with hardware auto-CMD12 (`sd.c:2885-2895`).
- Mainline sets `MMC_CAP_CMD23` for all hosts (`mtk-sd.c:3096`) and sends CMD23 manually for SD (`mtk-sd.c:1532-1534`). The card SCR (`02b5808300000000`) says CMD23 is supported, so the core uses it.
- The card is an OEM part (manfid 0xc9, name `MSSD0`). Some cards handle CMD23 badly. This path was never used on stock.

### 1.5 Clock (low, verified partly)

- Mainline: `msdc30_1_sel` = `msdcpll_d2`, 192 MHz, mode 1 (no divider) (`mtk-sd.c:1066-1069`).
- Vendor: `HOST_MAX_MCLK` 200 MHz; table says 200 MHz (`MSDCPLL/2`) or 208 MHz for `clk_src` 2 (`msdc_cust.h:162-163`). Stock DT uses `clk_src = [02]`, but the driver uses `clk_get_rate()` if it works (`msdc_cust.c:529-533`). So stock also runs about 192-200 MHz. No sign of an MSDC1 SDR104 limit in vendor code.
- Vendor sets VMC to 1.86 V at 1.8 V signalling (`msdc_cust.h:136`, `msdc_cust.c:189-192`). Mainline uses 1.80 V. Small.
- Vendor tunes `INT_DAT_LATCH_CK_SEL` for SD (`autok.c:3761-3830`). Mainline uses fixed `mediatek,latch-ck` (default 0). This matters when sclk < hclk, for example the 96 MHz debugfs test.

## 2. Changes to try (smallest and safest first)

### A. Get the first error (no code change)

```
echo 'format "interrupt events" +p' > /sys/kernel/debug/dynamic_debug/control
echo 'format "data_error=" +p'      > /sys/kernel/debug/dynamic_debug/control
echo 'format "cmd_error=" +p'       > /sys/kernel/debug/dynamic_debug/control
cd /sys/kernel/tracing
echo 'cmd_err != 0 || data_err != 0 || sbc_err != 0 || stop_err != 0' > events/mmc/mmc_request_done/filter
echo 1 > events/mmc/mmc_request_done/enable
```

This shows whether the first error is a data CRC (-84), a data timeout (-110), or a CMD13/CMD23 error.

At boot, check tuning with `dyndbg="file mtk-sd.c format \"phase:\" +p; file mtk-sd.c format \"Final pad delay\" +p"`. If the rising-edge map is all ones (`ffffffffffffffff`) or all zeros, the PAD_TUNE0 delays do nothing. That confirms 1.1.

### B. Read pad and controller registers (read only)

| Address | Meaning | Vendor value at 1.8 V |
|---|---|---|
| 0x100024b0 | MSDC1 TDSEL | bits 15:4 = 0 |
| 0x10002470 | MSDC1 RDSEL | bits 23:6 = 0 |
| 0x10002490 | MSDC1 SMT | bits 3:1 = 0b111 |
| 0x10002400 | MSDC1 DRV | bits 11:3 = 0xdb |
| 0x112400b4 / b8 | PATCH_BIT1 / 2 | see table 1.3 |
| 0x11c40000 / 04 / 08 | top CONTROL / CMD / PAD_CTL0 | |

`0x11c40000` is not claimed by any driver, so `/dev/mem` can read it. The MSDC1 main block is claimed, so `CONFIG_IO_STRICT_DEVMEM` may block it.

### C. DT only: Schmitt trigger and a real UHS state

Add `input-schmitt-enable;` to both groups of `mmc1_pins`. Make a separate `mmc1_pins_uhs` for `pinctrl-1`. Later try drive 4 or 5 on CLK only in that state.

### D. TDSEL/RDSEL

1. Quick test at runtime: write the vendor 1.8 V values (clear bits 15:4 of 0x100024b0 and bits 23:6 of 0x10002470). Vendor does the same at the voltage switch, so it is low risk.
2. Proper fix: add `PINCTRL_PIN_REG_TDSEL` and `PINCTRL_PIN_REG_RDSEL` ranges to `pinctrl-mt6739.c` (at least for pins 71 to 76; base index 3):
   - TDSEL +0xb0: pin 71 bit 8, pin 72 bit 12, pins 73-76 shared bit 4, width 4.
   - RDSEL +0x70: pin 71 bit 12, pin 72 bit 6, pins 73-76 shared bit 18, width 6.
   
   Then set `mediatek,tdsel = <0>; mediatek,rdsel = <0>;` in the UHS state and `<0xa>` / `<0xc>` in the default state.

### E. Map the top block (main candidate)

1. DT-only test: on `&mmc1`, set `compatible = "mediatek,mt8183-mmc";` and `reg = <0 0x11240000 0 0x1000>, <0 0x11c40000 0 0x1000>;`. mt8183 has the same flags as mt6779 plus `needs_top_base`. Without the card, the initrd fails and the phone reboots into LineageOS, so a failed test is recoverable.
2. Proper fix: a `mt6739_compat` in `mtk-sd.c` with `needs_top_base = true`, `pad_tune_reg = MSDC_PAD_TUNE0`, `async_fifo`, `data_tune`, `busy_check`, `stop_clk_fix`, `stop_dly_sel = 3`, `pop_en_cnt = 8`, `enhance_rx`, `support_64g`, `clk_div_bits = 12`. Add the second reg in `mt6739.dtsi` for both hosts (MSDC0 top: 0x11cd0000). The dt-binding must allow 2 reg entries for mt6739.

### F. Workaround if nothing else helps

- `max-frequency = <100000000>;` on mmc1. The card tunes at 96 MHz at boot. The debugfs test did not re-tune.
- Or remove `sd-uhs-sdr104;` (SDR50, 96 MHz). Expect about 40-45 MB/s.

### G. Only for testing: no CMD23 on SD

In `msdc_drv_probe()`, set `MMC_CAP_CMD23` only when `!(mmc->caps2 & MMC_CAP2_NO_MMC)`. This matches the vendor. The core then uses open-ended reads plus CMD12.

## 3. Test plan

Do one change at a time. Each run uses a fresh boot.

1. Load (about 10 minutes, root on the card):
   - `dd if=/dev/mmcblk1 of=/dev/null bs=1M iflag=direct` over the whole card.
   - At the same time, write and `fsync` 1 GiB files on the root fs in a loop. Read them back with `sha256sum`.
   - The old reproducer: `e2fsck -fn` and `resize2fs` on a copy, if possible.
2. Count after each run:
   - `dmesg | grep -c msdc_track_cmd_data` (divide by 6 for recovery events).
   - `dmesg | grep -c 'I/O error, dev mmcblk1'`.
   - Lines from the `mmc_request_done` trace filter (first errors, by type).
   - `mmc1:` reset or re-tune messages.
   - Throughput of the dd read and the write loop, and the worst stall (`iostat -x 1` or the dd progress).
   - sha256 mismatches (must be 0).
3. Pass: 0 first errors and 0 I/O errors in 3 runs, and read speed at least 80 MB/s. Baseline is 36 CMD12 lines (about 6 events) per boot.
4. Check that the eMMC still works if the MSDC0 node was changed.

## Result (2026-10-13)

- Diagnostic kernel with the old DT: the MSDC1 phase map was `ffffffffffffffff`. The first errors were data CRC errors (interrupt events `0xa040`, `data_error=-84`), not CMD12 timeouts.
- With `compatible = "mediatek,mt6739-mmc", "mediatek,mt8183-mmc"` and the second `reg` block at 0x11c40000, the map was `c01fffff87ffff91` (final delay 42).
- A 10 minute load test at 192 MHz SDR104 (full-card direct read plus 26 x 256 MiB write and verify) gave 0 I/O errors, 0 CMD12 recovery, 0 data errors, 0 resets and 0 checksum mismatches. Direct reads went from 15 MB/s to 70 MB/s.
- Still open: the TDSEL/RDSEL and Schmitt trigger pad settings, and the same fix for MSDC0 (HS400).
