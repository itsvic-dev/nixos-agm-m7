use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    for name in ["quad.vert", "probe.frag"] {
        let src = format!("shaders/{name}");
        println!("cargo:rerun-if-changed={src}");
        let status = Command::new("glslangValidator")
            .args(["-V", "-o"])
            .arg(out.join(format!("{name}.spv")))
            .arg(&src)
            .status()
            .expect("glslangValidator not found");
        assert!(status.success(), "glslangValidator failed on {src}");
    }
}
