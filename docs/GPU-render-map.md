# DDK 1.9 GE8100 firmware: TA / 3D kick register map

Source: `/Volumes/Projects/agm-m7-work/out/rgx.fw.elf` (rgxfw_ta.c, rgxfw_3d.c, rgxfw_pm.c line info).
All register offsets are relative to 0xCF400000. "u" = written unconditionally on every kick that
reaches the register-programming stage (that is, after `rgxfw_pm_setup_context` returned 0).
"lo" = only the low 32 bits of the 64-bit command field are written.

## 0. Naming confidence

The public headers (`pvr_rogue_cr_defs.h`, `pvr_rogue_cr_defs_client.h`, 1.9 `rgx_cr_defs_km.h`)
do not define most TA/3D register offsets. Names below come from three sources:

* **[H]** offset is defined in a header: TE_AA 0xC00, TE_MTILE1 0xC08, TE_MTILE2 0xC10,
  TE_SCREEN 0xC18, PPP_SCREEN 0xC98, ISP_MTILE_SIZE 0xF18, ISP_RENDER 0xF08, ISP_CTL 0xF38,
  VDM_CONTEXT_STORE/RESUME_TASK0..2 0x438..0x460, PM_PARTIAL_RENDER_ENABLE 0x338,
  BIF_PM_CAT_BASE_* 0x1248..0x1280, RASTERISATION_INDIRECT 0x8238, EVENT_STATUS 0x130,
  EVENT_CLEAR 0x138, CLK_CTRL 0x0, PERF_TA_CYCLE 0x6020.
* **[D]** inferred: the FW copies a DWARF-named command/HWRT field (for example
  `uTAReg_TE_PSGREGION_ADDR`) straight into that offset. Every [H] anchor agrees with this
  method (HWRT.ui32TEScreen -> 0xC18, HWRT.ui32PPPScreen -> 0xC98, cmd ISP_MTILE_SIZE -> 0xF18,
  cmd ISP_CTL -> 0xF38), so [D] names are high confidence.
* **[?]** unnamed. Described by behaviour only.

## 1. Pointer conventions

* `rgxfw_ta_kickta(a0 = RGXFWIF_FWCOMMONCONTEXT*)`:
  `s0 = (ctx->psCCB & ~3) + ((ctx->psCCBCtl & ~3)->ui32ReadOffset)` = CCB command header
  (c0004296..c00042ba). Command field N is at `s0+56+N`.
  `s1 = cmd.sHWRTData & ~3` (`lw s1,280(s0)` c00042c0). `s2 = ctx->psFWMemContext & ~3`.
  `s4 = ctx->psContextState & ~3` = `RGXFWIF_TACTX_STATE` (c0004384).
* `rgxfw_3d_kick3d(a0 = ctx, a1, a2 = mode)`: `s0 = header + 56` (`addiu s0,v0,56` c00017f6),
  so command fields are at `s0+N` directly. For a partial render (`a2 == 2`, from
  `rgxfw_pm_execute_pr` c000ef04 and c000377e), `s0 = a1[28]` (c0002118). `s5 = a2 bit1`
  (partial render, "PR"). `s1 = cmd.sHWRTData & ~3`, `s6 = HWRT.psRTACtl & ~3`,
  `s4 = memctx`, `108(sp) = ctx->psContextState` = `RGXFWIF_3DCTX_STATE`.
* `RGXFWIF_TACTX_STATE` (64 B): 0 VDM_CALL_STACK_POINTER, 8 VDM_CALL_STACK_POINTER_Init,
  16 VDM_BATCH, 24..48 VBS_SO_PRIM0..3, 56 ui16TACurrentIdx.
* `asTAState[2]` element (48 B): STORE_TASK0, STORE_TASK1, STORE_TASK2, RESUME_TASK0,
  RESUME_TASK1, RESUME_TASK2 (DWARF 0x19c00).
* `RGXFWIF_RTDATA_STATE`: 0 NONE, 1 KICKTA, 2 KICKTAFIRST, 3 TAFINISHED, 4 KICK3D,
  5 3DFINISHED, 6 TAOUTOFMEM, 7 PARTIALRENDERFINISHED, 8 HWR.

## 2. RGXFWIF_CMDTA (280 B) field -> register

