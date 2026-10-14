//! Modem NVRAM file service for the MT6739 CCCI `ccci_fs` port (see docs/CCCI-FS.md).

use std::collections::HashMap;
use std::ffi::CString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

const HDR: usize = 16;
const FRAG_MAX: usize = 0xd94;
const FRAG_DATA: usize = 0xd80;
const MORE: u32 = 1 << 31;
const RESP: u32 = 0xffff_0000;
const SLOTS: u32 = 5;
const HANDLES: usize = 128;
const MAX_PARAMS: usize = 6;
const STREAM_MAX: usize = 0x40c8;

const E_PARAM: i32 = -2;
const E_DRIVE: i32 = -4;
const E_TOO_MANY: i32 = -5;
const E_NO_MORE: i32 = -6;
const E_NOT_FOUND: i32 = -9;
const E_HANDLE: i32 = -10;
const E_UNSUPPORTED: i32 = -12;
const E_ACCESS: i32 = -16;
const E_GENERAL: i32 = -18;
const E_PATH: i32 = -19;

const O_READ_ONLY: u32 = 0x100;
const O_CREATE: u32 = 0x10000;
const O_CREATE_ALWAYS: u32 = 0x20000;
const O_COMMITTED: u32 = 0x0100_0000;

const ATTR_READ_ONLY: u8 = 0x01;
const ATTR_DIR: u8 = 0x10;

enum Handle {
    File {
        file: File,
        flags: u32,
    },
    Find {
        dir: PathBuf,
        names: Vec<String>,
        pos: usize,
        pattern: String,
        attr: u8,
        mask: u8,
    },
}

struct Fsd {
    root: PathBuf,
    handles: Vec<Option<Handle>>,
}

type Reply = Vec<Vec<u8>>;

fn le32(b: &[u8]) -> u32 {
    let mut v = [0u8; 4];
    let n = b.len().min(4);
    v[..n].copy_from_slice(&b[..n]);
    u32::from_le_bytes(v)
}

