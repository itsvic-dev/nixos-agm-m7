# CONSYS power-on: reverse engineering notes (MT6739, stock wmt_drv.ko)

Sources and how sure each item is:
- **[D]** Verified in the disassembly of `logs/reference/wifi/ko/wmt_drv.ko` (`llvm-objdump -d -r`, relocations resolved).
- **[S]** MediaTek's own `common_main/platform/mt6739.c`, `include/mt6739.h` and `mtk_wcn_consys_hw.c` are public
  (github.com/gemian/cosmo-linux-kernel-4.4, `drivers/misc/mediatek/connectivity/common/common_main/platform/`).
  The binary matches this source function for function, so we do not need the MT7623 code as a reference.
- **[R]** Vendor reference kernel `ref/lumi-mt6739` (SPM MTCMOS, clkbuf, EMI MPU, MT6357 headers).
- **[L]** Stock `logs/reference/wifi/dmesg.txt`.

## Bases (consys_read_reg_from_dts, [D])
`of_iomap(node, i)` on `mediatek,mt6739-consys`, which is stored in `conn_reg[]`:
| idx | DT reg | name | used for |
|---|---|---|---|
| 0 | 0x18070000/0x200 | mcu_base (CONN_MCU_CONFIG) | CHIP_ID +0x08, ACR +0x110, CPUPCR +0x160 |
| 1 | 0x10007000/0x100 | ap_rgu_base (TOPRGU) | CPU_SW_RST +0x18 |
| 2 | 0x10000000/0x2000 | topckgen_base | +0x1380 EMI remap, +0x1f00 AP2CONN_OSC_EN (these are in INFRACFG_AO 0x10001000) |
| 3 | 0x10006000/0x1000 | spm_base | dump only: +0x180, +0x184, +0x32c |
Clock: `devm_clk_get(dev, "conn")` -> DT `clocks = <&scpsys SCP_SYS_CONN(3)>`. This is the CONN MTCMOS power domain.
Regulators: `regulator_get(dev, "vcn18" | "vcn28" | "vcn33_bt" | "vcn33_wifi")`.

## 1. Power-on order (mtk_wcn_consys_hw_pwr_on -> mtk_wcn_consys_hw_reg_ctrl(1, co_clock)) [D]+[S]
The ops table is `consys_ic_ops` (.data+0xf4). The calls are indirect. Order:
1. `consys_hw_vcn18_ctrl(1)`:
   - PMIC `RG_LDO_VCN18_EN`=1 (enum 0xc2f -> MT6357 `LDO_VCN18_CON0` 0x1a58 bit0)
   - PMIC `RG_LDO_VCN18_SW_OP_EN`=1 (enum 0xc31 -> `LDO_VCN18_OP_EN` 0x1a5a bit0)
   - `regulator_set_voltage(vcn18, 1.8 V)`, then `regulator_enable`
   - `regulator_set_voltage(vcn33_bt, 3.3 V)`, then `regulator_enable` (log: "ldo vcn33_bt enable")
   - `RG_LDO_VCN33_HW0_OP_EN`=0 (0xd03 -> 0x1b1e bit1), `RG_LDO_VCN33_HW0_OP_CFG`=0 (0xd08 -> 0x1b24 bit1)
2. `udelay(150)`. The constant is 15 times larger than the one in step 7, and the source says 150 us.
3. If co_clock (on this phone co_clock = 1, [L]):
   - `connectivity_export_clk_buf_ctrl(CLK_BUF_CONN=1, 1)`. On the vendor kernel this only records state if the XO2 buffer is already under SW control and the DCXO CONN enable is set. It does not toggle hardware ([R] clkbuf_v1/mt6739). The XO_WCN buffer (MT6357 DCXO EXTBUF2) runs in HW mode from the conn srclkena. Mainline can ignore this step if the buffer is on.
   - `consys_vcn28_hw_mode_ctrl(0)`: `RG_LDO_VCN28_HW0_OP_EN`=0 (0xd3b -> 0x1b8a bit1), `RG_LDO_VCN28_HW0_OP_CFG`=0 (0xd41 -> 0x1b90 bit1)
   - `mtk_wcn_consys_hw_efuse_paldo_ctrl(1, 1)` -> `consys_hw_vcn28_ctrl(1)`: `regulator_set_voltage(vcn28, 2.8 V)`, then `regulator_enable`
   Without co-clock: vcn28_hw_mode_ctrl(1) (HW0_OP_EN=1, OP_CFG=0), then vcn28_ctrl(1).