| off | field | register | how | cond | evidence |
|---|---|---|---|---|---|
| 0 | PM_MTILE_ARRAY | 0x240 [D] | raw 64-bit | u | lwp a0,56(s0) c0004388 -> sw 576/580 c000438c/90 |
| 8 | PPP_CTRL | 0xC88 [D] | lo | u | lw v1,64(s0) c00043ca/c00048d2 -> sw 3208 c00043da |
| 16 | PPP_WCLAMP | 0xC90 [D] | 64 | u | lwp a0,72(s0) c00043d2 -> sw 3216/3220 |
| 24 | TE_PSG | 0xC28 [D] | lo, no flags ORed | u | lw a2,80(s0) c0004408 -> sw 3112 c0004410 |
| 32 | TE_PSGREGION_ADDR | 0xC38 [D] | raw 64-bit | u | lwp a0,88(s0) c000440c -> sw 3128/3132 |
| 40 | VDM_CTRL_STREAM_BASE | 0x408 [D] | 64 | u, except on resume (below) | lwp a0,96(s0) c000443c -> sw 1032/1036 c0004460/64 |
| 48 | VDM_CTRL_STREAM_TERMINATE | - | not read by kickta | - | no load of s0+104 in kickta |
| 56 | VDM_SYNC_PDS_DATA_BASE | 0x470 [D] | lo | u | lw a2,112(s0) c0004468 -> sw 1136 c0004470 |
| 64 | TA_RTC_ADDR_BASE | 0xCB0 [D] | 64 | if != 0 (else 0xCB0 = 0) | lwp a0,120(s0) c0004488 -> sw 3248/3252 c00048ee |
| 72 | TPU_BORDER_COLOUR_TABLE | 0x17A0 [D] | 64 | u | lwp a0,128(s0) c000446c -> sw 6048/6052 |
| 80 | TPU_YUV_CSC_COEFFICIENTS | 0x1788 [D] | 64 | only if != 0; also sets global flag (44(gs) \|= 4) | c000447c, c00048da..e8 |
| 88 | TPU | 0x1780 [D] | lo | u | lw a3,144(s0) c000448c -> sw 6016 c0004498 |
| 96 | TPU_ARRAYS_VDM | - | not read | - | no load of s0+152 |
| 104 | VDM_CONTEXT_STATE_BASE_ADDR | 0x428 [D] | 64 | u | lwp v0,160(s0) c0004538 -> sw 1064/1068 |
| 104 | (same) | 0x408 | 64, overrides VDM_CTRL_STREAM_BASE | only when ctx->ui32Flags bit0 (TA NEED_RESUME) | c0004986 -> sw 1032/1036 c0004990/c000499c |
| 112 | TA_CONTEXT_STATE_BASE_ADDR | 0xCC0 [D] | 64 | u | lwp v0,168(s0) c0004566 -> sw 3264/3268 |
| 120+48*i | asTAState[i].STORE_TASK0 | 0x438 [H] | 64 | u, i = TACTX.ui16TACurrentIdx | c0004572 -> sw 1080/1084 |
| +8 | STORE_TASK1 | 0x440 [H] | lo | u | c000457e -> sw 1088 |
| +16 | STORE_TASK2 | 0x448 [H] | 64 | u | c0004582 -> sw 1096/1100 |
| +24 | RESUME_TASK0 | 0x450 [H] | 64 | u | c0004592 -> sw 1104/1108 |
| +32 | RESUME_TASK1 | 0x458 [H] | lo | u | c000459e -> sw 1112 |
| +40 | RESUME_TASK2 | 0x460 [H] | 64 | u | c00045a2 -> sw 1120/1124 |
| 216 | ui32Flags | - | bit 0x100 (TA_ABORT): abort path via rgxfw_pm_setup_ctx_check_rtdstate (c00042c4, c0004774). bit 0x2 (LASTKICK): HWRT.bHasLastTA = !!bit (c00045de..e8) and rgxfw_ta_rtcache_parse if RTC != 0 (c000492a). FIRSTKICK (0x1) is NOT used; "first" comes from HWRT.eState. | | |
| 220 | ui32FrameNum | - | log / hwperf only | | c0004a84, c0004ac8 |
| 224 | sHWRTData | - | HWRT pointer | | c00042c0 |
| 228..252 | sZBuffer, sSBuffer, sMSAAScratchBuffer, PR fences | - | not read by kickta | | |
| 256 | ui32MaxDeadlineMS | - | rgxfw_hwr_setup_deadlines arg | | lw a1,312(s0) c0004642 |
| 264 | sDummyRgnHdrInitTARegs.TE_PSGREGION_ADDR | 0xC38 | 64 | only in BRN dummy-region-header init, once (global flag at gs[1784]+64) | lwp t4,320(s0) c00047a2 -> sw 3128/3132 c00047b4/ba |
| 272 | ui32BRN61484LiveRT | - | not read by kickta | | |

TA resume index (c0004528..c00045b0): if `ctx->ui32Flags & 1`, `idx = 1 - TACTX.ui16TACurrentIdx`,
store back (sh c00049b0); else `idx = TACTX.ui16TACurrentIdx`. The same `idx` selects both
STORE and RESUME task words.

## 3. RGXFWIF_CMD3D (664 B) field -> register

`CurRT` = RTACtl.ui32CurrentRenderTarget (s6[4]). "PR" = partial render kick.
"priorPR" (96(sp), c000181a..c00018be) = `RTACtl.sNumRenders[CurRT] != 0` if sNumRenders is set,
else `HWRT.ui32NumPartialRenders != 0`.

