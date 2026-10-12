use std::process::Command;

use serde_json::Value;

const SINK: &str = "@DEFAULT_AUDIO_SINK@";
const SOURCE: &str = "@DEFAULT_AUDIO_SOURCE@";

#[derive(Clone)]
pub struct Output {
    pub name: String,
    profile: i64,
}

#[derive(Default)]
pub struct State {
    pub available: bool,
    pub volume: f32,
    pub muted: bool,
    pub mic_muted: bool,
    pub outputs: Vec<Output>,
    pub output: Option<usize>,
    device: Option<i64>,
}

fn wpctl(args: &[&str]) -> Option<String> {
    let out = Command::new(env!("AGM_WPCTL")).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Parses `wpctl get-volume` output such as "Volume: 0.40 [MUTED]".
fn read_volume(target: &str) -> Option<(f32, bool)> {
    let out = wpctl(&["get-volume", target])?;
    let v = out.split_whitespace().nth(1)?.parse().ok()?;
    Some((v, out.contains("[MUTED]")))
}

/// Finds the ALSA card in `pw-dump` and lists its output profiles.
fn card() -> Option<(i64, Vec<Output>, Option<usize>)> {
    let out = Command::new(env!("AGM_PW_DUMP")).output().ok()?;
    let objects: Vec<Value> = serde_json::from_slice(&out.stdout).ok()?;
    let device = objects.iter().find(|o| {
        o["type"] == "PipeWire:Interface:Device" && o["info"]["props"]["device.api"] == "alsa"
    })?;
    let params = &device["info"]["params"];
    let active = params["Profile"].get(0).and_then(|p| p["index"].as_i64());
    let mut routes: Vec<&Value> = params["EnumRoute"].as_array()?.iter().filter(|r| r["direction"] == "Output").collect();
    routes.sort_by_key(|r| -r["priority"].as_i64().unwrap_or(0));
    let mut outputs = Vec::new();
    let mut current = None;
    for route in routes {
        let profiles: Vec<i64> = route["profiles"].as_array().map_or(Vec::new(), |p| p.iter().filter_map(Value::as_i64).collect());
        let Some(&profile) = profiles.first() else { continue };
        if active.is_some_and(|a| profiles.contains(&a)) {
            current = Some(outputs.len());
        }
        outputs.push(Output { name: route["description"].as_str().unwrap_or("Output").to_string(), profile });
    }
    Some((device["id"].as_i64()?, outputs, current))
}

impl State {
    pub fn load() -> State {
        let Some((volume, muted)) = read_volume(SINK) else { return State::default() };
        let mic_muted = read_volume(SOURCE).is_some_and(|(_, m)| m);
        let (device, outputs, output) = card().map_or((None, Vec::new(), None), |(d, o, c)| (Some(d), o, c));
        State { available: true, volume, muted, mic_muted, outputs, output, device }
    }

    pub fn set_volume(&self, v: f32) {
        wpctl(&["set-volume", SINK, &format!("{:.2}", v.clamp(0.0, 1.0))]);
    }

    pub fn set_muted(&self, muted: bool) {
        wpctl(&["set-mute", SINK, if muted { "1" } else { "0" }]);
    }

    pub fn set_mic_muted(&self, muted: bool) {
        wpctl(&["set-mute", SOURCE, if muted { "1" } else { "0" }]);
    }

    pub fn set_output(&self, index: usize) {
        if let (Some(device), Some(output)) = (self.device, self.outputs.get(index)) {
            wpctl(&["set-profile", &device.to_string(), &output.profile.to_string()]);
        }
    }

    pub fn output_name(&self) -> String {
        self.output.and_then(|i| self.outputs.get(i)).map_or("Unknown".into(), |o| o.name.clone())
    }
}
