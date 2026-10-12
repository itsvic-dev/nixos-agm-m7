{
  stdenv,
  glslang,
  vulkan-headers,
  vulkan-loader,
}:

stdenv.mkDerivation {
  name = "vk-triangle";
  src = ./.;

  nativeBuildInputs = [ glslang ];
  buildInputs = [
    vulkan-headers
    vulkan-loader
  ];

  buildPhase = ''
    glslangValidator -V --vn tri_vert -o tri_vert.h tri.vert
    glslangValidator -V --vn tri_frag -o tri_frag.h tri.frag
    glslangValidator -V --vn read_comp -o read_comp.h read.comp
    glslangValidator -V --vn store_comp -o store_comp.h store.comp
    $CC -O2 -Wall -o vk-triangle main.c -lvulkan
  '';

  installPhase = ''
    install -Dm755 vk-triangle $out/bin/vk-triangle
  '';
}
