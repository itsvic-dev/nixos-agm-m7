// Prints the text in a persistent_ram console zone (earlycon=pstoreram, U-Boot's ramconsole).
use std::env;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::process::exit;

const SIG: u32 = 0x4347_4244;
const PROT_READ: i32 = 1;
const MAP_SHARED: i32 = 1;

extern "C" {
    fn mmap(addr: *mut u8, len: usize, prot: i32, flags: i32, fd: i32, off: i64) -> *mut u8;
}

fn parse(s: &str) -> usize {
    let r = match s.strip_prefix("0x") {
        Some(h) => usize::from_str_radix(h, 16),
        None => s.parse(),
    };
    r.unwrap_or_else(|_| {
        eprintln!("bad number: {s}");
        exit(2)
    })
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() > 3 || args.iter().any(|a| a == "-h") {
        eprintln!("usage: ramconsole [phys addr, default 0x4445f000] [zone size, default 0x40000]");
        exit(2);
    }
    let addr = args.get(1).map_or(0x4445_f000, |s| parse(s));
    let size = args.get(2).map_or(0x40000, |s| parse(s));

    let f = OpenOptions::new().read(true).open("/dev/mem").unwrap_or_else(|e| {
        eprintln!("/dev/mem: {e}");
        exit(1)
    });
    let p = unsafe { mmap(std::ptr::null_mut(), size, PROT_READ, MAP_SHARED, f.as_raw_fd(), addr as i64) };
    if p as isize == -1 {
        eprintln!("mmap: {}", io::Error::last_os_error());
        exit(1);
    }
    // The zone is device memory, so copy it with volatile reads.
    let zone: Vec<u8> = (0..size).map(|i| unsafe { p.add(i).read_volatile() }).collect();

    let word = |o: usize| u32::from_le_bytes(zone[o..o + 4].try_into().unwrap()) as usize;
    let data = &zone[12..];
    let (sig, start, len) = (word(0) as u32, word(4), word(8));
    if sig != SIG || start >= data.len() || len > data.len() {
        eprintln!("no console at {addr:#x}: sig {sig:#x}, start {start}, size {len}");
        exit(1);
    }
    let mut out = io::stdout().lock();
    let res = if len < data.len() {
        out.write_all(&data[..len])
    } else {
        out.write_all(&data[start..]).and_then(|_| out.write_all(&data[..start]))
    };
    if let Err(e) = res {
        if e.kind() != io::ErrorKind::BrokenPipe {
            eprintln!("{e}");
            exit(1);
        }
    }
}
