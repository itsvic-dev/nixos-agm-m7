# CCCI file service (`ccci_fsd`) protocol for MD1 NVRAM

Goal: a Rust replacement for the stock `ccci_fsd`. The daemon serves the modem file system requests on CCCI channel 14/15 from the NVRAM folders on the AP.

Sources:
- **[D]** Disassembly and Ghidra decompile of `re/ccci/ccci_fsd` ("md_fsd Ver:v2.2"). Symbols are present (`FS_CCCI_Open`, `FS_CCCI_Read`, ...). Most ops are inline in `main` (0x13e6c, 66 KB).
- **[S]** Vendor kernel `ref/lumi-mt6739/drivers/misc/mediatek/eccci` (`port/port_proxy.c`, `port/port_char.c`, `port/port_cfg.c`, `hif/ccci_hif_internal.h`, `ccci_util/ccci_util_lib_sys.c`).
- **[P]** Phone observation (first request, below).
- **[I]** `~/agm-m7-backup/nvdata.img`, `protect1.img`, `protect2.img` (ext4, read with `debugfs`).
- **[A]** Assumption. Names and struct layouts from the MediaTek FS API (`FS_DiskInfo`, `FS_DOSDirEntry`, error names). The binary does not contain these names. The offsets that the daemon writes agree with them.

Scratch work (decompile, patched binary, `asm0.txt`): `/tmp/agm-fsd-re/`.

## 1. Kernel interface

### 1.1 Device node and mode
| item | value | source |
|---|---|---|
| Service | `ccci_fsd 0` (MD1, user radio, group radio system, oneshot). `ccci3_fsd 2` is for MD3 (C2K), not needed. | `init.cccifsd.rc` |
| CCCI version | First char of `/sys/kernel/ccci/version`. Vendor kernel prints `6` (`ccci_port_ver = 6 /* ECCCI_FSM */`). | [S] [D] |
| Node | `/dev/ccci_fs` (version table index `ver-1`; versions 3, 5 and 6 all give `/dev/ccci_fs`), `open(O_RDWR)`, retry 600 x 10 ms. | [D] |
| Data mode | Version 3, 5 or 6: **inline mode**. Data travels in the CCCI packets. The daemon uses `malloc`, not `mmap`. | [D] |
| Old mode | Other versions: `mmap(fd, 0x14014)` shared buffer (5 slots of 0x4004), `ioctl(0x4b01)` gets the slot index, `ioctl(0x80044b02, {len, slot})` sends the reply. **Not used on this phone.** | [D] |
| Header | Port flags `PORT_F_USER_HEADER \| PORT_F_WITH_CHAR_NODE`. `read()` returns the full 16-byte `ccci_header` plus payload. `write()` takes header plus payload. | [S] `port_cfg.c:234`, `port_adjust_skb` |
| Kernel on write | Sets `data[1] = count` (whole write, header included) and `channel = 15`. Keeps `data[0]` and `reserved`. The HIF sets `seq_num` and `assert_bit` (assert_bit = 1 only while MD state is `BOOT_WAITING_FOR_HS2`, else 0). Max write = `CCCI_MTU + 16 + 4`. FS and RPC writes are allowed before HS2. | [S] `port_dev_write`, `ccci_md_inc_tx_seq_num` |
| Kernel on read | Packet queued even if no reader has the port open (`CCCI_FS_RX` exception). | [S] `port_char_recv_skb` |

ioctls that the daemon uses:
| ioctl | name | why |
|---|---|---|
| 0x00004304 | `CCCI_IOC_FORCE_MD_ASSERT` | Request has a bad slot index or no packet. The daemon asserts the MD. |
| 0x80044322 | `CCCI_IOC_GET_EXT_MD_POST_FIX` | Only for drive `W:` (DSP image name). |
| 0x00004b01, 0x80044b02 | old shared-buffer mode | Not used with version 6. |
| 0x40046b01..03 | `/dev/otp` ioctls | OTP ops, only if `libstorage_otp` is old. |

The daemon does not use `CCCI_IOC_SET_HEADER`. The port has the user-header flag from the kernel table.