| off | field | register | how | cond | evidence |
|---|---|---|---|---|---|
| 0 | USC_PIXEL_OUTPUT_CTRL | 0x4070 [D] | lo | u | lw t2,0(s0) c000190c -> sw 16496 c0001932 |
| 0 | (same) | 0xFD8 [?] | `min(count of trailing 1s of bits[14:3] (partition_mask), 2)`; preceded by 0x8238 = 0 | u | rgxfw_update_tiles_in_flight c000170a..c0001754 |
| 8..32 | USC_CLEAR_REGISTER0..3 | 0x4078, 0x4080, 0x4088, 0x4090 [D] | lo | u | c0001912..c0001942 |
| 40..64 | USC_CLEAR_REGISTER4..7 | - | not read | | |
| 72 | ISP_MTILE_BASE | 0xF20 [D] | 64, `+ CurRT * ALIGN(ISP_RGN.lo * 5, 4096)` | u | c00018f8..c0001964 |
| 80 | ISP_MTILE_SIZE | 0xF18 [H] | lo | u | c0001968 -> sw 3864 |
| 88 | ISP_GRIDOFFSET | 0xFA0 [D] | lo | u | c0001970 -> sw 4000 |
| 96 | ISP_MULTISAMPLECTL | 0xFA8 [D] | 64 | u | c000196c -> sw 4008/4012 |
| 104 | ISP_SCISSOR_BASE | 0xFB0 [D] | 64 | u | c0001984 |
| 112 | ISP_DBIAS_BASE | 0xFB8 [D] | 64 | u | c0001990 |
| 120 | ISP_BGOBJDEPTH | 0xF80 [D] | lo | u | c000199c -> sw 3968 |
| 128 | ISP_BGOBJVALS | 0xF88 [D] | lo, `\|= 0x200` (ENABLEBGTAG) if priorPR | u | c0001c6e, c00028a4, sw 3976 c0001d14 |
| 136 | ISP_AA | 0xF30 [D] | lo | u | c00019a4 -> sw 3888 |
| 144 | ISP_OCLQRY_BASE | 0xFC0 [D] | 64 | u | c00019a0 -> sw 4032/4036 |
| 152 | ISP_ZLSCTL | 0xF48 [D] | 64, modified (see 3.1) | u | c0001c36, sw 3912/3916 c0001d04 |
| 160 | ISP_MASK_LOAD_BASE | 0xF70 [D] | 64 | u | c00019bc |
| 168 | ISP_MASK_STORE_BASE | 0xF78 [D] | 64 | u | c00019c8 |
| 176 | ISP_ZLOAD_BASE | 0xF50 [D] | 64, `+ CurRT * ui32ZLSStride`; if priorPR and DEPTH: replaced by raw ISP_ZSTORE_BASE (no CurRT offset) | u | c0001c02..c0001c68, c0002840, sw 3920/3924 |
| 184 | ISP_ZSTORE_BASE | 0xF58 [D] | 64, `+ CurRT * ui32ZLSStride` | u | c0001c06, sw 3928/3932 |
| 192 | ISP_STENCIL_LOAD_BASE | 0xF60 [D] | 64, `+ CurRT * ui32SLSStride`; if priorPR and STENCIL: replaced by raw ISP_STENCIL_STORE_BASE | u | c0001c12, c000285c/c00029de, sw 3936/3940 |
| 200 | ISP_STENCIL_STORE_BASE | 0xF68 [D] | 64, `+ CurRT * ui32SLSStride` | u | c0001c16, sw 3944/3948 |
| 208 | ISP_ZLS_PIXELS | - | not read | | |
| 216 | ISP_RGN | 0xF28 [D] | lo | u | c00019d4 -> sw 3880; also used as MTILE_BASE stride |
| 224 | ISP_CTL | 0xF38 [H] | lo; bit 19 cleared (`& 0xFFF7FFFF`); bit 17 changed by PR logic (3.1) | u | c0001c92, c0001cde..c0001d10 |
| 232 | FB_CDC_ZLS | - | not read | | |
| 240, 248 | PM_3D_FSTACK_BASE, PM_3D_FSTACK | - | not read | | |
| 256 | EVENT_PIXEL_PDS_CODE | 0x618 [D] | lo | u | c00019d8 -> sw 1560 c0001a1e |
| 264 | EVENT_PIXEL_PDS_DATA | 0x620 [D] | lo | unless (PR and flag 0x80000) | c0001a26 |
| 272 | PR_EVENT_PIXEL_PDS_DATA | 0x620 | lo | only PR and flag 0x80000 | c0002706..c0002716 |
| 280 | EVENT_PIXEL_PDS_INFO | 0x628 [D] | lo | u | c0001a32 -> sw 1576 |
| 288+24*i | PBE_WORD0_MRTi | 0x1510 + 8*i [D] | 64 | u, i = 0..7 | c0001a4a..c0001bc6 |
| 296+24*i | PBE_WORD1_MRTi | 0x1550 + 8*i [D] | 64; lo `\|= (aui16ZSliceOffsets[i] + CurRT) << 14` when `PBE_WORD0_MRT0[33:32] == 2` (memlayout, 3D twiddled) | u | c0001a4e, c000221c..c000228c |
| 304+24*i | PBE_WORD2_MRTi | 0x15A0 + 8*i [D] | 64 | u | |
| 480 | PBE | 0x1500 [D] | lo | u | c0001bd2 -> sw 5376 |
| 488 | TPU_BORDER_COLOUR_TABLE | 0x1790 [D] | 64 | u | c0001bd6 -> sw 6032/6036 |
| 496 | TPU_YUV_CSC_COEFFICIENTS | 0x1788 [D] | 64 | only if != 0; sets global flag 44(gs) \|= 8 | c0001bea, c0002722 |
| 504 | TPU | 0x1780 [D] | lo | u | c0001bf6 -> sw 6016 |
| 512 | TPU_ARRAYS | - | not read | | |
| 520 | TPUMADDCtrl | 0x17C0 [D] | lo | u | c0001bfe -> sw 6080 |
| 528 | PDS_BGND0_BASE | 0x6A0 [D] | 64; PR_BGND0 if priorPR | u | c0001c60, sw 1696/1700 c0001d46 |
| 536 | PDS_BGND1_BASE | 0x6A8 [D] | 64; PR_BGND1 if priorPR | u | sw 1704/1708 |
| 544 | PDS_BGND2_BASE | 0x6B0 [D] | lo only; PR_BGND2 lo if priorPR | u | sw 1712 c0001d62 |
| 552 | PDS_BGND3_SIZEINFO | 0x6B8 [D] | 64; PR_BGND3 if priorPR | u | c0001c96 (lwp v1,548 -> a0/a1 = 552/556), sw 1720/1724 |
| 560..584 | PDS_PR_BGND0..3 | as above | used when priorPR (c0002884..c000289c); compared with BGND in PR (c000273e..c00027e2) | | |
| 592 | RGX_CR_BLACKPEARL_FIX | - | not read | | |
| 600 | ISP_DUMMY_STENCIL_STORE_BASE | 0xF68 | 64 | only in BRN dummy render (3.2) | c000232a -> sw 3944/3948 |
| 608 | ui32Flags | - | see 3.3 | | |
| 612 | ui32FrameNum | - | log only | | |
| 616 | sHWRTData | - | | | c00017fc |
| 620..628 | sZBuffer, sSBuffer, sMSAAScratchBuffer | - | not read by kick3d | | |
| 632 | ui32ZLSStride | - | per-RT byte stride for ZLOAD/ZSTORE | | c0001c1e |
| 636 | ui32SLSStride | - | per-RT byte stride for STENCIL_LOAD/STORE | | c0001c22 |
| 640 | aui16ZSliceOffsets[8] | - | PBE WORD1 slice add (above) | | c0002220.. |
| 656 | ui32MaxDeadlineMS | - | rgxfw_hwr_setup_deadlines | | c0001e62 |

