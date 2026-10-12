# CLDMA, the MD1 network HIF (MT6739, MD_GENERATION 6293)

Goal: start the AP CLDMA so that the modem can send and receive IP packets, then add a raw-IP netdev for ccmni0.
Only the `MD_GENERATION >= 6293` code paths are in this document.

Tags:
- **[S]** vendor source `ref/lumi-mt6739/drivers/misc/mediatek/eccci/` (`hif/ccci_hif_cldma.[ch]`, `mt6739/cldma_reg.h`, `port/port_net.c`, `port/port_cfg.c`, `port/port_proxy.c`, `port/port_rpc.c`, `../ccmni/ccmni.c`). File:line is given where it helps.
- **[L]** stock `logs/reference/wifi/dmesg.txt`.
- **[B]** stock `stock/stock.dts`.
- **[V]** strings in the stock `vendor.img` (`~/agm-m7-backup`), MTK RIL and netagent.
- **[H]** our hypothesis, not verified.

## 0. Summary

- 6293 has **4 TX queues and 1 RX queue** [S]. All network RX comes on RX queue 0. ccmni0 sends data on TX q0 and TCP ACKs on TX q1.
- **No CCCI header on the wire.** RX buffer = 4-byte `lhif_header` + IP packet. TX buffer = IP packet only; the ccmni index goes in byte 3 (`netif`) of the TGPD [S].
- RX: one ring of 512 RGPDs, each with a 3584-byte buffer, all HWO=1 at start. Write the ring address to AO+0x404, then PDN+0x404 = 0xF (start) [S].
- Our driver does only `cldma_reset` (AO+0x418, AO+0x400). It does not do `cldma_start`, it does not map PDN 0x1021b000, and it does not request IRQ 165. So the MD has no RX GPD to write to [H: this is why CCIF ch20 repeats and the MD asserts].

## 1. Blocks, clock, reset

| item | value | tag |
|---|---|---|
| CLDMA AO base ("AO") | 0x10014000, size 0x1000 | [B] reg[0] |
| CLDMA PDN base ("PDN") | 0x1021b000, size 0x1000 | [B] reg[1] |
| IRQ | GIC SPI 165 (0xa5), flags 0x04 = level high, name "CLDMA_AP", requested then disabled until `cldma_start` | [B] [S] `cldma_sw_init` |
| clock | `infra-cldma-bclk` = `CLK_INFRA_CLDMA_BCLK` = infracfg_ao INFRA2 CG bit 3 (set +0xa4, clr +0xa8, sta +0xac). Turned on by `ccci_set_clk_cg(md, 1)` before MD power-on | [S] `clk-mt6739.c:955`, `md_sys1_platform.c:235` |
| reset AO | infracfg_ao +0x140 = BIT(6) (assert), +0x144 = BIT(6) (release) | [S] `md_cldma_hw_reset` |
| reset PD | infracfg_ao +0x150 = BIT(2), +0x154 = BIT(2) | [S] |
| wakeup mask | infracfg_ao +0xc00 \|= BIT(1) (`CLDMA_IP_BUSY_MASK`, "set cldma wakeup source mask") | [S] |
| `md_cd_lock_cldma_clock_src` | empty on MT6739 (`spm_ap_mdsrc_req` is commented out) | [S] `md_sys1_platform.c:288` |
| `mediatek,cldma_capability` | 0x06 = `MODEM_CAP_TXBUSY_STOP` \| `MODEM_CAP_SGIO`. No NAPI | [B] [S] |

There is no `cldma_plat_set_clk_cg` in this tree. Our driver already does the clock, both resets and the 0xc00 bit [S vs our `mt6739_ccci_start`].

## 2. Registers used on 6293

All accesses are 32-bit. The vendor does a "dummy read" of the same register after each command write (to post the write).

### 2.1 AO (0x10014000)