### 1.2 Startup sequence [D]
1. Read the CCCI version. Select device and drive folders from `argv[1]` (`0` = MD1).
2. Read `persist.md.fsd.debug` (log level only).
3. Open `/dev/ccci_fs`. Allocate 5 slot buffers of 0x40dc bytes, a param table, and a 0x1000-byte RX buffer.
4. Poll `mtk.md1.status` every 100 ms until it is set and not `init` (set by `ccci_mdinit`). Our daemon does not need this wait.
5. `FS_Init`: `umask(007)`. `mkdir 0770` if missing: `/vendor/nvdata`, `/vendor/nvdata/md_cmn`, `/vendor/nvdata/md`, `/vendor/protect_f/md`, `/vendor/protect_s/md`. Clear the handle table.
6. `FS_OTP_init`: probe `/dev/otp`. Errors are only logged.
7. Install signal handlers (they set an exit flag).
8. Loop: blocking `read(fd, buf, 0xd94)`, then reassemble, run the op, reply. A wake lock is held per request.

### 1.3 Packet framing [D] [P]
Request (MD to AP, channel 14) and reply (AP to MD, channel 15) use the same layout:

| off | size | field |
|---|---|---|
| 0x00 | 4 | `data[0]`: bit 31 = more fragments follow. Other bits: copied back. MD sends 0. |
| 0x04 | 4 | `data[1]`: bytes in this packet, header included |
| 0x08 | 2 | `channel` (14 request, 15 reply) |
| 0x0a | 2 | `seq_num:15, assert_bit:1` (kernel owned) |
| 0x0c | 4 | `reserved` = **slot index 0..4**. It is NOT a shared buffer offset in inline mode. |
| 0x10 | 4 | `op_id` |
| 0x14 | 4 | `para_num` |
| 0x18 | ... | `para_num` x {`u32 len`; `len` bytes data; pad to 4 bytes} |

The observed first request (`data[1]` 0x2c = 44 = 16 + 28, slot 0, op 0x100e, 2 args) matches this [P].

Receive checks [D]: `reserved <= 4`, `channel & 0xffff == 14`, `data[1] <= 0x40dc`. Failure logs "packet data check fail" and asserts the MD. Param parse (`GetPackInfo`): `para_num <= 6`, no param with `len == 0`, total size `<= 0x40c8`. Failure gives a reply with result -2.

Fragments [D]: The MD splits a request larger than 0xd94 bytes. Each fragment has the 16-byte header and the 4-byte `op_id`, then up to 0xd80 bytes of the stream. Non-last fragments have `data[0]` bit 31 set. The daemon keeps one reassembly buffer per slot. It copies the first fragment whole. For each next fragment it appends the bytes after offset 0x14. On the last fragment (bit 31 clear) it copies the last header to the slot start and runs the op.

Reply build [D] (`0x22164`):
- Start from the stored request header of the same slot. `op_id |= 0xFFFF0000` (0x100e becomes 0xFFFF100E). `para_num` = reply param count. Params as in the request.
- Header word 2 is incremented by 1, so channel 14 becomes 15. Seq bits stay, the kernel rewrites them. `reserved` keeps the slot index. `data[0] &= 0x7fffffff`.
- If total `<= 0xd94`: one `write()` of the whole packet, `data[1]` = total.
- Else: fragments. Each one = header + `op_id` (with 0xFFFF0000) + up to 0xd80 bytes. `data[1]` = fragment size, bit 31 of `data[0]` set on all but the last.
- Reply size limit: `0x18 + sum(4 + align4(len)) <= 0x40c8`, else "Stream buffer full" and no reply.
- Shutdown (0x1017) sends a reply with `para_num = 0`.

## 2. Op table [D]
Args are numbered in request order. "R" = reply params. `res` = `s32` result (0 or positive = OK, negative = FS error). Names come from the log tags and the MediaTek `FS_*` API [A].