4. `consys_hw_reset_bit_set(1)`: TOPRGU 0x10007018 = read | 0x88001000 (key 0x88 in [31:24], CONN CPU reset bit 12 = 1).
5. `consys_hw_spm_clk_gating_enable()`: empty (the write of 0x0b160001 to SPM 0x10006000 is commented out in the source).
6. `consys_hw_power_ctrl(1)`: `clk_prepare_enable(conn)`. This turns on the SCP_SYS_CONN MTCMOS. In the vendor kernel it is `spm_mtcmos_ctrl_conn(ON)` ([R] drivers/clk/mediatek/clk-mt6739-pg.c):
   - SPM `CONN_PWR_CON` 0x1000632c |= PWR_ON(bit2), then |= PWR_ON_2ND(bit3)
   - poll until `PWR_STATUS` 0x10006180 bit1 = 1 and `PWR_STATUS_2ND` 0x10006184 bit1 = 1
   - CONN_PWR_CON &= ~PWR_CLK_DIS(bit4), then &= ~PWR_ISO(bit1), then |= PWR_RST_B(bit0)
   - INFRACFG_AO `TOPAXI_PROTECTEN_CLR` 0x100012a4 = (1<<13)|(1<<14) (release bus protect, no ack poll)
   - Precondition: SPM writes are accepted only after `POWERON_CONFIG_EN` 0x10006000 = 0x0b160001 (project key 0xb16, bit0). The stock boot sets this before Linux runs. Mainline must check it.
7. `udelay(10)`.
8. `consys_ahb_clock_ctrl(1)`: returns 0, no-op.
9. `polling_consys_chipid()` (see section 2).
10. `consys_acr_reg_setting()`: 0x18070110 |= BIT(18) (ACR MBIST real-speed).
11. `consys_afe_reg_setting()`: empty (CONSYS_AFE_REG_SETTING=0).
12. `consys_hw_reset_bit_set(0)`: 0x10007018 = (read & ~0x1000) | 0x88000000 (release the CONN CPU from reset).
13. `msleep(20)`.
Then pwr_on calls `mtk_wcn_consys_hw_gpio_ctrl(1)` (BGF EINT irq 311 from DT, pinctrl) and `mtk_wcn_consys_jtag_set_for_mcu()` (returns 0).
Before any power-on, pwr_on returns -1 if `gConEmiPhyBase` == 0 [D].

## 2. Chip ID test [D]+[S]+[L]
`polling_consys_chipid`: up to 10 reads of **0x18070008** (mcu_base+0x8), with `msleep(20)` between reads. Success value is **0x00000699**, not 0x6739. This is the CONNSYS HW code. WMT logs "get hwcode (chip id) (0x699)" and "SOC_CONSYS.E2 (0x8b00/0x8a00)". `consys_soc_chipid_get()` returns the constant 0x6739 (the SoC ID, not read from hardware).
Stock log: the first read is 0x00000000. The second read, 20 ms later, is 0x00000699 ("retry(8)"). So poll for some tens of ms. The CONN CPU is still in reset (step 4) during the poll. The ID is readable as soon as the MTCMOS is on and bus protect is released.
If the poll fails, the driver dumps 0x10007018, 0x10006180, 0x10006184 and 0x1000632c. These are good debug reads for us too.

## 3. Power-off (reg_ctrl(0, co_clock)) [D]
ahb_clock_ctrl(0) (no-op), then `clk_disable_unprepare(conn)`. The vendor MTCMOS off sequence is: `TOPAXI_PROTECTEN_SET` 0x100012a0 = 0x6000, poll `PROTECTEN_STA1` 0x10001228 & 0x6000 == 0x6000, then CONN_PWR_CON |= ISO, |= CLK_DIS, &= ~RST_B, &= ~PWR_ON, &= ~PWR_ON_2ND, then poll status bit1 = 0. Then, with co-clock: clk_buf_ctrl(CONN, 0). Without co-clock: vcn28_hw_mode(0) and vcn28_ctrl(0). Then `consys_hw_vcn18_ctrl(0)`: VCN18_EN=1 and SW_OP_EN=1 (sic), regulator_disable(vcn33_bt), VCN18_EN=1 and SW_OP_EN=1 again, regulator_disable(vcn18). Then msleep(20). In co-clock mode, VCN28 is not turned off here. The WMT core turns it off after FW init through efuse_paldo_ctrl(0) ([L] at 41.55 s).

