{ lib, pkgs, ... }:

{
  nixpkgs.overlays = [
    (final: prev: {
      mesa =
        (prev.mesa.override {
          galliumDrivers = [ ];
          vulkanDrivers = [ "imagination" ];
          vulkanLayers = [ ];
          eglPlatforms = [ "wayland" ];
        }).overrideAttrs
          (old: {
            patches = old.patches ++ [ ./patches/mesa-ge8100.patch ];
            # No Gallium drivers and no rusticl, so there is nothing to add an RPATH to.
            postFixup = builtins.replaceStrings [ "patchelf --add-rpath" ] [ "true" ] old.postFixup;
            # Dozen is not built, so nothing lands in this output.
            postInstall = old.postInstall + ''
              mkdir -p $spirv2dxil
            '';
            mesonFlags = old.mesonFlags ++ [
              (lib.mesonBool "gallium-rusticl" false)
              (lib.mesonBool "teflon" false)
              (lib.mesonOption "video-codecs" "")
              (lib.mesonEnable "xlib-lease" false)
              (lib.mesonBool "opengl" false)
              (lib.mesonEnable "egl" false)
              (lib.mesonEnable "glx" false)
              (lib.mesonEnable "gbm" false)
              (lib.mesonEnable "glvnd" false)
              (lib.mesonEnable "gles1" false)
              (lib.mesonEnable "gles2" false)
              (lib.mesonEnable "gallium-va" false)
              (lib.mesonBool "gallium-extra-hud" false)
            ];
          });
    })
  ];

  hardware.graphics.enable = true;

  environment.systemPackages = [
    pkgs.vulkan-tools
    (pkgs.callPackage ../tools/vk-compute/package.nix { })
  ];
}