| op | name (log tag) | args | reply |
|---|---|---|---|
| 0x1001 | Open (`O`) | path(wstr), flags u32 | R1 res = handle 1..128 |
| 0x1002 | Seek (`S`) | handle, offset s32, whence u32 (0 set, 1 cur, 2 end; else -2) | R1 res = new position |
| 0x1003 | Read (`R`) | handle, len u32 | R3 res, u32 nread, data[nread] |
| 0x1004 | Write (`W`) | handle, data[n], len u32 | R2 res, u32 nwritten |
| 0x1005 | Close (`C`) | handle | R1 res. `fsync` first if flags had 0x01000000, 0x10000 or 0x20000. Also closes find handles. |
| 0x1006 | CloseAll (`CA`) | none | R1 res |
| 0x1007 | CreateDir (`CD`) | path | R1 res (`mkdir 0770`) |
| 0x1008 | RemoveDir (`MD`) | path | R1 res (`rmdir`) |
| 0x1009 | GetFileSize (`F`) | handle | R2 res, u32 size (`fstat` st_size) |
| 0x100a | GetFolderSize (`FS`) | path, flag u32 | R1 res = cluster count x cluster size. Recursive. If flag == 2, the cluster count only. -2 if not a dir. |
| 0x100b | Rename (`R`) | old path, new path | R1 res (`rename`) |
| 0x100c | Move (`M`) | src, dst, flag u32 | R1 res. flag bits: 1 copy, 2 kill (delete src), 0x10000 overwrite. Same path or bad flag: -2/-45. Copy = read all, `creat(0777)`, write. |
| 0x100d | Count (`CN`) | path, flag u32 | R1 res = count from recursive search (flag 0x4 files, 0x10 recursive) |
| 0x100e | GetDiskInfo (`I`) | drive wstr ("Z:\\"), flags u32 | R2 res, 0x54-byte `FS_DiskInfo`. Args are ignored. Always `statfs(Z: folder)`. |
| 0x100f | Delete (`D`) | path | R1 res (`unlink`) |
| 0x1010 | GetAttributes (`A`) | path | R1 res = attr bits (0x01 read-only, 0x10 dir) or error (-9 if missing) |
| 0x1011 | OpenHint (`O`) | path, flags, hint (8 bytes) | R2 res = handle, hint echoed (8 bytes) |
| 0x1012 | FindFirst (`FF`) | pattern(wstr), attr u8, attr_mask u8, max_len u32 (wchars) | R3 res = find handle, `FS_DOSDirEntry` 0x34, name (wstr, see 4.4) |
| 0x1013 | FindNext (`FN`) | handle, max_len | R3 res (0 or -6), `FS_DOSDirEntry`, name |
| 0x1014 | FindClose (`FC`) | handle | R1 res (`closedir`) |
| 0x1015 | LockFAT (`LF`) | any | R1 0 |
| 0x1016 | UnlockAll (`UK`) | any | R1 **1** |
| 0x1017 | Shutdown | any | CloseAll, R0 (no params) |
| 0x1018 | XDelete (`X`) | path, flag u32 | R1 res = number of deleted entries. flag must be inside 0x1c and not 0. 0x04 files, 0x08 dirs, 0x10 recursive; 0x18 also removes the top dir. |
| 0x1019 | ClearDiskFlag (`CDF`) | any | R1 0 |
| 0x101a | GetDrive (`GD`) | type u32, serial u32, alt_mask u32 | R1 0x5a ('Z') if type == 4 (`FS_DRIVE_I_SYSTEM`), else -2 |
| 0x101b | GetClusterSize (`CS`) | drive letter u32 ('V'..'Z') | R1 `statfs.f_bsize` of that drive, or -4 |
| 0x101c | SetDiskFlag (`SDF`) | any | R1 0 |
| 0x101d | OTPWrite (`OW`) | dev u32, offset u32, data[], len u32 | R1 res |
| 0x101e | OTPRead (`OR`) | dev, offset, len | R2 res, data[len] |
| 0x101f | OTPQueryLength (`OQL`) | dev | R2 res, u32 length |
| 0x1020 | OTPLock (`OL`) | dev | R1 res |
| 0x1021 | Restore (`RS`) | path | R1 res. `NVM_RestoreFromBinRegion_OneFile(-1, linux_path)` from libnvram (reads `/dev/nvram`). 0 or -18. |
| 0x1022 | CMPT read (compact read) | path, 40-byte struct | R4 (see 4.6) |
| 0x1023 | BinRegion access | type u32 (1 backup, 2 restore, 3 erase) | R1 -2 always ("bin_region unsupport") |
| other | | | R1 -2 ("Unknow File Op ID") |