| off | name | use |
|---|---|---|
| 0x004..0x010 | `UL_START_ADDR_BK_0..3` | copy of TX queue q start address (low 32), for power-down backup. Written in `cldma_start` |
| 0x014 | `UL_START_ADDR_BK_4MSB` | bits [35:32] of the backup address, 4 bits per queue at `q*4` |
| 0x018..0x028 | `UL_CURRENT_ADDR_BK_*` | debug only |
| 0x400 | `SO_CFG` | bit 0 = enable OUT (RX) DMA. 6293 writes only bit 0. Bit 2 (wait RGPD write response) and bit 6 (36-bit address) are written only for <= 6292. The fallback in `md_cd_ccif_allQreset_work` writes \|= 0x5 if bit 0 does not stick |
| 0x404 | `SO_START_ADDR_0` | RX q0 first RGPD address, low 32 bits |
| 0x408 | `SO_CURRENT_ADDR_0` | RX q0 current RGPD (read only, debug) |
| 0x40c | `SO_START_ADDR_4MSB` | RX start address bits [35:32], nibble `q*4` |
| 0x410 | `SO_CURRENT_ADDR_4MSB` | debug |
| 0x414 | `SO_STATUS` | bit q = RX queue q is running. 0 = stopped (it reached an HWO=0 RGPD) |
| 0x418 | `DL_MTU_SIZE` | = `NET_RX_BUF` = 3584 |
| 0x800 | `L2RIMR0` | RX L2 mask, read. Format "gathered": bit 0 done q0, bit 1 queue-empty q0, bit 2 error q0. 1 = masked |
| 0x804 | `L2RIMCR0` | RX mask clear (unmask) |
| 0x808 | `L2RIMSR0` | RX mask set (mask) |

**AO misc quirk (6293)** [S] `cldma_write32_ao_misc`: 0x804 and 0x808 act as plain RW registers. To write value `v` to one of them, first read the other one, clear the bits `v` in it, write it back, then write `v` to the target. Example, unmask: `r = rd(AO+0x808); wr(AO+0x808, r & ~v); wr(AO+0x804, v)`.

Bits [35:32] (all 4MSB registers and the GPD `msb` byte): the vendor writes them only with `CONFIG_ARCH_DMA_ADDR_T_64BIT`. The stock kernel is 32-bit ARMv7, so they were not written there. Our RAM is below 4 GiB (0x40000000 + 512 MiB), so write 0.

### 2.2 PDN (0x1021b000)

| off | name | use |
|---|---|---|
| 0x000..0x00c | `UL_START_ADDR_0..3` | TX queue q first TGPD address, low 32 |
| 0x010 | `UL_START_ADDR_4MSB` | nibble `q*4` |
| 0x014..0x020 | `UL_CURRENT_ADDR_0..3` | debug |
| 0x024 | `UL_CURR_ADDR_4MSB` | debug |
| 0x028 | `UL_STATUS` | bit q = TX queue q is running |
| 0x030 | `UL_START_CMD` | write queue bitmap. Vendor writes 0xF once, on the first TX after `cldma_start` |
| 0x034 | `UL_RESUME_CMD` | write `1 << q` to continue a stopped TX queue |
| 0x038 | `UL_STOP_CMD` | write 0xF to stop all TX queues (6293: no poll after it) |
| 0x03c | `UL_ERROR` | debug |
| 0x040 | `UL_CFG` | not written on 6293 (bit 6 = 36-bit is <= 6292 only) |
| 0x044..0x054 | `UL_LAST_UPDATE_ADDR_*` | debug |
| 0x400 | `SO_ERROR` | debug |
| 0x404 | `SO_START_CMD` | write 0xF in `cldma_start` (only q0 exists) |
| 0x408 | `SO_RESUME_CMD` | write `1 << q` after refill when `SO_STATUS` bit q is 0 |
| 0x40c | `SO_STOP_CMD` | write 0xF |
| 0x800 | `L2TISAR0` | TX L2 status, write 1 to clear. bits [3:0] done q0..3, [7:4] queue empty q0..3, [11:8] error q0..3 |
| 0x804 | `L2TIMR0` | TX L2 mask (1 = masked), same bit layout |
| 0x808 | `L2TIMCR0` | TX unmask (W1C of mask) |
| 0x80c | `L2TIMSR0` | TX mask (W1S) |
| 0x810, 0x814 | `L3TISAR0/1` | TX error detail, W1C. Vendor only prints them |
| 0x818..0x82c | `L3TIMR0/1`, `L3TIMCR0/1`, `L3TIMSR0/1` | TX L3 mask / unmask / mask |
| 0x830 | `L2RISAR0` | RX L2 status, W1C. **Scattered on 6293**: bit 0 done q0, bit 8 queue empty q0, bit 16 error q0 (bit `8*n + q`) |
| 0x834 | `L2RISAR1` | not used on 6293 |
| 0x838, 0x83c | `L3RISAR0/1` | RX error detail, W1C |
| 0x840..0x854 | `L3RIMR0/1`, `L3RIMCR0/1`, `L3RIMSR0/1` | RX L3 mask / unmask / mask |
| 0x858 | `AXI_MI` | dump only |
| 0x860 | `CLDMA_IP_BUSY` | wakeup/busy bits set by the MD side. Vendor clears it with `wr(PDN+0x860, rd(PDN+0x860))` in the RX ISR, in RX collect, and after TX done or TX queue-empty ("to avoid md can't sleep") |
| 0x864, 0x868 | `HW_DVFS_CTRL`, `QOS` | not used |
| 0x870, 0x874 | `DMA_ERR`, `DMA_ERR_MASK` | dump only |

