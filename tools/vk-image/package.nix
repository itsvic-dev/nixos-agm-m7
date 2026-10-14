{
  stdenv,
  glslang,
  vulkan-headers,
  vulkan-loader,
}:

stdenv.mkDerivation {
  name = "vk-image";
  src = ./.;

  nativeBuildInputs = [ glslang ];
  buildInputs = [
    vulkan-headers
    vulkan-loader
  ];

  buildPhase = ''
    glslangValidator -V --vn load_spv -o load.spv.h load.comp
    glslangValidator -V --vn store_spv -o store.spv.h store.comp
    $CC -O2 -Wall -o vk-image main.c -lvulkan
  '';

  installPhase = ''
    install -Dm755 vk-image $out/bin/vk-image
  '';
}