fn res(v: i32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn errno(e: &io::Error) -> i32 {
    match e.raw_os_error() {
        Some(libc::ENOENT) => E_NOT_FOUND,
        Some(libc::EACCES) => E_ACCESS,
        Some(libc::EMFILE) => E_TOO_MANY,
        _ => E_GENERAL,
    }
}

/// The stock daemon keeps the low byte of each UTF-16 unit.
fn wstr(b: &[u8]) -> String {
    b.chunks_exact(2)
        .map(|c| c[0])
        .take_while(|&c| c != 0)
        .map(|c| if c == b'\\' { '/' } else { c as char })
        .collect()
}

fn to_wstr(name: &str, max: usize) -> Vec<u8> {
    let mut out: Vec<u8> = name
        .encode_utf16()
        .take(max)
        .map(|u| if u == u16::from(b'/') { u16::from(b'\\') } else { u })
        .flat_map(|u| u.to_le_bytes())
        .collect();
    out.extend_from_slice(&[0, 0]);
    out
}

fn attributes(meta: &fs::Metadata) -> u8 {
    let mode = meta.mode();
    let mut a = 0;
    if meta.is_dir() {
        a |= ATTR_DIR;
    }
    if mode & 0o333 == 0 && mode & 0o444 != 0 {
        a |= ATTR_READ_ONLY;
    }
    a
}

fn wildcard(pat: &[u8], name: &[u8]) -> bool {
    match (pat.first(), name.first()) {
        (None, None) => true,
        (Some(b'*'), _) => wildcard(&pat[1..], name) || (!name.is_empty() && wildcard(pat, &name[1..])),
        (Some(b'?'), Some(_)) => wildcard(&pat[1..], &name[1..]),
        (Some(p), Some(n)) if p == n => wildcard(&pat[1..], &name[1..]),
        _ => false,
    }
}

fn dos_time(t: i64) -> u32 {
    let days = t.div_euclid(86400);
    let secs = t.rem_euclid(86400) as u32;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (yoe + era * 400 + i64::from(month <= 2)).clamp(1980, 2107) as u32;
    (secs % 60 / 2) | (secs / 60 % 60) << 5 | (secs / 3600) << 11 | day << 16 | month << 21 | (year - 1980) << 25
}

fn dir_entry(meta: Option<&fs::Metadata>) -> Vec<u8> {
    let mut e = vec![0u8; 0x34];
    if let Some(m) = meta {
        e[0x0b] = attributes(m);
        e[0x0e..0x12].copy_from_slice(&dos_time(m.atime()).to_le_bytes());
        e[0x16..0x1a].copy_from_slice(&dos_time(m.mtime()).to_le_bytes());
        e[0x1c..0x20].copy_from_slice(&(m.size() as u32).to_le_bytes());
    }
    e
}

fn statvfs(path: &Path) -> Option<libc::statvfs> {
    let c = CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    // SAFETY: statvfs is plain old data, so all zeros is a valid value.
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: c is a NUL-terminated path and st is a valid out pointer.
    (unsafe { libc::statvfs(c.as_ptr(), &mut st) } == 0).then_some(st)
}

impl Fsd {
    fn drive(&self, letter: u8) -> Option<PathBuf> {
        let sub = match letter {
            b'Z' => "nvdata/md",
            b'X' => "protect_f/md",
            b'Y' => "protect_s/md",
            b'V' => "nvdata/md_cmn",
            _ => return None,
        };
        Some(self.root.join(sub))
    }

    fn path(&self, arg: &[u8]) -> Result<PathBuf, i32> {
        let p = wstr(arg);
        if p.contains("..") {
            eprintln!("rejected relative path {p}");
            return Err(E_ACCESS);
        }
        let b = p.as_bytes();
        if b.len() < 2 || b[1] != b':' {
            return Err(E_PATH);
        }
        let base = self.drive(b[0]).ok_or(E_PATH)?;
        let rest = p[2..].trim_start_matches('/');
        Ok(if rest.is_empty() { base } else { base.join(rest) })
    }

    fn alloc(&mut self, h: Handle) -> i32 {
        match self.handles.iter().skip(1).position(Option::is_none) {
            Some(i) => {
                self.handles[i + 1] = Some(h);
                (i + 1) as i32
            }
            None => E_TOO_MANY,
        }
    }

    fn file(&mut self, h: &[u8]) -> Result<&mut File, i32> {
        match self.handles.get_mut(le32(h) as usize) {
            Some(Some(Handle::File { file, .. })) => Ok(file),
            _ => Err(E_HANDLE),
        }
    }

    fn open(&mut self, path: &[u8], flags: u32) -> i32 {
        let path = match self.path(path) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let mut o = OpenOptions::new();
        o.read(true).mode(0o660);
        if flags & (O_CREATE | O_CREATE_ALWAYS) != 0 {
            o.write(true).create(true).truncate(flags & O_CREATE_ALWAYS != 0);
        } else if flags & O_READ_ONLY == 0 {
            o.write(true);
        }
        let r = match o.open(&path) {
            Ok(file) => self.alloc(Handle::File { file, flags }),
            Err(e) => errno(&e),
        };
        eprintln!("open {} {flags:#x} -> {r}", path.display());
        r
    }

    fn close(&mut self, h: u32) -> i32 {
        match self.handles.get_mut(h as usize).and_then(Option::take) {
            Some(Handle::File { file, flags }) => {
                if flags & (O_COMMITTED | O_CREATE | O_CREATE_ALWAYS) != 0
                    && let Err(e) = file.sync_all()
                {
                    return errno(&e);
                }
                0
            }
            Some(Handle::Find { .. }) => 0,
            None => E_HANDLE,
        }
    }

    fn find_next(&mut self, h: usize, max: usize) -> Reply {
        let Some(Some(Handle::Find { dir, names, pos, pattern, attr, mask })) = self.handles.get_mut(h) else {
            return vec![res(E_HANDLE), dir_entry(None), Vec::new()];
        };
        while *pos < names.len() {
            let name = &names[*pos];
            *pos += 1;
            if !wildcard(pattern.as_bytes(), name.as_bytes()) {
                continue;
            }
            let Ok(meta) = fs::metadata(dir.join(name)) else { continue };
            let a = attributes(&meta);
            if a & *attr == *attr && a & *mask == 0 {
                return vec![res(h as i32), dir_entry(Some(&meta)), to_wstr(name, max)];
            }
        }
        vec![res(E_NO_MORE), dir_entry(None), Vec::new()]
    }

    fn find_first(&mut self, a: &[&[u8]]) -> Reply {
        let pat = match self.path(a[0]) {
            Ok(p) => p,
            Err(e) => return vec![res(e), dir_entry(None), Vec::new()],
        };
        let (dir, pattern) = match (pat.parent(), pat.file_name()) {
            (Some(d), Some(f)) => (d.to_path_buf(), f.to_string_lossy().into_owned()),
            _ => return vec![res(E_PARAM), dir_entry(None), Vec::new()],
        };
        let mut names: Vec<String> = match fs::read_dir(&dir) {
            Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect(),
            Err(e) => return vec![res(errno(&e)), dir_entry(None), Vec::new()],
        };
        names.sort();
        let find = Handle::Find {
            dir: dir.clone(),
            names,
            pos: 0,
            pattern,
            attr: le32(a[1]) as u8,
            mask: le32(a[2]) as u8,
        };
        let h = self.alloc(find);
        if h < 0 {
            return vec![res(h), dir_entry(None), Vec::new()];
        }
        let r = self.find_next(h as usize, le32(a[3]) as usize);
        if le32(&r[0]) as i32 != h {
            self.handles[h as usize] = None;
        }
        eprintln!("find {} -> {}", pat.display(), le32(&r[0]) as i32);
        r
    }

    fn xdelete(path: &Path, flag: u32) -> io::Result<i32> {
        let mut n = 0;
        for e in fs::read_dir(path)? {
            let e = e?;
            let p = e.path();
            if e.file_type()?.is_dir() {
                if flag & 0x10 != 0 {
                    n += Self::xdelete(&p, flag | 0x08)?;
                }
                if flag & 0x08 != 0 && fs::remove_dir(&p).is_ok() {
                    n += 1;
                }
            } else if flag & 0x04 != 0 {
                fs::remove_file(&p)?;
                n += 1;
            }
        }
        Ok(n)
    }

    fn folder_size(path: &Path, bsize: u64) -> io::Result<u64> {
        let mut clusters = 0;
        for e in fs::read_dir(path)? {
            let e = e?;
            let m = e.metadata()?;
            clusters += if m.is_dir() { Self::folder_size(&e.path(), bsize)? } else { m.size().div_ceil(bsize) };
        }
        Ok(clusters)
    }

    fn cmpt_read(&mut self, path: &[u8], req: &[u8]) -> Reply {
        let w = |i: usize| req.get(i * 4..).map_or(0, le32);
        let (map, flags, offset, whence, len) = (w(0), w(3), w(5) as i32, w(6), w(8));
        let (mut done, mut err, mut size, mut data) = (0u32, 0i32, 0u32, Vec::new());
        if req.len() < 40 || map & 0x11 != 0x11 {
            err = E_PARAM;
        } else {
            let h = self.open(path, flags);
            if h < 0 {
                err = h;
            } else {
                done |= 1;
                let f = self.file(&h.to_le_bytes()).unwrap();
                let step = (|| -> Result<(), (u32, i32)> {
                    if map & 0x2 != 0 {
                        size = f.metadata().map_err(|e| (0x2, errno(&e)))?.size() as u32;
                        done |= 0x2;
                    }
                    if map & 0x4 != 0 {
                        let pos = match whence {
                            0 => SeekFrom::Start(offset as u64),
                            1 => SeekFrom::Current(offset.into()),
                            2 => SeekFrom::End(offset.into()),
                            _ => return Err((0x4, E_PARAM)),
                        };
                        f.seek(pos).map_err(|e| (0x4, errno(&e)))?;
                        done |= 0x4;
                    }
                    if map & 0x8 != 0 {
                        data = vec![0; len as usize];
                        let n = read_full(f, &mut data).map_err(|e| (0x8, errno(&e)))?;
                        data.truncate(n);
                        done |= 0x8;
                    }
                    Ok(())
                })();
                if let Err((_, e)) = step {
                    err = e;
                }
                let c = self.close(h as u32);
                if c == 0 {
                    done |= 0x10;
                } else if err == 0 {
                    err = c;
                }
            }
        }
        let mut st = done.to_le_bytes().to_vec();
        st.extend_from_slice(&err.to_le_bytes());
        let n = data.len() as u32;
        vec![st, size.to_le_bytes().to_vec(), n.to_le_bytes().to_vec(), data]
    }

    fn op(&mut self, op: u32, a: &[&[u8]]) -> Option<Reply> {
        let need = match op {
            0x1001 | 0x100b | 0x100e | 0x1022 => 2,
            0x1002 | 0x1004 | 0x1011 => 3,
            0x1012 => 4,
            0x100c => 3,
            0x1003 | 0x100a | 0x100d | 0x1013 | 0x1018 => 2,
            0x1005 | 0x1007 | 0x1008 | 0x1009 | 0x100f | 0x1010 | 0x1014 | 0x1021 => 1,
            _ => 0,
        };
        if a.len() < need {
            return Some(vec![res(E_PARAM)]);
        }
        let r: Reply = match op {
            0x1001 => vec![res(self.open(a[0], le32(a[1])))],
            0x1011 => vec![res(self.open(a[0], le32(a[1]))), a[2].to_vec()],
            0x1002 => {
                let off = le32(a[1]) as i32;
                let pos = match le32(a[2]) {
                    0 => SeekFrom::Start(off as u64),
                    1 => SeekFrom::Current(off.into()),
                    2 => SeekFrom::End(off.into()),
                    _ => return Some(vec![res(E_PARAM)]),
                };
                vec![res(match self.file(a[0]) {
                    Ok(f) => f.seek(pos).map_or_else(|e| errno(&e), |p| p as i32),
                    Err(e) => e,
                })]
            }
            0x1003 => match self.file(a[0]) {
                Ok(f) => {
                    let mut buf = vec![0; (le32(a[1]) as usize).min(STREAM_MAX)];
                    match read_full(f, &mut buf) {
                        Ok(n) => {
                            buf.truncate(n);
                            vec![res(0), (n as u32).to_le_bytes().to_vec(), buf]
                        }
                        Err(e) => vec![res(errno(&e)), res(0), Vec::new()],
                    }
                }
                Err(e) => vec![res(e), res(0), Vec::new()],
            },
            0x1004 => {
                let n = (le32(a[2]) as usize).min(a[1].len());
                match self.file(a[0]) {
                    Ok(f) => match f.write_all(&a[1][..n]) {
                        Ok(()) => vec![res(0), (n as u32).to_le_bytes().to_vec()],
                        Err(e) => vec![res(errno(&e)), res(0)],
                    },
                    Err(e) => vec![res(e), res(0)],
                }
            }
            0x1005 | 0x1014 => vec![res(self.close(le32(a[0])))],
            0x1006 => {
                for h in 1..=HANDLES {
                    self.close(h as u32);
                }
                vec![res(0)]
            }
            0x1007 | 0x1008 | 0x100f => vec![res(match self.path(a[0]) {
                Ok(p) => {
                    let r = match op {
                        0x1007 => fs::DirBuilder::new().mode(0o770).create(&p),
                        0x1008 => fs::remove_dir(&p),
                        _ => fs::remove_file(&p),
                    };
                    eprintln!("op {op:#x} {} -> {:?}", p.display(), r.as_ref().err());
                    r.map_or_else(|e| errno(&e), |()| 0)
                }
                Err(e) => e,
            })],
            0x1009 => match self.file(a[0]) {
                Ok(f) => match f.metadata() {
                    Ok(m) => vec![res(0), (m.size() as u32).to_le_bytes().to_vec()],
                    Err(e) => vec![res(errno(&e)), res(0)],
                },
                Err(e) => vec![res(e), res(0)],
            },
            0x100a => vec![res(match self.path(a[0]) {
                Ok(p) if p.is_dir() => {
                    let bsize = statvfs(&p).map_or(4096, |s| s.f_bsize);
                    match Self::folder_size(&p, bsize) {
                        Ok(c) if le32(a[1]) == 2 => c as i32,
                        Ok(c) => (c * bsize) as i32,
                        Err(e) => errno(&e),
                    }
                }
                Ok(_) => E_PARAM,
                Err(e) => e,
            })],
            0x100b => vec![res(match (self.path(a[0]), self.path(a[1])) {
                (Ok(from), Ok(to)) => fs::rename(&from, &to).map_or_else(|e| errno(&e), |()| 0),
                (Err(e), _) | (_, Err(e)) => e,
            })],
            0x100c => vec![res(match (self.path(a[0]), self.path(a[1])) {
                (Ok(from), Ok(to)) => {
                    let flag = le32(a[2]);
                    let r = if flag & 0x1 != 0 { fs::copy(&from, &to).map(|_| ()) } else { fs::rename(&from, &to) };
                    match r {
                        Ok(()) if flag & 0x3 == 0x3 => fs::remove_file(&from).map_or_else(|e| errno(&e), |()| 0),
                        Ok(()) => 0,
                        Err(e) => errno(&e),
                    }
                }
                (Err(e), _) | (_, Err(e)) => e,
            })],
            0x100e => {
                let mut info = vec![0u8; 0x54];
                if let Some(st) = self.drive(b'Z').and_then(|z| statvfs(&z)) {
                    info[0x30..0x34].copy_from_slice(&512u32.to_le_bytes());
                    info[0x34..0x38].copy_from_slice(&((st.f_bsize / 512) as u32).to_le_bytes());
                    info[0x38..0x3c].copy_from_slice(&(st.f_blocks as u32).to_le_bytes());
                    info[0x40..0x44].copy_from_slice(&512u32.to_le_bytes());
                    vec![res(0), info]
                } else {
                    vec![res(E_GENERAL), info]
                }
            }
            0x1010 => vec![res(match self.path(a[0]) {
                Ok(p) => fs::metadata(&p).map_or(E_NOT_FOUND, |m| attributes(&m).into()),
                Err(e) => e,
            })],
            0x1012 => self.find_first(a),
            0x1013 => self.find_next(le32(a[0]) as usize, le32(a[1]) as usize),
            0x1015 | 0x1019 | 0x101c => vec![res(0)],
            0x1016 => vec![res(1)],
            0x1017 => {
                for h in 1..=HANDLES {
                    self.close(h as u32);
                }
                Vec::new()
            }
            0x1018 => vec![res(match self.path(a[0]) {
                Ok(p) => {
                    let flag = le32(a[1]);
                    if flag == 0 || flag & !0x1c != 0 {
                        E_PARAM
                    } else {
                        match Self::xdelete(&p, flag) {
                            Ok(n) if flag & 0x18 == 0x18 => n + i32::from(fs::remove_dir(&p).is_ok()),
                            Ok(n) => n,
                            Err(e) => errno(&e),
                        }
                    }
                }
                Err(e) => e,
            })],
            0x101a => vec![res(if a.first().map(|x| le32(x)) == Some(4) { i32::from(b'Z') } else { E_PARAM })],
            0x101b => vec![res(
                match a.first().and_then(|x| self.drive(le32(x) as u8)).and_then(|p| statvfs(&p)) {
                    Some(st) => st.f_bsize as i32,
                    None => E_DRIVE,
                },
            )],
            0x101d..=0x1020 => vec![res(E_UNSUPPORTED)],
            0x1021 => vec![res(E_GENERAL)],
            0x1022 => self.cmpt_read(a[0], a[1]),
            _ => {
                eprintln!("unknown op {op:#x} with {} params", a.len());
                vec![res(E_PARAM)]
            }
        };
        Some(r)
    }
}

fn read_full(f: &mut File, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

fn params(stream: &[u8]) -> Option<Vec<&[u8]>> {
    let count = le32(stream.get(4..8)?) as usize;
    if count > MAX_PARAMS {
        return None;
    }
    let mut off = 8;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let len = le32(stream.get(off..off + 4)?) as usize;
        off += 4;
        out.push(stream.get(off..off + len)?);
        off += len.next_multiple_of(4);
    }
    Some(out)
}

fn send(dev: &mut File, hdr: &[u8], op: u32, reply: &Reply) -> io::Result<()> {
    let mut body = Vec::new();
    body.extend_from_slice(&(reply.len() as u32).to_le_bytes());
    for p in reply {
        body.extend_from_slice(&(p.len() as u32).to_le_bytes());
        body.extend_from_slice(p);
        body.resize(body.len().next_multiple_of(4), 0);
    }
    if body.len() + 4 > STREAM_MAX {
        eprintln!("reply for {op:#x} too large ({} bytes), dropped", body.len());
        return Ok(());
    }
    let chunks: Vec<&[u8]> = if HDR + 4 + body.len() <= FRAG_MAX { vec![&body] } else { body.chunks(FRAG_DATA).collect() };
    for (i, chunk) in chunks.iter().enumerate() {
        let mut pkt = hdr[..HDR].to_vec();
        let d0 = le32(&pkt[0..4]) & !MORE | if i + 1 < chunks.len() { MORE } else { 0 };
        pkt[0..4].copy_from_slice(&d0.to_le_bytes());
        pkt.extend_from_slice(&(op | RESP).to_le_bytes());
        pkt.extend_from_slice(chunk);
        let len = pkt.len() as u32;
        pkt[4..8].copy_from_slice(&len.to_le_bytes());
        dev.write_all(&pkt)?;
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let mut args = std::env::args().skip(1);
    let dev_path = args.next().unwrap_or_else(|| "/dev/ccci_fs".into());
    let root = PathBuf::from(args.next().unwrap_or_else(|| "/var/lib/ccci-fs/data".into()));

    for sub in ["nvdata/md", "nvdata/md_cmn", "protect_f/md", "protect_s/md"] {
        fs::DirBuilder::new().recursive(true).mode(0o770).create(root.join(sub))?;
    }
    let mut dev = OpenOptions::new().read(true).write(true).open(&dev_path)?;
    let mut fsd = Fsd { root, handles: (0..=HANDLES).map(|_| None).collect() };
    let mut slots: HashMap<u32, Vec<u8>> = HashMap::new();
    let mut buf = vec![0u8; 0x4000];
    eprintln!("ccci-fsd: serving {dev_path} from {}", fsd.root.display());

    loop {
        let n = dev.read(&mut buf)?;
        let pkt = &buf[..n];
        if n < HDR + 4 {
            eprintln!("short packet ({n} bytes)");
            continue;
        }
        let slot = le32(&pkt[12..16]);
        if slot >= SLOTS {
            eprintln!("bad slot {slot}");
            continue;
        }
        let more = le32(&pkt[0..4]) & MORE != 0;
        let stream = slots.entry(slot).or_default();
        if stream.is_empty() {
            stream.extend_from_slice(pkt);
        } else {
            stream[..HDR].copy_from_slice(&pkt[..HDR]);
            stream.extend_from_slice(&pkt[HDR + 4..]);
        }
        if more {
            continue;
        }
        let whole = slots.remove(&slot).unwrap_or_default();
        let hdr = &whole[..HDR];
        let op = le32(&whole[HDR..HDR + 4]);
        let reply = match params(&whole[HDR..]) {
            Some(a) if a.iter().all(|p| !p.is_empty()) => fsd.op(op, &a),
            _ => {
                eprintln!("op {op:#x}: bad parameters");
                Some(vec![res(E_PARAM)])
            }
        };
        if let Some(r) = reply {
            if op != 0x1003 && op != 0x1002 && op != 0x1009 && op != 0x1005 {
                eprintln!("op {op:#x} slot {slot} -> {:?}", r.first().map(|x| le32(x) as i32));
            }
            send(&mut dev, hdr, op, &r)?;
        }
    }
}