There is no `BUS_CFG` (split enable) and no `DEBUG_ID_EN` on 6293 [S].

Vendor bit macros [S] `cldma_reg.h`: `TX_INT_DONE 0xF`, `TX_INT_QUEUE_EMPTY 0xF0` (offset 4), `TX_INT_ERROR 0xF00`; `RX_INT_DONE 0x1`, `RX_INT_QUEUE_EMPTY 0x2` (offset 1), `RX_INT_ERROR 0x4` (gathered format); `BM_ALL_QUEUE 0xF`, `BM_INT_ALL 0xFFFFFFFF`.

## 3. Descriptors

All are 16 bytes, from one `dma_pool` with size 16 and align 16 [S] `md_cd_late_init`. Little endian. The ring is a closed loop: the last descriptor points to the first. There is no dummy GPD and no tail register. The hardware runs until it finds HWO=0, then stops the queue (queue-empty IRQ, status bit goes to 0). Software sets HWO again and writes RESUME.

### 3.1 TGPD (TX)
| byte | field | value |
|---|---|---|
| 0 | `gpd_flags` | bit 0 HWO (1 = owned by HW), bit 1 BDP (data pointer is a TBD list), bit 7 IOC (IRQ on completion). Init 0x80 (GPD ring) or 0x82 (GPD+BD ring) |
| 1 | `non_used` | old checksum byte. Now debug: 1 = queued, 2 = done |
| 2 | `msb` | bits [3:0] data/BD pointer [35:32], bits [7:4] next pointer [35:32] |
| 3 | `netif` | **ccmni index** (5 bits) = `ccci_h->data[0]` |
| 4..7 | `next_gpd_ptr` | low 32 bits |
| 8..11 | `data_buff_bd_ptr` | low 32 bits of the IP packet (or of the first TBD) |
| 12..13 | `data_buff_len` | bytes. The BD path writes it with a 32-bit write, so bytes 14..15 become 0 |
| 14..15 | `psn` | packet sequence number. Never set (0) |

### 3.2 RGPD (RX)
| byte | field | value |
|---|---|---|
| 0 | `gpd_flags` | 0x81 = IOC \| HWO when given to HW. HW clears bit 0 when done |
| 1 | `non_used` | 0 |
| 2..3 | `data_allow_len` | 3584 (ring `pkt_size`), set once at init |
| 4..7 | `next_gpd_ptr` | low 32 |
| 8..11 | `data_buff_bd_ptr` | low 32 of the buffer |
| 12..13 | `data_buff_len` | 0 when given; HW writes the received length |
| 14 | `msb` | as TGPD byte 2 |
| 15 | `non_used2` | 0 |

### 3.3 TBD / RBD
TBD: byte 0 `bd_flags` bit 0 EOL; byte 1 debug (1/2); byte 2 `msb`; 4..7 `next_bd_ptr`; 8..11 `data_buff_ptr`; 12..13 `data_buff_len`. RBD has the RGPD layout. RBD is never used.

