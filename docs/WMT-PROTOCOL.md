# WMT/STP host protocol over BTIF (MT6739 CONSYS, chip id 0x699)

Tags: [S] verified in gemian source (github.com/gemian/cosmo-linux-kernel-4.4 @ a741a2e, paths below are
under `drivers/misc/mediatek/connectivity/`). [B] verified in the lumi BTIF source (`ref/lumi-mt6739/drivers/misc/mediatek/btif`).
[K] byte array found in stock `wmt_drv.ko`. [L] stock `dmesg.txt`. [X] observed by a third party
(EchoMuse, ioctl trace of a stock launcher with the same patch names). [I] inferred, not verified.

Rule: the stock .ko and gemian differ in some places. Where they differ, use the stock bytes [K].

## 1. BTIF host setup (PIO) [B] btif_plat.c:486 hal_btif_hw_init
Base 0x1100c000, IRQ GIC_SPI 112 level-low, clock `CLK_INFRA_BTIF` (DT mt6739.dts:1365). Registers (+offset):
RBR/THR 0x00, IER 0x04, IIR(r)/FIFOCTRL(w) 0x08, FAKELCR 0x0c, LSR 0x14, SLEEP_EN 0x48, DMA_EN 0x4c,
RTOCNT 0x54, TRI_LVL 0x60, WAK 0x64, WAT_TIME 0x68, HANDSHAKE 0x6c.
1. FAKELCR = 0 (normal mode).
2. HANDSHAKE |= 1 (new handshake mode).
3. FIFOCTRL: set bit1 (CLR_RX), clear it; set bit2 (CLR_TX), clear it. (Vendor uses read-modify-write, which reads IIR. Plain writes 0x2, 0x0, 0x4, 0x0 are fine [I].)
4. TRI_LVL = TX(8) | RX(1)<<4 | LOOP_DIS = 0x18.
5. DMA_EN: clear TX (bit1) and RX (bit0) DMA, set AUTORST (bit2) -> 0x4.
6. IER: TXEEN (bit1) = 0, RXFEN (bit0) = 1 (or 0 if you poll).
SLEEP_EN, RTOCNT, WAT_TIME: never written (reset values). Stock uses DMA by default; PIO is a supported vendor mode (debug cmd 0xc).
TX (PIO): if LSR.TEMT (bit6) write up to 16 bytes to THR; else if LSR.THRE (bit5) write up to 8. RX: read RBR while LSR.DR (bit0).

BTIF_WAK: pulse = write WAK bit0 = 0, wait 128-160 us, write 1. It is called only by the STP power-save wakeup path
(wmt_core.c:1471 opfunc_pwr_sv WAKEUP, in BTIF full mode), after the host has sent WMT SLEEP. Not called at open or
before each transfer [S][B]. Stock answers the first command < 1 ms after power-on with no wakeup [L 39.5515 -> 39.5523].
If you never enable PSM (never send `01 03 01 00 01`), you never need WAK [I].

## 2. STP framing [S] core/stp_core.c
Packet = 4-byte header + payload + 2-byte CRC. Types (common_main/include/stp_exp.h:45): BT 0, FM 1, GPS 2, WIFI 3,
WMT 4, STP 5 (fw assert/ctrl), INFO 6, ANT 7, COEX 8. Delimiter 0x55 0x55 and NAK are disabled (statics = 0).
- MAND mode (stp_core.c:2622 TX, :1735 RX): `80 | (type<<4)|(len>>8) | len&0xff | 00`, payload, `00 00`.
  No seq, no ack, no checksum, CRC = 0. RX: byte0 must be exactly 0x80; CRC should be 0.
- FULL mode (stp_core.c:2658 TX, :2025 RX): byte0 = 0x80 | seq<<3 | ack (bit6 = 0), byte1 = type<<4 | len[11:8],
  byte2 = len[7:0], byte3 = (b0+b1+b2) & 0xff. CRC = CRC-16/ARC (poly 0xA001 reflected, init 0, linux/osal.c:322)
  over payload only, little-endian. Max length 2048 on RX.
  Seq/ack 3 bits, window 7, TX retry timer 180 ms. After enable: txseq 0, txack 7, rxack 7, expected_rxseq 0 (stp_rest_ctx_state :586).
  For every received data packet with seq == expected: set txack = seq, expected++, send ack-only header
  `80|txack, 00, 00, sum` (stp_send_ack :1078). Ack-only packets (len 0) from chip: process ack, no reply.
  Out-of-order seq: resend previous ack. Type 5/6 packets are fw assert text, no CRC check. Resync = 4x 0x7f.
