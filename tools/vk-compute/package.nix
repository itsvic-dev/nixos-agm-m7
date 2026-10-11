{
  stdenv,
  glslang,
  vulkan-headers,
  vulkan-loader,
}:

stdenv.mkDerivation {
  name = "vk-compute";
  src = ./.;

  nativeBuildInputs = [ glslang ];
  buildInputs = [
    vulkan-headers
    vulkan-loader
  ];

  buildPhase = ''
    glslangValidator -V --vn fill_spv -o fill.spv.h fill.comp
    $CC -O2 -Wall -o vk-compute main.c -lvulkan
  '';

  installPhase = ''
    install -Dm755 vk-compute $out/bin/vk-compute
  '';
}