### 3.4 What 6293 uses
| ring | type | count | buffer |
|---|---|---|---|
| net RX q0 | plain RGPD (`RING_GPD`) | 512 | 3584 (`NET_RX_BUF` = `SKB_4K` = `CCCI_MTU` + 128 = 3456 + 128) |
| net TX q0..q3 | GPD + BD (`CLDMA_NET_TX_BD` is defined), `MAX_SKB_FRAGS + 2` TBDs per TGPD | 256 each | skb head + frags |

A plain TGPD with a direct data pointer (`cldma_gpd_handle_tx_request`, flags 0x80) is also valid on 6293 [S]. Use it for a first driver.

**Checksum:** none. No code computes a GPD checksum on 6293. Byte 1 is debug only [S].

## 4. Queues and channels

`CLDMA_TXQ_NUM 4`, `CLDMA_RXQ_NUM 1`; all queues are net queues (`NET_TX_QUEUE_MASK 0xF`, `NET_RX_QUEUE_MASK 0x1`); RX q0 is handled in a hi tasklet [S].

Port table [S] `port_cfg.c` (6293: `DATA_TX_Q 0`, `DATA_RX_Q 0`, `DATA_TX_ACK_Q 1`, `DATA2_Q 2`, `DATA_MDT_Q 0`):
| port | ccmni idx (= netif) | CCCI RX / TX ch (software only) | TX q | ACK TX q | RX q |
|---|---|---|---|---|---|
| ccmni0 | 0 | 20 / 22 | 0 | 1 | 0 |
| ccmni1 | 1 | 24 / 26 | 0 | 1 | 0 |
| ccmni2..6 | 2..6 | 28/30, 69/71, 74/76, 79/81, 84/86 | 2 | 2 | 0 |
| ccmni7 | 7 | 89 / 91 | 0 | 1 | 0 |
| ccmni9..20 | 9..20 | 103/105 ... | 2 | 2 | 0 |
| ccmni-lan | 21 | 96 / 98 | 0 | 0 | 0 |

The CCCI channel numbers are used only inside the AP driver. On 6293 RX dispatch maps `lhif_header.netif` -> ccmni index -> port [S] `proxy_dispatch_recv_skb`.

ACK queue: for ccmni0/1/7 (`multiq` = 1 on MD1), a TCP packet with no SYN/FIN/RST, no payload and IP total length <= 112 bytes goes to the ACK queue [S] `ccmni.c:94 is_ack_skb`.

RPC 0x400D (`IPC_RPC_CCCI_LHIF_MAPPING`) [S] `port_rpc.c:503`: argument `struct { u32 net_if; u32 lhif_q; }`. `lhif_q` 2 (`LHIF_HWQ_AP_UL_Q0`): TX q0, ACK q1. `lhif_q` 3 (`LHIF_HWQ_AP_UL_Q1`, IMS): TX q3, ACK q3. Reply: two u32 zero. Our driver already answers 0/0 but does not store the map.
[L] 104.450 `remap port ccmni0 Tx to cldma0`, 6 ms later `ccmni0_Open`. 119.533 `remap port ccmni2 Tx to cldma3` (IMS), then `ccmni2_Open`. So the MD sends 0x400D for each PDN before the AP brings the interface up.

[L] traffic: `ccmni0 rx=(21 pkts, 5063 B)`, `tx=(29, 21 on data qdisc, 8 on ACK qdisc)`, `net Txq0-3 (status=0x0): 21-21, 8-8, 0-0, 0-0`, flags 0x81 (UP, NOARP). This confirms TX q0 for data and TX q1 for ACKs.

## 5. Sequences (vendor, 6293)

### 5.1 Boot order [S] `md_cd_start`
1. First boot only: `md_cd_late_init` (build rings, request IRQ, disable it).
2. `ccci_set_clk_cg(1)` (CLDMA bclk and CCIF clocks on).
3. `md_cd_clear_all_queue(OUT)`, `(IN)`: TX all HWO=0, length 0; RX all flags 0x81, length 0.
4. CCIF reset, `md_cldma_hw_reset` (section 1).
5. MD power on, `md_cd_let_md_go`, enable WDT and CCIF IRQ.
6. `cldma_reset`, then `cldma_start`. [L] 38.3179 both, HS1 at 38.3755.

