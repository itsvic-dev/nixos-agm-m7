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
    glslangValidator -V --vn tri_ubo_vert -o tri_ubo_vert.h tri_ubo.vert
    glslangValidator -V --vn tri_tex_frag -o tri_tex_frag.h tri_tex.frag
    glslangValidator -V --vn tri_vary_vert -o tri_vary_vert.h tri_vary.vert
    glslangValidator -V --vn tri_vary_frag -o tri_vary_frag.h tri_vary.frag
    glslangValidator -V --vn tri_deriv_frag -o tri_deriv_frag.h tri_deriv.frag
    glslangValidator -V --vn probe_vert -o probe_vert.h probe.vert
    glslangValidator -V --vn probe_frag -o probe_frag.h probe.frag
    glslangValidator -V --vn read_comp -o read_comp.h read.comp
    glslangValidator -V --vn store_comp -o store_comp.h store.comp
    $CC -O2 -Wall -o vk-triangle main.c -lvulkan
  '';

  installPhase = ''
    install -Dm755 vk-triangle $out/bin/vk-triangle
  '';
}