Handle errors: handle > 128 or not open gives -10.

### 2.1 Error codes seen in the binary [D] (names [A])
| code | name | when |
|---|---|---|
| -2 | `FS_PARAM_ERROR` | bad flag, bad whence, not a dir, unknown op, bad packet |
| -4 | `FS_DRIVE_NOT_FOUND` | GetClusterSize with bad letter |
| -5 | `FS_TOO_MANY_FILES` | no free handle; errno `EMFILE` |
| -6 | `FS_NO_MORE_FILES` | FindFirst/FindNext end |
| -9 | `FS_FILE_NOT_FOUND` | errno `ENOENT` |
| -10 | `FS_INVALID_FILE_HANDLE` | bad handle |
| -12 | `FS_UNSUPPORTED_DRIVER_FUNCTION` | OTP open fail |
| -16 | `FS_ACCESS_DENIED` | errno `EACCES`; `..` in path (Open) |
| -18 | `FS_GENERAL_FAILURE` | any other errno; restore fail |
| -19 | `FS_PATH_NOT_FOUND` | unknown drive letter |
| -45 | `FS_READ_ONLY_ERROR` | Move onto read-only file |
| -49 | `FS_PATH_OVER_LEN_ERROR` | Linux path >= 4096 |

`FS_ErrorConv(errno)`: `ENOENT` -9, `EACCES` -16, `EMFILE` -5, else -18. `-1` is returned for some malloc and Move failures.

## 3. Path mapping [D]
Conversion `FS_ConvWcsToCs`: take the **low byte** of each UTF-16LE unit (high byte dropped), stop at a 0 unit or 4095 chars, change `\` to `/`. If the text has `..`, it logs "can't use relative path for security" and returns -1. Only Open checks the return value (gives -16); other ops go on with the string cut after `..`. Our daemon must reject `..` in all ops.

Drive prefix compare is case sensitive (`strncmp` of 2 chars). No case folding of the rest. Linux path = `folder + rest` (`"%s%s"`, the rest keeps its leading `/`).

| drive | MD1 folder | use |
|---|---|---|
| `Z:` | `/vendor/nvdata/md` | main NVRAM (`Z:\NVRAM\NVD_DATA\LD0B_001`) |
| `X:` | `/vendor/protect_f/md` | protect F (calibration-critical, has a mirror) |
| `Y:` | `/vendor/protect_s/md` | protect S (mirror of X) |
| `W:` | `/vendor/firmware` | DSP image. The rest of the path is ignored. File = `<folder>/dsp_<postfix>.bin` (postfix from `CCCI_IOC_GET_EXT_MD_POST_FIX`). If missing: "try CIP path" `/custom/etc/firmware/dsp_<postfix>.bin`. Not needed here: LK loads `md1dsp`, `/vendor/firmware` has no `dsp_*`. |
| `V:` | `/vendor/nvdata/md_cmn` | common folder (empty on this phone [I]) |
| other | | -19 |

Logs show a raw dump for `X:` and `Y:` paths only. The daemon does not sync `X:` and `Y:`. The modem itself writes both copies.

Op 0x100e args: arg0 = drive name `"Z:\"`, arg1 = `3` is the info mask [A] (`FS_DI_BASIC_INFO 1 | FS_DI_FREE_SPACE 2`). The stock daemon ignores both.

## 4. File semantics

### 4.1 Handles [D]
Table of 128 entries (handle 1..128, index 0 unused), entry = 0x1c bytes: `in_use`, `kind` (0 file, 1 find), `fd` or `DIR*`, `flags` (find: `attr`, `attr_mask` bytes), path copy, pattern copy. Lowest free handle first.

### 4.2 Open flags [D]
| MD flag | value | POSIX |
|---|---|---|
| default (read/write) | 0 | `O_RDWR` |
| `FS_READ_ONLY` | 0x100 | `O_RDONLY` (if no create flag) |
| `FS_CREATE` | 0x10000 | `O_RDWR \| O_CREAT` |
| `FS_CREATE_ALWAYS` | 0x20000 | `O_RDWR \| O_CREAT \| O_TRUNC` (wins) |
| `FS_NONBLOCK_MODE` [A] | 0x10000000 | `\| O_NONBLOCK` (`(flags >> 17) & 0x800`) |
| `FS_COMMITTED` [A] | 0x01000000 | no open effect; `fsync` on Close |
Mode 0660. Other bits (0x200 shared, 0x400, 0x800, cache bits) are ignored. If `open` fails, the daemon checks the parent folders. If `/vendor/nvdata/md` or `/vendor/nvdata` is missing, it writes to address 0 (crash, init does not restart a oneshot).