So the vendor starts CLDMA after the MD is released, before HS1.

### 5.2 `cldma_reset`
1. AO+0x418 = 3584.
2. AO+0x400 \|= 0x1.

### 5.3 `cldma_start`
1. `enable_irq(165)`.
2. For TX q = 0..3: PDN+0x000+4q = first TGPD; PDN+0x010 nibble q = bits [35:32]; AO+0x004+4q = same address; AO+0x014 nibble q.
3. RX q0: AO+0x404 = first RGPD; AO+0x40c nibble 0.
4. `wmb()`.
5. `txq_started = 0`; PDN+0x404 = 0xF; read PDN+0x404.
6. PDN+0x808 = 0xFFF (unmask TX done, empty, error).
7. AO misc unmask RX: `wr(AO+0x808, rd(AO+0x808) & ~0x7); wr(AO+0x804, 0x7)`.
8. PDN+0x820 = PDN+0x824 = PDN+0x848 = PDN+0x84c = 0xFFFFFFFF (unmask all L3).

TX is not started here. On the first send, PDN+0x030 = 0xF; later sends write PDN+0x034 = `1 << q` if PDN+0x028 bit q is 0.

### 5.4 TX one packet [S] `md_cd_send_skb`, `cldma_gpd_handle_tx_request`
1. Take `tx_xmit` TGPD (must have no skb, budget > 0).
2. Map the IP packet (`DMA_TO_DEVICE`); write data pointer, `data_buff_len`, `netif` = ccmni index, byte 1 = 1.
3. Set HWO (`flags |= 1`) last, under the lock. Advance `tx_xmit`.
4. Start or resume the queue (5.3).
TX done (work, 1 jiffy delay for net queues): from `tr_done` while HWO = 0 and skb set: unmap, free, advance; resume the queue if an HWO=1 TGPD waits and `UL_STATUS` bit q is 0; clear `IP_BUSY`. Then if L2TISAR0 still has done for q: ack it and run again; else unmask PDN+0x808 = `(1<<q) | (1<<(q+4))`.

### 5.5 ISR [S] `cldma_irq_work_cb`
1. Read `t = PDN+0x800`, `r = PDN+0x830`, `tm = PDN+0x804`, `rm = AO+0x800`.
2. `t &= ~tm`. `r = gather(r) & ~rm` (bit `8n` -> bit `n`); `r_reg = scatter(r)`.
3. Errors: print L3TISAR0/1 or L3RISAR0/1. L3 is not cleared here.
4. TX: PDN+0x800 = t. For each q with done: mask PDN+0x80c = `(1<<q) | (1<<(q+4))`, queue TX-done work. For each q with empty: ack PDN+0x800 = `1 << (q+4)`; resume if an HWO=1 TGPD waits; else, if `UL_STATUS` bit q is 0 and `IP_BUSY` != 0, clear `IP_BUSY`.
5. RX: PDN+0x830 = r_reg; `wr(PDN+0x860, rd(PDN+0x860))`. If done or empty for q0: AO misc mask 0x3 (`wr(AO+0x804, rd(AO+0x804) & ~0x3); wr(AO+0x808, 0x3)`), read AO+0x808, schedule the RX tasklet.

### 5.6 RX collect [S] `cldma_gpd_rx_collect` (budget 512 or 2 jiffies)
For `req = tr_done` while `flags & 1 == 0` and the request has a buffer:
1. Allocate a new 3584-byte buffer. Unmap the old one. Length = `data_buff_len`.
2. Give the buffer up (section 6). Set data pointer 0, advance `tr_done`.
3. Refill `rx_refill` (the same slot): map the new buffer (`DMA_FROM_DEVICE`), write data pointer, `data_buff_len` = 0, then flags = 0x81. Advance `rx_refill`.
4. If AO+0x414 bit 0 is 0: PDN+0x408 = 0x1, read back.
After the loop: resume again if needed. Read PDN+0x830; if done bit 0 is set (and not over budget): write PDN+0x830 = BIT(8), then BIT(0), clear `IP_BUSY`, loop again. At the end unmask RX: AO misc unmask 0x3.

