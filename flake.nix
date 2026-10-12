{
  description = "NixOS for the AGM M7";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    linux = {
      url = "github:itsvic-dev/linux-mt6739-agm-m7/agm-m7";
      flake = false;
    };
  };

  outputs =
    { nixpkgs, linux, ... }:
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
        specialArgs = { inherit kernel; };
        modules = [ ./nixos/configuration.nix ];
      };
    in
    {
      nixosConfigurations.agm-m7 = agm-m7;
      packages.aarch64-linux = {
        inherit kernel;
        boot-parts = pkgs.callPackage ./pkgs/boot-parts.nix {
          inherit (agm-m7.config.system.build) kernel;
          initrd = agm-m7.config.system.build.initialRamdisk;
          inherit (agm-m7.config.boot) kernelParams;
        };
        rootfs = agm-m7.config.system.build.rootfs;
        agm-shell = pkgs.callPackage ./tools/shell/package.nix { };
      };
      checks.aarch64-linux.t9 = pkgs.callPackage ./tools/t9/test/wayland.nix {
        t9 = pkgs.callPackage ./tools/t9/package.nix { };
      };
      checks.aarch64-linux.agm-shell = pkgs.callPackage ./tools/shell/test.nix {
        agm-shell = pkgs.callPackage ./tools/shell/package.nix { };
      };
    };
}