### 3.1 ZLS / ISP_CTL rewrites (c0001ca2..c00028a4)

* PR kick (`s5`):
  * If not priorPR: set ISP_CTL bit 17 (PROCESS_EMPTY_TILES) if any PR_BGND word differs from
    BGND, or if flag 0x80000 (c000273e..c00027e2).
  * Flag DEPTHBUFFER (0x80): ZLSCTL lo `|= 0x80004` (ZSTOREEN, FORCEZSTORE). With
    STENCILBUFFER (0x100) too: `|= 0xC0004` (adds SSTOREEN). Only STENCIL: `|= 0x40004`.
    Sets `s7 = 32` (bit 5 of 0x6D0). (c0002758..c0002794, c00028e6, c0002a2e)
* priorPR (both PR and normal): DEPTH: ZLSCTL lo `|= 0x8002` (ZLOADEN, FORCEZLOAD) and
  ZLOAD = ISP_ZSTORE_BASE raw. STENCIL: `|= 0x4002` (SLOADEN) and STENCIL_LOAD = STENCIL_STORE
  raw. Then ISP_CTL bit 17 = (flag 0x80000 and not PR and HWRT.bLastWasPartial), else cleared.
  BGND -> PR_BGND, BGOBJVALS `|= 0x200`. (c0002836..c00028a4, c000290e, c00029d0)
* No PR and no priorPR: ZLSCTL, ISP_CTL (except bit 19), BGOBJVALS, BGND used as given.
* 0x6D0 [?] = `(old & 0x1F) | s7`, `s7 = 96` if flags & 0x8000 (DISABLE_PIXELMERGE), else 32 for
  PR with Z/S store, else 0 (c0001cd6 movn, c0001d38..c0001d42).

Note: the priorPR load addresses do not get the `CurRT * stride` offset while the store addresses
do. For layered depth (CurRT > 0) with partial renders this looks inconsistent. Unclear if that
is intended.

### 3.2 BRN dummy render (c00022fa..c0002628)

Runs once when global gs[1804] != 0 and (ZLSCTL & 0xC002 or (priorPR and flags & 0x180)).
Writes ISP_MTILE_SIZE = 0x10001, 0xF10 = 0, ISP_CTL = 0x20000, ISP_AA = 0, 0x4070 = cmd[0],
ISP_RENDER = 3, EVENT_PIXEL_PDS_CODE/DATA/INFO from cmd, STENCIL_STORE = ISP_DUMMY_STENCIL_STORE_BASE,
ZLSCTL = 0x40005, PBE words masked (`WORD0 lo & 0xFFF0003F | 0x2000`, `WORD1 lo & ~0x3FFF | 0x81`,
`WORD1 hi & 0xF0000000 | 0x200081`), BGOBJVALS = 0, BGND0..3 = 0, 0x388 = 0, 0xF00 = 1, polls
EVENT_STATUS (0x130) for 0x4008, writes EVENT_CLEAR (0x138) = 0x4008, clears gs[1804], then does
the normal kick.

### 3.3 3D flags (cmd 608) used

