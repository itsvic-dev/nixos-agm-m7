{ pkgs, ... }:

let
  mount = pkgs.writeShellScript "sdcard-mount" ''
    n=$(cat /sys/class/block/$1/partition 2>/dev/null || echo 1)
    dir=/media/sdcard
    [ "$n" -le 1 ] || dir=/media/sdcard-$n
    exec ${pkgs.systemd}/bin/systemd-mount --no-block --collect --owner=user \
      -o nosuid,nodev /dev/$1 $dir
  '';
in
{
  services.udev.extraRules = ''
    SUBSYSTEM=="block", ACTION=="add", ENV{ID_PATH}=="platform-11240000.mmc", ENV{ID_FS_USAGE}=="filesystem", TAG+="systemd", ENV{SYSTEMD_WANTS}+="sdcard-mount@%k.service"
  '';

  systemd.services."sdcard-mount@" = {
    description = "Mount microSD card partition %I";
    serviceConfig = {
      Type = "oneshot";
      ExecStart = "${mount} %I";
    };
  };

  environment.systemPackages = [
    pkgs.dosfstools
    pkgs.exfatprogs
  ];
}
