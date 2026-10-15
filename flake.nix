{
  description = "NixOS for the AGM M7";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    linux = {
      url = "github:itsvic-dev/linux-mt6739/linux-7.2";
      flake = false;
    };
    firmware = {
      url = "github:itsvic-dev/agm-m7-firmware";
      flake = false;
    };
    ccci-fsd = {
      url = "github:itsvic-dev/ccci-fsd";
      flake = false;
    };
  };

  outputs =
    {
      nixpkgs,
      linux,
      firmware,
      ccci-fsd,
      ...
    }:
    let
      pkgs = nixpkgs.legacyPackages.aarch64-linux;
      kernel = import ./pkgs/kernel.nix {
        inherit (pkgs)
          stdenv
          linuxManualConfig
          bison
          flex
          bc
          perl
          ;
        src = linux;
      };
      agm-m7 = nixpkgs.lib.nixosSystem {
        system = "aarch64-linux";
        specialArgs = { inherit kernel firmware ccci-fsd; };
        modules = [ ./nixos/configuration.nix ];
      };
    in
    {
      nixosConfigurations.agm-m7 = agm-m7;
      packages.aarch64-linux = {
        inherit kernel;
        boot-parts = agm-m7.config.system.build.bootParts;
        boot-img = agm-m7.config.system.build.bootImage;
        rootfs = agm-m7.config.system.build.rootfs;
        sd-image = agm-m7.config.system.build.sdImage;
        agm-shell = pkgs.callPackage ./tools/shell/package.nix { };
        ccci-fsd = pkgs.callPackage "${ccci-fsd}/package.nix" { };
        modemmanager-agm = pkgs.callPackage ./tools/mm-plugin-mtk-ccci/package.nix { };
      };
      checks.aarch64-linux.t9 = pkgs.callPackage ./tools/t9/test/wayland.nix {
        t9 = pkgs.callPackage ./tools/t9/package.nix { };
      };
      checks.aarch64-linux.agm-shell = pkgs.callPackage ./tools/shell/test.nix {
        agm-shell = pkgs.callPackage ./tools/shell/package.nix { };
      };
    };
}
