# Native arm64 LineageOS build

Build in the `android-build-arm64` OrbStack VM:

```
source /Volumes/Projects/agm-m7-work/android/arm64-host/env.sh
m bootimage superimage
```

Patches 0001-0014 are from https://actinis.io/blog/building-aosp-on-arm64.
0008 and 0010 are their reverted records and are not applied. 0009 drops
the `slices` import hunk, which the tree already has. They are committed on
the `arm64-host` repo branch in build/make, build/soong, prebuilts/build-tools,
prebuilts/jdk/jdk21, system/sepolicy and external/lfi/lfi-verifier.

Prebuilts:

- `prebuilts/clang/host/linux-arm64`, `prebuilts/go/linux-arm64`: AOSP branch
  `mirror-google-llvm-r614150-release`, from `.repo/local_manifests/arm64-host.xml`.
  The clang project has a local commit that deletes its Android.bp (the x86
  one declares the modules) and links `clang-r584948 -> clang-r584948b`.
- `prebuilts/rust-toolchain/linux-arm64/1.93.1`: rustup `1.93.1-aarch64-unknown-linux-musl`
  with the Android targets, rust-src, rustfmt, clippy and llvm-tools. Not in a repo project.
  `lib/libgcc_s.so.1` is from Alpine 3.22 `libgcc-14.2.0-r6` (aarch64): the musl rustc needs
  it, and the VM needs the `musl` package for `/lib/ld-musl-aarch64.so.1`.
  `bin/{rustc,rustdoc,clippy-driver}` are sh wrappers that set `RUSTC_BOOTSTRAP=1` and run
  `*.real`: Soong passes `-Z` flags, which upstream stable rejects.
- `prebuilts/build-tools/linux-arm64/bin/bison`: a wrapper that sets `BISON_PKGDATADIR` and runs
  `bison.real`. The binary has the linux-x86 install path built in and cannot find its data.
- `prebuilts/jdk/jdk21/linux-arm64`: Ubuntu `openjdk-21-jdk-headless`. Untracked in the jdk21 project.
- `prebuilts/jdk/jdk8/linux-arm64 -> linux-x86`: Soong only reads the JDK 8 jars (`rt.jar`, `jce.jar`).
