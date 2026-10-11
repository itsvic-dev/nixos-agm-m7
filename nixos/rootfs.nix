{
  config,
  pkgs,
  modulesPath,
  ...
}:

let
  toplevel = config.system.build.toplevel;
in
{
  system.build.rootfs = pkgs.callPackage "${modulesPath}/../lib/make-ext4-fs.nix" {
    storePaths = [ toplevel ];
    volumeLabel = "nixos";
    populateImageCommands = ''
      mkdir -p files/sbin
      ln -s /nix/var/nix/profiles/system/init files/sbin/init
      mkdir -p files/nix/var/nix/profiles
      ln -s ${toplevel} files/nix/var/nix/profiles/system-1-link
      ln -s system-1-link files/nix/var/nix/profiles/system
    '';
  };

  systemd.services.nix-register-rootfs = {
    wantedBy = [ "multi-user.target" ];
    before = [
      "nix-daemon.socket"
      "nix-daemon.service"
      "shutdown.target"
    ];
    after = [ "local-fs.target" ];
    conflicts = [ "shutdown.target" ];
    unitConfig = {
      DefaultDependencies = false;
      ConditionPathExists = "/nix-path-registration";
    };
    serviceConfig.Type = "oneshot";
    script = ''
      ${config.nix.package.out}/bin/nix-store --load-db < /nix-path-registration
      touch /etc/NIXOS
      ${config.nix.package.out}/bin/nix-env -p /nix/var/nix/profiles/system --set /run/current-system
      rm -f /nix-path-registration
    '';
  };
}