### 5.7 `cldma_stop` [S]
1. PDN+0x038 = 0xF, read back. PDN+0x40c = 0xF, read back. (6293 does not poll the status.)
2. Clear status: PDN+0x800, 0x830, 0x810, 0x838, 0x814, 0x83c = 0xFFFFFFFF.
3. Mask all: PDN+0x80c = ~0; AO misc L2RIMSR0 = ~0; PDN+0x828, 0x850, 0x82c, 0x854 = ~0.
4. `disable_irq`, flush the works.
On MD stop (`md_cldma_clear`): first poll up to 5 x 20 ms for AO+0x414 & 0xF == 0 and PDN+0x860 == 0, then stop, clear the queues and do the hardware reset.

### 5.8 Exception flow [S] `md_cd_exception`
`HIF_EX_INIT`: `cldma_stop_for_ee` (6293: `NONSTOP_QUEUE_MASK 0xFF`, so all queues stop), clear TX rings, send CCIF `H2D_EXCEPTION_ACK`. `HIF_EX_CLEARQ_DONE`: wait 2 s, `cldma_stop`, `md_cldma_hw_reset`, clear RX ring, then CCIF `CLEARQ_ACK`. `HIF_EX_ALLQ_RESET`: `cldma_reset` + `cldma_start`.

### 5.9 CCIF interplay
- CCIF ch 20 (`AP_MD_PEER_WAKEUP` = `RINGQ_EXP_BASE` + 5): the vendor ISR only takes a 1 s wake lock [S] `modem_sys1.c:263`. No register action.
- `ID_GET_MD_WAKEUP_SRC` (from SPM) sets a flag so the next CLDMA RX/TX logs "wakeup source" [S]. Debug only.
- CLDMA does not use CCIF to start. The MD sees the AP state through the CLDMA registers [H].

## 6. Packet format

### 6.1 RX (MD -> AP), RX q0 [S] `cldma_gpd_rx_collect`, `port_net_recv_skb`
| offset | size | field |
|---|---|---|
| 0 | 2 | `pdcp_count` |
| 2 | 1 | bits [3:0] `flow`, [6:4] `f`, [7] reserved |
| 3 | 1 | bits [4:0] `netif` = ccmni index, [7:5] `nw_type` |
| 4 | `data_buff_len` - 4 | IP packet |

`data_buff_len` includes the 4-byte header. The vendor rejects a `netif` with no ccmni port (drop). Max frame = 3584.

### 6.2 TX (AP -> MD)
Buffer = IP packet only. The ccmni driver adds a 16-byte `ccci_header` (`data[0]` = ccmni index, `data[1]` = length, `channel` = TX channel 22 for ccmni0); the HIF removes it (`skb_pull`) and copies `data[0]` into TGPD byte 3 [S] `cldma_gpd_handle_tx_request`.

### 6.3 ccmni netdev [S] `ccmni.c`, `ccmni.h`
- MTU 1500 (`CCMNI_MTU`), larger values are refused. Runtime data feature 20 (`AP_CCMNI_MTU`) = 3568 tells the MD the AP buffer size.
- Pure IP, `IFF_NOARP`, no broadcast or multicast. Version nibble: `data[0] & 0xF0` = 0x40 IPv4, 0x60 IPv6. The vendor puts a fake Ethernet header in the headroom only for its own stack.
- No hardware checksum: `ip_summed = CHECKSUM_NONE`.