0x2 ABORT (pm_free_3d_context, eState = 5, no kick, c00018e4/c0001ec8), 0x80 DEPTHBUFFER,
0x100 STENCILBUFFER, 0x2000 SECURE (memctx activate arg, c0001864), 0x4000 ABORT_NOFREE
(rgxfw_pm_setup_ctx_check_rtdstate, c0001812/c0002140), 0x8000 DISABLE_PIXELMERGE,
0x80000 (selects PR_EVENT_PIXEL_PDS_DATA and raw PBE in PR; PROCESS_EMPTY_TILES logic; same
value as mainline ROGUE_FWIF_RENDERFLAGS_SPMSCRATCHBUFFER). All match mainline bit values.

## 4. HWRT / RENDER_TARGET / RTA_CTL / FREELIST consumption

### RGXFWIF_HWRTDATA

| off | field | use | evidence |
|---|---|---|---|
| 0 | eState | TA: first kick if NONE(0) or 3DFINISHED(5); passes KICKTAFIRST(2) / KICKTA(1) to pm_setup_context. FW writes the new state. | c00042d2..c00042ee, c000e570 |
| 4 | ui32NumPartialRenders | TA first kick: = 0. 3D PR: ++. 3D: priorPR fallback. | c000450a, c00028b2 |
| 8 | bLastWasPartial | TA first: = 0. 3D: = 1 on PR, 0 otherwise. | c000450c, c0001d74, c00028ac |
| 16 | psPMMListDevVAddr | PM ctx0 -> 0x2D8, ctx1 -> 0x2E0 [?, PM_MLIST0/1_BASE likely], 64, raw | c000e6d0, c000e580 |
| 24 | ui64VCECatBase[0] | BIF_PM_CAT_BASE_VCE0 (0x1248) / VCE1 (0x1268) [H] | c000e6e0, c000e590 |
| 88 | ui64TECatBase[0] | BIF_PM_CAT_BASE_TE0 (0x1250) / TE1 (0x1270) [H] | c000e6ec, c000e59c |
| 152 | ui64AlistCatBase | BIF_PM_CAT_BASE_ALIST0 (0x1260) / ALIST1 (0x1280) [H] | c000e6f8, c000e5a8 |
| 56, 120, 160 | *LastCatBase | not read in kick path | |
| 168 | ui64PMAListStackPointer | ctx0 -> 0x268, ctx1 -> 0x278 [?], 64; only when state != KICKTAFIRST (context reload) | c000ea00..ea10, c000ea5c |
| 176 | ui32PMMListStackPointer | ctx0 -> 0x2B8, ctx1 -> 0x2C8 [?]; same condition | c000e9ec, c000ea08, c000ea58 |
| 184 | apsFreeLists[0..1] | local / global freelist, loaded with rgxfw_pm_load_freelist when PM's loaded FL differs | c000e338.., c000e8ca, c000e926 |
| 192 | aui32FreeListHWRSnapshot[0..1] | compared with FL.ui32HWRCounter; on KICKTAFIRST overwritten with it | c000e344.., c000e74c..e776 |
| 200 | psParentRenderTarget | TA: VHeap + bTACachesNeedZeroing | c000435c |
| 228 | psRTACtl | TA RTC / 3D layer logic | c00043d6, c000180a |
| 232 | bHasLastTA | TA: written = LASTKICK. 3D: only logging if 0. | c00045e8, c00021ea |
| 240 | ui32PPPScreen | PPP_SCREEN 0xC98 [H], raw | c00043ec -> sw 3224 |
| 244 | ui32PPPGridOffset | 0xC78 [?], raw | c00043f4 -> sw 3192 |
| 248 | ui64PPPMultiSampleCtl | 0xC80 [D: PPP_MULTISAMPLECTL], 64 raw | c00043fc -> sw 3200/3204 |
| 256 | ui32TPCStride | 0xC48 [D: TE TPC stride], raw | c0004414 -> sw 3144 |
| 264 | sTailPtrsDevVAddr | 0xC40 [D: TE_TPC_ADDR], 64 raw; also zeroed (TPCSize bytes) when RT.bTACachesNeedZeroing | c0004428/2c -> sw 3136/3140; c0004b2c |
| 272 | ui32TPCSize | no register; size for rgxfw_zero_gpumem | c0004b22 |
| 276 | ui32TEScreen | TE_SCREEN 0xC18 [H], raw | c0004450 -> sw 3096 |
| 280 | ui32MTileStride | 0xC20 [D: TE_MTILE_STRIDE], raw | c0004458 -> sw 3104 |
| 284 | ui32TEAA | TE_AA 0xC00 [H], raw | c0004424 -> sw 3072 |
| 288 | ui32TEMTILE1 | TE_MTILE1 0xC08 [H], raw | c0004440 -> sw 3080 |
| 292 | ui32TEMTILE2 | TE_MTILE2 0xC10 [H], raw | c0004448 -> sw 3088 |
| 296..316 | ui32ISPMerge LowerX, LowerY, UpperX, UpperY, ScaleX, ScaleY | 0x11D0, 0x11D8, 0x11E0, 0x11E8, 0x11F0, 0x11F8 [D], raw, 3D kick | c00019e8..c0001a14 |
| 320 | bDisableTileReordering | not read in kicks | |

All TE/PPP/ISP-merge HWRT words are written to the registers with no shift, mask or OR.
They must be register-formatted.

### RGXFWIF_RENDER_TARGET

