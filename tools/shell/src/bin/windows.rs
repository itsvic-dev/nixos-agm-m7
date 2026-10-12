//! Moves every new window to its own workspace, so apps run full screen without Sway's tab titles.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use serde_json::Value;

const RUN_COMMAND: u32 = 0;
const GET_WORKSPACES: u32 = 1;
const SUBSCRIBE: u32 = 2;
const HOME: u64 = 1;

fn send(stream: &mut UnixStream, kind: u32, payload: &str) -> std::io::Result<()> {
    let mut msg = b"i3-ipc".to_vec();
    msg.extend((payload.len() as u32).to_ne_bytes());
    msg.extend(kind.to_ne_bytes());
    msg.extend(payload.as_bytes());
    stream.write_all(&msg)
}

fn receive(stream: &mut UnixStream) -> std::io::Result<Value> {
    let mut header = [0u8; 14];
    stream.read_exact(&mut header)?;
    let len = u32::from_ne_bytes(header[6..10].try_into().unwrap()) as usize;
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body)?;
    Ok(serde_json::from_slice(&body).unwrap_or(Value::Null))
}

fn request(stream: &mut UnixStream, kind: u32, payload: &str) -> std::io::Result<Value> {
    send(stream, kind, payload)?;
    receive(stream)
}

fn free_workspace(commands: &mut UnixStream) -> std::io::Result<u64> {
    let workspaces = request(commands, GET_WORKSPACES, "")?;
    let used: Vec<u64> = workspaces.as_array().into_iter().flatten().filter_map(|w| w["num"].as_u64()).collect();
    Ok((HOME + 1..).find(|n| !used.contains(n)).unwrap())
}

fn main() -> std::io::Result<()> {
    let socket = std::env::var("SWAYSOCK").expect("SWAYSOCK is not set");
    let mut events = UnixStream::connect(&socket)?;
    let mut commands = UnixStream::connect(&socket)?;
    request(&mut events, SUBSCRIBE, r#"["window"]"#)?;

    loop {
        let event = receive(&mut events)?;
        if event["change"] != "new" {
            continue;
        }
        let Some(id) = event["container"]["id"].as_u64() else { continue };
        let n = free_workspace(&mut commands)?;
        request(&mut commands, RUN_COMMAND, &format!("[con_id={id}] move container to workspace number {n}; workspace number {n}"))?;
    }
}