- Mode choice [S]: wmt_core_stp_init (wmt_core.c:769) opens BTIF and sets BTIF_MAND + enable. The ROM talks MAND.
  `_wmt_soc_mode_ctrl` (wmt_ic_soc.c:1092) switches to FULL before patch download, because mt6739 has
  `pda_dl_patch_flag` = 0 (platform/mt6739.c:155, flag never set). Patch download then runs in FULL mode.
Worked example (FULL, first packet after switch): QUERY_STP `01 04 01 00 04` ->
`87 40 05 cc 01 04 01 00 04 6c f3`. Chip reply with seq 0, ack 0 [I]: `80 40 0a ca 02 04 06 00 00 04 df 0e 68 01 56 f7`,
host acks with `80 00 00 80`. MAND example: `80 40 05 00 01 04 01 00 04 00 00`.

## 3. Host sequence, power-on to Wi-Fi on
WMT PDU: `type(1=cmd,2=evt) opcode len16LE payload`. Vendor compares the full expected event (except opcode 0x14).
Reg cmd (opcode 8): `01 08 10 00 op(1=w,2=r) 01 00 01 addr32LE val32LE mask32LE`; write evt `02 08 04 00 00 00 00 01`;
read evt is 16 bytes, addr at [8..11], value at [12..15] ([4..7] not checked) (wmt_core.c:612).
Chip address 0x80000000 = AP 0x18070000 [I, matches 0x18070008 = 0x699].

MAND mode:
1. HW code: read 0x80000008 mask 0xffff: `01 08 10 00 02 01 00 01 08 00 00 80 00 00 00 00 ff ff 00 00`, value 0x0699 [S wmt_core.c:1004][L].
2. HW ver: read 0x80000000 -> 0x8b00; FW(ROM) ver: read 0x80000004 -> 0x8a00 [S :1989][L "0x8b00/0x8a00"].
3. QUERY_STP `01 04 01 00 04` -> `02 04 06 00 00 04 11 00 00 00` [S][K].
4. SET_STP `01 04 05 00 03 df 0e 68 01` -> `02 04 02 00 00 03` [K]. (gemian has 0xdb, stock has 0xdf. Meaning unknown.)
5. Host: mode FULL, reset seq state, sleep 10 ms.
FULL mode from here:
6. QUERY_STP -> `02 04 06 00 00 04 df 0e 68 01` [K].
7. DLM power on (wmt_ic_soc.c:725) [S][K]: three writes to 0x80100060, value 0, masks 0x00000f00, 0x000000f0, 0x00000008.
8. MCU clock up (set_mcuclk_table_1) [S][K]: write 0x80000334 = 0x00010000 (mask ffffffff); write 0x8000010c = 0x00844d59.
9. For each patch, in order ROMv2_lm_patch_1_1_hdr.bin then ROMv2_lm_patch_1_0_hdr.bin [L frag(320,748) then (180,959)][X]:
   a. `01 08 10 00 01 01 00 01 40 02 09 02 00 00 00 00 ff ff ff ff` (write 0x02090240 = 0) -> write evt [S :2797, byte 8 = 0x40 for 0x699].
   b. `01 08 10 00 01 01 00 01 c8 04 09 02 A0 A1 A2 A3 ff ff ff ff` (write 0x020904c8) -> write evt.
      A = 00 00 hdr[0x1a] hdr[0x1b] [X]: 1_1 -> `00 00 0a f0`, 1_0 -> `00 00 06 00`.
   c. Body = file minus 28-byte header (date[16] plat[4] hwver u16 swver u16 patchver u32). Fragments of 1000 bytes:
      `01 01 LL LL F` + data, LL = 1 + fragsize LE, F = 1 first, 2 middle, 3 last. Each -> `02 01 01 00 00`.
      1_1: 319748 B = 320 frags, last 748. 1_0: 179959 B = 180 frags, last 959. Stock: 485 ms and 227 ms.
   d. WMT_RESET `01 07 01 00 04` -> `02 07 01 00 00`. STP state is NOT reset by the host after this [S].
   Patch selection [I]: header HwVer/SwVer = 0x8a00 = ROM version (FVR). hdr[0x18] = 0x21 / 0x22 (high nibble = count 2,
   low nibble = sequence) matches the order. The launcher (userspace, not public) chooses; both files are used.