### 4.3 Attributes [D]
Read-only (0x01) = no write or exec bit in mode (`mode & 0333 == 0`) and some read bit. Dir = 0x10. Other DOS bits are never set.

### 4.4 FindFirst / FindNext [D]
- Pattern = last path component. Match with `?` (one char) and `*` (any run), case sensitive. The folder is `opendir`'d.
- FindFirst skips `.` and `..`. **FindNext does not** (a `*` pattern can return them; our daemon should skip them).
- Entry is taken if `(fattr & attr) == attr` and `(fattr & attr_mask) == 0`.
- Name param: UTF-16LE of the Linux name, `/` changed to `\`, at most `max_len` units, plus a 0 unit. Length = `n*2 + 2`. On error the name param has length 0.

`FS_DOSDirEntry` (0x34 bytes, packed, all zero first):
| off | size | field [A] | value [D] |
|---|---|---|---|
| 0x00 | 8+3 | `FileName`, `Extension` | 0 (long name is in the 3rd param) |
| 0x0b | 1 | `Attributes` | 0x01 / 0x10 |
| 0x0e | 4 | `CreateDateTime` | DOS date-time of **st_atime** |
| 0x12 | 4 | `LastAccessDate`, `FirstClusterHi` | 0 |
| 0x16 | 4 | `DateTime` | DOS date-time of st_mtime |
| 0x1a | 2 | `FirstCluster` | 0 |
| 0x1c | 4 | `FileSize` | st_size |
| 0x20 | 20 | `Cluster, Index, Stamp, Drive, SerialNumber` | 0 |
DOS date-time = `sec&31 | min<<5 | hour<<11 | mday<<16 | (mday&15)<<21 | (year-80)<<25`. The stock code puts `tm_mday` in the month field (bug) and does not halve the seconds.

### 4.5 GetDiskInfo result `FS_DiskInfo` (0x54 bytes) [D] (names [A])
Only these offsets are written. Send all other bytes as 0.
| off | field | value |
|---|---|---|
| 0x30 | `BytesPerSector` | 512 |
| 0x34 | `SectorsPerCluster` | `f_bsize / 512` (8 for 4 KiB) |
| 0x38 | `TotalClusters` | `f_blocks` (low 32 bits) |
| 0x40 | `FreeClusters` | **512 (constant)** |
Layout before 0x30 [A]: `Label[24]`, `DriveLetter`, `WriteProtect`, `Reserved[2]`, `SerialNumber`, `FirstPhysicalSector`, `FATType`, `FATCount`, `MaxDirEntries`. After 0x40: `Files`, `FileChains`, `FreeChains`.

### 4.6 CMPT read (0x1022) [D], medium confidence
Request arg1 (40 bytes, u32 each): `+0 opid_map` (bit0 open, bit1 get size, bit2 seek, bit3 read, bit4 close), `+4` done map (out), `+8` failing step error (out), `+0xc` open flags, `+0x10` file size (out), `+0x14` offset, `+0x18` whence, `+0x1c` MD buffer pointer (ignored), `+0x20` length, `+0x24` read count (out).
The daemon runs the steps in order and stops on the first failure (close the file if open). Bit 0 and bit 4 must be set (else error).
Reply, 4 params: `{u32 done_map, s32 step_error}` (8 bytes), `u32 file_size`, `u32 nread`, `data[nread]`. The overall result code is not sent.

### 4.7 Other behavior [D]
- Seek offset is 32-bit. Files are small, so this is OK.
- No backup or sync logic for `protect_f`/`protect_s` in the daemon. BinRegion (0x1023) is not supported. Restore (0x1021) calls libnvram, which reads the `nvram` partition (`/dev/nvram`, `nvram.img` 5.2 MB) [D libnvram strings]. That format is not reversed yet.
- Slow ops (> 100 ms) during boot are only counted for a log.

## 5. Files on this phone [I]
`/vendor/nvdata` (from `nvdata.img`). Folders mode 0770/0771, owner root:system or radio:radio; files 0660.
| path | files | bytes | note |
|---|---|---|---|
| `md/NVRAM/NVD_IMEI` | 4 | 660 | `FILELIST`, `LD0B_001`, `NV01_000`, `NV0S_000` |
| `md/NVRAM/CALIBRAT` | 144 | 432181 | RF calibration (`EL*`, `HL*`, `LA*`, `UL*`, `EA*`, `ML*`, `TD*`, `FILELIST`). Largest `EL92_001` 127048. |
| `md/NVRAM/NVD_CORE` | 2 | 464 | `MT00A000`, `MT00B000` |
| `md/NVRAM/NVD_DATA` | 157 | 261838 | settings (`UL*` 59, `VS*` 21, `LD*` 14, ...). Largest `LA9Y_000` 18152. |
| `md/NVRAM/BACKUP` | 0 | 0 | empty folder |
| `md/NVRAM/INFO_FILE` | 1 | 1564 | starts with `INFO` |
| `md_cmn`, `md3` | 0 | 0 | empty |
| `AllFile`, `AllMap`, `APCFG/`, `RestoreFlag`, `media/` | | | AP-side NVRAM; not seen by the modem |

`/vendor/protect_f/md` (`protect1.img`) and `/vendor/protect_s/md` (`protect2.img`) have the same 4 files: `LD40_001` 19264, `NA77_006` 9312, `ST33_004` 2784, `ST34_001` 832. `NA77_006` has a 10-Oct-2026 mtime, so the MD writes it at runtime. `nvcfg.img` has only `fg/` (fuel gauge) and is not for the modem.

File names are LID names: 4 chars + `_` + 3-digit version. The MD makes missing LIDs from its defaults if it can create files.
List all files: `, debugfs -R 'rdump / /tmp/x' ~/agm-m7-backup/nvdata.img` then `find /tmp/x/md -type f`.

## 6. Minimal implementation plan
1. Kernel: a char device for channel 14/15 with the user-header semantics of 1.1 (full header in `read()`; on `write()` set `data[1]`, channel 15, seq, assert_bit). Keep `reserved` as is.
2. Daemon core: read loop with a buffer >= 0xd94, 5 slot buffers, fragment reassembly, param parser, reply builder with fragmentation. Do not assert the MD on errors at first; log them.
3. Storage: copy `md/`, `protect_f/md`, `protect_s/md` from the images to a writable tree (for example `/var/lib/nvram/{nvdata,protect_f,protect_s}`). Map the drive letters to it. Reject `..`. Keep the stock `mkdir` list.
4. First-boot op set (implement fully): 0x100e GetDiskInfo, 0x1010 GetAttributes, 0x1001/0x1011 Open/OpenHint, 0x1002 Seek, 0x1003 Read, 0x1004 Write, 0x1005 Close, 0x1006 CloseAll, 0x1009 GetFileSize, 0x1012/0x1013/0x1014 Find*, 0x1007 CreateDir, 0x100f Delete, 0x100b Rename, 0x1018 XDelete, 0x1022 CMPT read, 0x1017 Shutdown. They are thin POSIX wrappers.
5. Simple stubs with the stock values: 0x1015 0, 0x1016 1, 0x1019 0, 0x101c 0, 0x101a 'Z', 0x101b `f_bsize`, 0x1023 -2.
6. Return errors: OTP 0x101d..0x1020 -12, Restore 0x1021 -18 (until the `nvram` partition format is known). 0x100a, 0x100c, 0x100d, 0x1008: implement later if the log shows them.
7. Test on the phone: log every op with path and result. Check the expected order: GetDiskInfo, then folder scans and LID reads.

Open questions:
- Exact meaning of the reply fields for CMPT read (4.6) is from the decompile only.
- What the MD does with `FreeClusters = 512`. Keep the stock value.
- `NVM_RestoreFromBinRegion_OneFile` format (`nvram` partition), needed only if the MD finds a broken LID.