* `psVHeapTableDevVAddr` -> 0x248/0x24C [D: PM_VHEAP_TABLE], raw, every TA kick (c000436a..c0004380).
* `bTACachesNeedZeroing`: on a first TA kick, if set, FW zeroes the TPC (`HWRT.sTailPtrsDevVAddr`,
  `HWRT.ui32TPCSize` bytes) and, if `cmd.TA_RTC_ADDR_BASE != 0`, 768 bytes at the RTC
  (c0004b12..c0004b66). Then it writes 1 to this flag on every first kick (c000488e). The vendor KM
  initialises it to FALSE (rgxta3d.c:1907). Result: the first use of an RT relies on the host
  having zero-allocated TPC and RTC. Later first kicks zero them.

### RGXFWIF_RTA_CTL

* TA first kick: `ui32ActiveRenderTargets = 0`. If RTC == 0: also `ui32RenderTargetIndex = 0`,
  `ui32CurrentRenderTarget = 0` (c00044e6/e8). `sNumRenders` (if set): memset `ui16MaxRTs * 4`
  bytes (c0004af2).
* 0xD20 [?] = ActiveRenderTargets (or 0 on first kick) before and after the RTC init (c00044b2,
  c00044de, c0004926, c0004950).
* LASTKICK with RTC: rgxfw_ta_rtcache_parse maps `TA_RTC_ADDR + 512`, reads a 2048-bit bitmap,
  writes set-bit indices into `sValidRenderTargets[]`, then `ui32RenderTargetIndex = 0`,
  `ui32CurrentRenderTarget = sValidRenderTargets[0]` (c00041dc..c0004270). So the RTC buffer must
  be >= 768 bytes with the layer bitmap at +512.
* 3D: `CurRT` drives ISP_MTILE_BASE, ZLS and PBE slice offsets; 0x3D8 [?] = CurRT (c0001d84);
  0x3E0 [?] = 1 if `ActiveRTs == 0 || CurRT == ActiveRTs - 1`, else 0 (c0001d88..c0001da8,
  c00028ce). `sNumRenders[CurRT]++` on PR (c00028bc..c00028cc).

### RGXFWIF_FREELIST (rgxfw_pm_load_freelist, c000d2e4)

Args: `a1` = freelist index (0 = apsFreeLists[0] local, 1 = [1] global),
`a2` = (state == KICK3D) (0 = TA, 1 = 3D). Read fields: `ui64CurrentDevVAddr` (8),
`ui32CurrentStackTop` (16), `ui32AllocatedPageCount` (32), `ui32AllocatedMMUPageCount` (36).
`psFreeListDevVAddr`, MaxPages, GrowPages, CurrentPages are not used here.

| FL / DM | base (64) | stack top | page counts (lo, hi) | trigger (write 1, poll) |
|---|---|---|---|---|
| local, TA | 0x210 | 0x220 hi word = StackTop (lo = 0) | 0x390 | 0x208 |
| local, 3D | 0x218 | 0x230 = StackTop << 22 (64-bit) | 0x398 | 0x200 |
| global, TA | 0x20B8 | 0x20C0 = StackTop | 0x20D0 | 0x20B0 |
| global, 3D | 0x2078 | 0x2080 = StackTop | 0x2090 | 0x20A8 |

Evidence: c000d312..c000d34c, c000d35c..c000d396, c000d446..c000d470, c000d3c2..c000d3ee.
All names [?]. Host init (vendor rgxta3d.c:1600): `CurrentDevVAddr = base + (MaxPages -
CurrentPages) * 4`, `CurrentStackTop = CurrentPages - 1`, page counts 0.

## 5. rgxfw_pm_setup_context (c000e310) register writes

Selects PM context `s8` (0 or 1) using gs[1472]/gs[1480] (HWRT currently bound to each ctx).

* 0x3D0 [?] (PM context-ID select, 64-bit read-modify-write):
  TA (state 1/2): bits 0 and 24 = ctx (c000e708..e722). 3D (state 4): bit 8 and bit 32 = ctx
  (c000e3e6..e402). Always: bit 16 = ctx (c000e4c6..e4f0). Context reload: bit 16 and bit 40 =
  ctx (c000e9ca..e9fc). rgxfw_pm_alist_mlist_clear: bit 16 and bit 40 (c000d708..d742).
* 0x2A0 bit0 [?] set, poll 0x2A8 bit0 = 1 (PM pause) when gs[668] and a FL must be stored;
  later cleared, poll 0x2A8 = 0 (c000e7fe..e83e, c000e63e..e65c).
* rgxfw_pm_store_freelist / rgxfw_pm_load_freelist as needed (section 4).
* 0x2B0 [?] = `(otherDM_FL0 != this_FL0) << 1 | (otherDM_FL1 != this_FL1)` (c000e532..e53e).
  Ambiguous meaning ("FL shared between TA and 3D" flags).
* rgxfw_caches_pmmmu_flushinval(1 << ctx).
* PM_MLIST base and BIF_PM_CAT_BASE_* (section 4), unconditional.
* If state != KICKTAFIRST: MList SP, AList SP (section 4), then 0x190 = 1 poll, 0x288 = 1 poll
  [?, PM context load triggers] (c000ea18..ea3e).