10. MCU clock down: write 0x8000010c = 0x00844d00; write 0x80000334 = 0 [S][K].
11. (gemian only, absent in stock [K]: SET_CHIP_ID `01 02 05 00 11 39 67 00 00`. Skip.)
12. LTE coex (coex_wmt_filter_mode = 0, chip 0x699), sleep 5 ms, then 3 cmds, each -> `02 10 01 00 00` [S :1385][K]:
    WMT_COEX_FILTER_SPEC_CMD_6752 (73 B, wmt_ic_soc.c:694), WMT_COEX_LTE_FREQ_IDX_TABLE_CMD_6752 (37 B), `01 10 02 00 15 01`.
13. bt_tssi_from_wifi = 0, coex_wmt_ant_mode_ex = 0 [L]: no command.
14. Host: VCN33_BT and VCN33_WIFI on. RF calibration `01 14 01 00 01` -> 6 bytes `02 14 02 00 00 01` (content not checked).
    Host: VCN33_BT/WIFI off (wifi probe re-enables WIFI), VCN28 off (co-clock) [S][L 41.23 -> 41.55, ~320 ms].
15. Coex from WMT_SOC.cfg (unset fields = 0), each -> `02 10 01 00 00` [S :919][K]:
    `01 10 02 00 06 01` (ant mode 1), `01 10 03 00 1a 00 00` (wifi path 0), `01 10 12 00 1b` + 17x `00`, `01 10 02 00 1d 00`.
16. co_clock_flag = 1: `01 0a 02 00 08 03` -> `02 0a 01 00 00` [S][K][L "co-clock enabled"].
17. FM strap (FM_COMM = 2): `01 05 02 00 02 02` -> `02 05 02 00 00 02` [S][K].
18. Coredump flag is 0 on stock [L]: skip `01 0f 07 00 04 ...`. PSM: vendor enables host PSM here; a minimal host can skip it [I].
19. Wi-Fi function on: NO WMT command for SoC Wi-Fi [S wmt_func.c:678, wmt_core.c:1244]. WMT calls the wlan probe directly.
    (BT on would be FUNC_CTRL `01 06 02 00 00 01` -> `02 06 01 00 00` [S :431].)
EMI: nothing is written to EMI before or during this sequence [S]. The host only sets the MPU and the remap
(INFRACFG 0x1380). The coredump area (EMI+0x80000) is cleared only on coredump.

## 4. Wi-Fi firmware load [S wlan/core/gen2]
wlan_drv_gen2 (AHB HIF) loads WIFI_RAM_CODE_6755 itself through the Wi-Fi HIF, not through WMT or EMI.
common/wlan_lib.c:380: parse "MTKW" divided-download header (sections dest/len/offset, CRC32), send each chunk
as INIT_CMD_ID_DOWNLOAD_BUF packets over the HIF TX port (AP_DMA "HIF_0" channel, os/linux/hif/ahb/mt6739/ahb_pdma.c),
wait for status, then INIT_CMD_ID_WIFI_START. Stock: 219 packets in 230 ms [L 58.659 -> 58.890].
ahb.c:1339 turns VCN33_WIFI to HW mode before this. The EMI MPU region 21 (0x77c00000, 512 KiB) is only an access permission.

## 5. First experiment
After CPU reset release: BTIF clock on, setup as in section 1, poll mode. Send in MAND mode
`80 40 14 00 01 08 10 00 02 01 00 01 08 00 00 80 00 00 00 00 ff ff 00 00 00 00` (20-byte read of 0x80000008).
Expected [I from the parser and RD_EVT size]: `80 40 10 xx 02 08 0c 00 00 .. .. .. 08 00 00 80 99 06 00 00 00 00`
(byte 3 and bytes 4..7 of the event not checked by the vendor). The value 0x0699 proves the link.
Then QUERY_STP: send `80 40 05 00 01 04 01 00 04 00 00`, expect `80 40 0a xx 02 04 06 00 00 04 11 00 00 00 00 00`.
If nothing comes back, check the PC: the ROM loop at 0x98d2 should leave it when bytes arrive [I].
