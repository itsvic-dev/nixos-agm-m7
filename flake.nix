{
  description = "NixOS for the AGM M7";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs =
    { nixpkgs, ... }:
    let
      agm-m7 = nixpkgs.lib.nixosSystem {
        system = "aarch64-linux";
        modules = [ ./nixos/configuration.nix ];
      };
    in
    {
      nixosConfigurations.agm-m7 = agm-m7;
      packages.aarch64-linux.rootfs = agm-m7.config.system.build.rootfs;
    };
}
