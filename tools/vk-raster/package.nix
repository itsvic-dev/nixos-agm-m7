{
  lib,
  rustPlatform,
  glslang,
  vulkan-loader,
}:

rustPlatform.buildRustPackage {
  pname = "vk-raster";
  version = "0.1.0";
  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      ./build.rs
      ./shaders
      ./src
    ];
  };
  cargoLock.lockFile = ./Cargo.lock;

  nativeBuildInputs = [ glslang ];

  postFixup = ''
    patchelf --add-rpath ${lib.makeLibraryPath [ vulkan-loader ]} $out/bin/vk-raster
  '';
}