## 4. EMI [D]+[S]+[R]+[L]
- Reserved memory: `consys-reserve-memory` at 0x77c00000, size 0x200000 ([L]). gConEmiPhyBase/gConEmiSize come from the CONNADP reserved-memory hook.
- Remap register: **0x10001380** (topckgen_base+0x1380, INFRACFG_AO). Value: `|= ((phys >> 21) & 0x1fff) | BIT(13)` (bit13 enables the remap). Stock: 0x77c00000 -> 0x000023be ([L] "CONSYS_EMI_MAPPING ... 0x000023be"). It is an OR, not a write, so clear the field first on a re-run.
- The CONN side sees EMI at 0xf0080000 (= AP phys + 0x80000). The `mtk_wcn_emi_addr_info` table has 0xf0080000 with offsets 0x400 (paged trace), 0x8400 (paged dump), 0x10400 (full dump). The probe ioremaps phys+0x80000, length 0x55c00, and zeroes it.
- MPU: `emi_mpu_set_region_protection(start=phys+0x80000, end=phys+size-1, region=22, perm=0x04b6da28)` = SET_ACCESS_PERMISSON(LOCK, F,F,F,F,F,NP,F,NP). Domains 0 (AP) and 2 (CONN) have no protection. All others are forbidden. Vendor emi_mpu (mt6739) does **no MMIO**. It does an SMC: `MTK_SIP_KERNEL_EMIMPU_SET` = 0x82000209 (32-bit kernel; 0xC2000209 from arm64). The args are (start>>16, end>>16, apc) with apc = (perm & 0x4ffffff) | (region<<27) = 0xb4b6da28 (on a 32-bit kernel, apc is passed twice). Stock: 0x77c8, 0x77df. The call happens in probe and in the restore callback, not at every power-on.

## 5. Stock log timing (dmesg.txt 4973-4989)
39.501727 reg_ctrl start -> 39.501786 vcn33_bt enable -> 39.502507 clk buf (co_clock 1) -> 39.503002 vcn28 on -> 39.503065 first poll returns 0 -> 39.527095 0x699 -> 39.551322 finish (after the 20 ms sleep) -> 39.552272 WMT hwcode 0x699. The total power-on time is about 50 ms. IRQ from DT: 311, flags 8 (level low). Later, wlan_gen2 reads 0x18070008 = 0x699 again ("wlanDumpMcuChipId").

## 6. Open questions
- Mainline has no MT6739 SCPSYS CONN domain. For the spike, run the SPM sequence of step 6 directly. Check 0x10006000 (POWERON_CONFIG_EN) first. Also check that INFRACFG_AO 0x100012a0/0x100012a4 (SET/CLR) are the right bus-protect registers. The older WMT path writes PROTECTEN 0x10001220 directly.
- `consys_co_clock_type()` reads MT6357 "DCXO_CW15/CW16" at 0x701e/0x7000. These are wrong addresses for MT6357 (real DCXO_CW15/16 are 0x7ae/0x7b0). The co_clock value used at power-on comes from the WMT config (log: "co-clock enabled"). We assume co-clock = 1 (VCN28 must be on). We did not verify where the WMT core gets the flag.
- MT6357 VCN18_EN/SW_OP_EN are written to 1 even on "off". So the regulator framework state is what really turns VCN18 off. With mainline mt6357-regulator, use only regulator_enable/disable plus the OP_EN/OP_CFG bit writes above (or leave them at reset values; not tested).
- The SMC ID and argument order for the EMI MPU depend on the ATF on this phone. We have not checked that an arm64 SMC32/SMC64 call is accepted. If the MPU region is not set, CONN DMA to EMI may fault, but the chip ID read does not need EMI.
- The mainline regulator names must map vcn33_bt/vcn33_wifi to the MT6357 VCN33 EN_0/EN_1 enables. Not checked.
- Not verified: whether the AP2CONN_OSC_EN (0x10001f00 bit 10, wakeup bit 9) needs setting. The stock driver touches bit 9 only in force_trigger_assert_debug_pin.