* If state == KICKTAFIRST: HWRT FL HWR snapshots updated, no PM state load.
* rgxfw_pm_store_context (reverse): reads 0x2C0/0x270 (ctx0) or 0x2D0/0x280 (ctx1) plus the
  BIF_PM_CAT_BASE regs back into HWRT (c000e14c..e1e4).

## 6. Registers not sourced from command or HWRT

### TA kick (rgxfw_ta_kickta)

| reg | value | cond | evidence |
|---|---|---|---|
| 0x250 [?] | 1, poll bit0 = 0 (PM state load, continuing TA) | not first | c00043a6..c00043c6 |
| 0x258 [?] | 1, poll (PM init, first TA) | first | c0004892..c00048a0 |
| 0x290, 0x198 [?] | 1 each, poll (rgxfw_pm_alist_mlist_clear) | first | c000d74a, c000d758 |
| 0x418/0x41C [D: VDM_CALL_STACK_POINTER] | TACTX.VDM_CALL_STACK_POINTER_Init (first) or TACTX.VDM_CALL_STACK_POINTER | u | c00048c2..ca, c000439c..c00043bc |
| 0x420 [D: VDM_BATCH] | 0 (first) or TACTX.VDM_BATCH lo | u | c00048be, c00043b4 |
| 0xCB0 [D: TA_RTC_ADDR_BASE] | 0 when cmd RTC == 0 | | c00044c0 |
| 0xCE0 [?] | RTC == 0: 1 (first) / 3 (not first). RTC != 0: 0 | u | c00044ae, c000494a, c0004900, c0004b08 |
| 0xD20 [?] | 0 or RTACtl.ActiveRenderTargets (before and after init) | u | see 4 |
| 0xCB8 [?] | 1, poll bit0 = 0 (RTC init). rgxfw_ta_rtcache_store writes 2, polls bit1 | u | c00044ce, c0004910, c0004164 |
| 0xC68 [?, likely TE region-header init] | 1, poll bit0 = 0 | first | c0004502..c0004524 |
| CLK_CTRL (0x0) [H] | bits[9:8] (PM) = 01 (ON) | if !(gs[0] & 0x10) | c00046b4..c00046d2 |
| 0x400 [?, VDM/TA kick] | 1 | u, last write | c0004624 |
| BRN dummy region header init (once): TE_AA = 0, TE_MTILE1 = TE_MTILE2 = 0x00202018, TE_SCREEN = 0x1F01F, 0xC20 = 64, 0xCE0 = 1, 0xCB0 = 0, 0xCB8 = 1 poll, 0xD20 = 0, 0xCE0 = 0, 0xC68 = 1 poll | | gs[1784]+64 | c000479e..c0004828 |

Plus `rgxfw_bif_activate_memctx` (BIF catbase setup for the memctx) and section 5.

### 3D kick (rgxfw_3d_kick3d)

