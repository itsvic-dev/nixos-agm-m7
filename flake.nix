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
      kernel = pkgs.callPackage ./nixos/kernel.nix { src = linux; };
      agm-m7 = nixpkgs.lib.nixosSystem {
        system = "aarch64-linux";
        modules = [ ./nixos/configuration.nix ];
      };
    in
    {
      nixosConfigurations.agm-m7 = agm-m7;
      packages.aarch64-linux = {
        inherit kernel;
        boot-parts = pkgs.callPackage ./nixos/boot-parts.nix { inherit kernel; };
        rootfs = agm-m7.config.system.build.rootfs;
      };
    };
}
