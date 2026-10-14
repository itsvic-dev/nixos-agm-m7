{ lib, pkgs, ... }:

{
  nixpkgs.overlays = [
    (final: prev: {
      mesa =
        (prev.mesa.override {
          galliumDrivers = [ "zink" ];
          vulkanDrivers = [ "imagination" ];
          vulkanLayers = [ ];
          eglPlatforms = [ "wayland" ];
        }).overrideAttrs
          (old: {
            version = "26.2.3";
            src = pkgs.fetchFromGitHub {
              owner = "itsvic-dev";
              repo = "mesa";
              rev = "67422666a47ffdcb9339e1707cadd772efd8b370";
              hash = "sha256-JgACXFMvHuagCzO41sA99kOd4wbsmfGClaaL1rOdr+4=";
            };
            patches = old.patches ++ [ ./patches/mesa-zink-shm.patch ];
            # rusticl is not built.
            postFixup = builtins.replaceStrings [ "$opencl/lib/libRusticlOpenCL.so" ] [ "" ] old.postFixup;
            # Dozen is not built, so nothing lands in this output.
            postInstall = old.postInstall + ''
              mkdir -p $spirv2dxil
            '';
            mesonFlags = old.mesonFlags ++ [
              (lib.mesonBool "gallium-rusticl" false)
              (lib.mesonBool "teflon" false)
              (lib.mesonOption "video-codecs" "")
              (lib.mesonEnable "xlib-lease" false)
              (lib.mesonBool "draw-use-llvm" false)
              (lib.mesonEnable "glx" false)
              (lib.mesonEnable "gles1" false)
              (lib.mesonEnable "gallium-va" false)
              (lib.mesonBool "gallium-extra-hud" false)
            ];
          });
    })
  ];

  hardware.graphics.enable = true;

  environment.sessionVariables = {
    PVR_I_WANT_A_BROKEN_VULKAN_DRIVER = "1";
    # Sway renders with pixman, so present through wl_shm from a linear image.
    MESA_VK_WSI_DEBUG = "sw,linear";
    MESA_LOADER_DRIVER_OVERRIDE = "zink";
  };

  environment.systemPackages = [
    pkgs.vulkan-tools
    (pkgs.callPackage ../tools/vk-compute/package.nix { })
    (pkgs.callPackage ../tools/vk-image/package.nix { })
    (pkgs.callPackage ../tools/vk-raster/package.nix { })
    (pkgs.callPackage ../tools/vk-triangle/package.nix { })
  ];
}