| reg | value | cond | evidence |
|---|---|---|---|
| 0x1318 hi [?] | `(hi & 0xF8F8F8F8) \| id \| id<<8 \| id<<16 \| id<<24`, id = memctx.uiPageCatBaseRegID | if hi[2:0] != id | c000186e..c00021b4 |
| RASTERISATION_INDIRECT 0x8238 [H] | 0 | u (twice) | c00019b0, c000174c |
| 0xFD8 [?] | min(trailing ones of PIXEL_OUTPUT_CTRL[14:3], 2) | u | c0001754 |
| ISP_RENDER 0xF08 [H] | 0 (`\|= 0x10` RESUME if ctx NEED_RESUME) | u | c00019e4, c00026f2..fa |
| 0xF10 [?, ISP_RENDER_ORIGIN by position] | 0 | u | c00019e0 |
| 0x700 [?] | 1 | u | c0001a1a |
| 0xFC8 [?] | 0 (64) | u | c0001a40 |
| 0x6D0 [?] | `(old & 0x1F) \| {0,32,96}` | u | c0001d42 |
| PM_PARTIAL_RENDER_ENABLE 0x338 [H] | 1 | PR only (not cleared here) | c00028a8 |
| 0x388 [?] | 1 | u | c0001d7c |
| 0x3D8 [?] | RTACtl.CurrentRenderTarget | u | c0001d84 |
| 0x3E0 [?] | last-layer flag (section 4) | u | c0001da8 |
| 0x1C0 [?] | 0 if `a2 & 1`, else 1 (`a2` = caller's s3[20] for normal kicks; meaning unknown) | u | c0001db8, c0002690 |
| 0x1020, 0x1028 [?] | 3DCTX_STATE.au3DReg_ISP_STORE[0], [1] | ctx NEED_RESUME (ctx->ui32Flags & 2) | c00026ae..c00026cc |
| 0x298 [?] | 3DCTX_STATE.PM_DEALLOCATED_MASK_STATUS (lhu, 16 bits), else 0 | u | c00026e2, c0001ddc |
| 0x1C8 [?] | 3DCTX_STATE.PM_PDS_MTILEFREE_STATUS & 0x1FFFF, else 0 | u | c00026ee, c0001de0 |
| CLK_CTRL (0x0) [H] | PM field = ON | if !(gs[0] & 0x10) | c0001fbe..c0001fd8 |
| 0xF00 [?, 3D/ISP kick] | 1 | u, last write | c0001e44 |

Plus pm_setup_context(state 4): PM ctx select, 3D freelist load, MList/AList SP load, 0x190/0x288 triggers.

## 7. Encoding the host must pre-compute

* **Address registers are raw byte addresses.** In cr.xml every relevant address field has
  `start == shift` (PM_MTILE_ARRAY 4/4, PM_VHEAP_TABLE 4/4, PM_MLIST0_BASE 4/4,
  TE_PSGREGION_ADDR 6/6, TE_TPC_ADDR 6/6, TA_RTC_ADDR 6/6, ISP_MTILE_BASE 2/2, ZLS bases), so the
  register value equals the aligned device virtual address. The FW confirms it: it adds byte
  offsets to ISP_MTILE_BASE and the ZLS bases (c0001956, c0001c3a..c0001c58), uses
  `TA_RTC_ADDR_BASE` and `sTailPtrsDevVAddr` as plain addresses for zeroing (c0004b30), and
  maps `RTC + 512` (c0004204). Alignment: MTILE_ARRAY/VHEAP/MLIST 16 B, PSGREGION/TPC/RTC 64 B.
* **TE_PSG** (cmd 24) is written as is. The host must encode REGION_STRIDE and all flags.
* **TE_PSGREGION_ADDR** (cmd 32): raw address of layer 0 region headers. The FW does not add a
  per-layer offset on the TA side (the TE uses TE_PSG.region_stride).
* **ISP_RGN** (cmd 216) is register-formatted (written to 0xF28), but the FW also uses its low
  word as a count: per-layer region-header stride = `ALIGN(ISP_RGN * 5, 4096)` bytes
  (c00018f8..c0001926). This only fits if ISP_RGN = number of region headers (the SIPF form,
  5 bytes each, which matches Mesa `pvr_rt_get_isp_region_size`). The host must lay out the
  layer region headers at that exact stride, and TE_PSG.region_stride must match.
* **ISP_MTILE_BASE** (cmd 72): raw address of layer 0 region headers (the same buffer as
  TE_PSGREGION_ADDR). The FW adds the layer offset.
* **ZLS bases**: give layer 0 addresses plus `ui32ZLSStride` / `ui32SLSStride` (bytes per layer).
  The FW adds `CurRT * stride`.
* **PBE_WORD1_MRTi**: for memlayout 2 (3D twiddled), the host must leave bits [?:14] free for
  the slice index. The FW adds `(aui16ZSliceOffsets[i] + CurRT) << 14`.
* **PM_MTILE_ARRAY** (cmd 0): raw 64-bit address, written each TA kick to 0x240.
* **HWRT TE/PPP/merge words**: register-formatted, no transformation. Examples:
  TE_MTILE1 = `x1<<18 | x2<<9 | x3`, TE_SCREEN = `ymax<<12 | xmax` (in tiles),
  PPP_SCREEN = `pixymax<<16 | pixxmax`. Cross-check: the FW's built-in dummy setup uses
  TE_MTILE = 0x00202018 (8/16/24 tiles), TE_SCREEN = 0x1F01F, MTileStride = 64.
* **HWRT PM state**: eState = 0, catbases = 0, stack pointers = 0 (vendor KM zero-allocates).
  First-kick detection uses eState, not TAFLAGS_FIRSTKICK.
* **Freelists**: `ui64CurrentDevVAddr = base + (MaxPages - CurrentPages) * 4`,
  `ui32CurrentStackTop = CurrentPages - 1`. `psFreeListDevVAddr` is not loaded into the PM by
  the FW. The CurrentDevVAddr is.
* **TPC and RTC buffers must be zero on first use** (FW zeroes only on later first kicks).
  RTC must be >= 768 bytes when used. TA_RTC_ADDR_BASE = 0 disables the RTC path.
* **VDM context resume**: when the TA context has NEED_RESUME (ctx->ui32Flags bit 0), the FW
  writes VDM_CONTEXT_STATE_BASE_ADDR to 0x408 (VDM_CTRL_STREAM_BASE). It also toggles
  TACTX.ui16TACurrentIdx and uses asTAState[idx] for both store and resume task words, so both
  array entries must be valid.

## 8. Command fields the FW ignores in the kick path

TA: VDM_CTRL_STREAM_TERMINATE, TPU_ARRAYS_VDM, sZBuffer, sSBuffer, sMSAAScratchBuffer,
PR fences, BRN61484LiveRT (they may be used in OOM/PR or BRN paths, not checked).
3D: USC_CLEAR_REGISTER4..7, ISP_ZLS_PIXELS, FB_CDC_ZLS, PM_3D_FSTACK_BASE, PM_3D_FSTACK,
TPU_ARRAYS, RGX_CR_BLACKPEARL_FIX, sZBuffer, sSBuffer, sMSAAScratchBuffer.
Also: 64-bit fields written low-word only (hi ignored): PPP_CTRL, TE_PSG, VDM_SYNC_PDS_DATA_BASE,
TPU, STORE_TASK1, RESUME_TASK1, ISP_MTILE_SIZE, ISP_GRIDOFFSET, ISP_BGOBJDEPTH, ISP_BGOBJVALS,
ISP_AA, ISP_RGN, ISP_CTL, EVENT_PIXEL_PDS_*, PBE, TPUMADDCtrl, PDS_BGND2_BASE (hi dropped).