### 6.4 Which ccmni carries the default bearer
- The kernel learns it only through RPC 0x400D (`net_if`, `lhif_q`) and through `netif` in each RX header [S] [L].
- The 6293 RIL activates a PDN with `AT+EAPNACT=1,"<apn>","<type>",<n>` and gets a `transIntfId`; it reads data with `AT+CGCONTRDP` and `AT+EPDN=<id>,"addr"`. The netdev name is `ccmni<transIntfId>` [V]. The MD sends `+EIF:` URCs and the netagent answers `AT+EIF=<id>,"ifst",<state>,<addrtype>`, `AT+EIF=<id>,"ipupdate",...` [V]. `+CGEV: ME PDN ACT` also comes [V]. `AT+CGDATA="M-CCMNI",<cid>,1` is in the MD image [V md1img], but the 6293 RIL does not use it [V].
- [L] the first data PDN was ccmni0 (lhif_q 2), IMS was ccmni2 (lhif_q 3).
- [H] With plain `AT+CGATT=1` + `AT+CGACT=1,1` the MD will most likely use ccmni0. Read the index from the 0x400D argument or from `netif`.

## 7. Minimal mainline plan

### 7.1 DT
Add to the `modem@10014000` node: reg `<0 0x1021b000 0 0x1000>` named `"cldma-pdn"`, and interrupt `<GIC_SPI 165 IRQ_TYPE_LEVEL_HIGH>` named `"cldma"` [B].

### 7.2 Step 1: make the MD happy (RX ring running)
1. At probe: `dma_set_mask_and_coherent(32)`. Allocate one coherent block: 4 TX rings of N TGPDs + 1 RX ring of M RGPDs (16 bytes each, 16-byte aligned). Start with M = 64, N = 64 (vendor: 512 and 256).
2. RX: M buffers of 3584 bytes (page_pool or `netdev_alloc_skb` + `dma_map_single`). Each RGPD: flags 0x81, allow_len 3584, data pointer, next pointer, msb 0. Last next = first.
3. TX rings: flags 0x80, HWO 0, linked in a loop, no buffers.
4. After `MD_CFG_BOOT_SLAVE` = 1 (MD released), and before HS1: `cldma_reset` (already there), then `cldma_start` exactly as 5.3, with 4MSB registers = 0.
5. IRQ handler as 5.5, a NAPI poll (or a threaded IRQ) as 5.6. For step 1, count and drop the packets, and log `netif` and the first bytes.
6. RPC 0x400D: store `lhif_q` per `net_if` (q0/ACK q1 for 2, q3 for 3); keep the 0/0 reply.
7. Test: `AT+CGATT=1`. Expected: no ch20 storm, no ERRC assert, `+CGEV` URCs. If it still asserts, dump AO+0x414, PDN+0x830, PDN+0x860 and the RGPD ring.

### 7.3 Step 2: ccmni0 as a WWAN netdev
- `wwan_register_ops(dev, &ops, md, 0)` on the same `wwan_device` parent as the AT port. Link id = `netif` (default link 0 = ccmni0). `ops->setup`: `dev->type = ARPHRD_RAWIP`, `IFF_POINTOPOINT | IFF_NOARP`, MTU 1500, `max_mtu` 1500, `needed_headroom` 0. The mainline `t7xx` driver (`drivers/net/wwan/t7xx/`) is a good code model, but its CLDMA registers are different (8+8 queues, other offsets); do not copy offsets.
- RX: `skb_pull(4)`, set `skb->protocol` from the version nibble, `ip_summed = CHECKSUM_NONE`, find the link by `netif`, `napi_gro_receive`. Refill and resume as 5.6.
- TX (`ndo_start_xmit`): linearize, map, fill TGPD (netif = link id, length), set HWO last (`dma_wmb()` before it), then start (first time, PDN+0x030 = 0xF) or resume (PDN+0x034). Queue: link's TX q (from 0x400D, default q0). The ACK queue split is an optimisation; skip it at first. Stop the netdev queue when the ring is full; wake it on TX done.
- Bearer setup from userspace: `AT+CGDCONT=1,"IP","<apn>"`, `AT+CGACT=1,1` (or `AT+EAPNACT`), read the address with `AT+CGCONTRDP=1`, then `ip addr add`/`ip link set wwan0 up`. The MD may wait for `AT+EIF` confirmations [V]; test without them first [H].

### 7.4 Open points
- `IP_BUSY` bit meaning per queue is not in the source.
- L3 status bits are not decoded in the source.
- [H] the ERRC assert after the ch20 storm comes from the stopped AP RX queue (the MD cannot deliver DL data or signalling for the PS bearer). Not proven; step 1 tests it.
