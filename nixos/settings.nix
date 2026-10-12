{ ... }:

{
  # The settings app changes the time zone through timedated, so NixOS must not own /etc/localtime.
  time.timeZone = null;
  systemd.tmpfiles.rules = [ "L /etc/localtime - - - - /etc/zoneinfo/Europe/Warsaw" ];

  security.polkit.enable = true;
  security.polkit.extraConfig = ''
    polkit.addRule(function(action, subject) {
      if (action.id == "org.freedesktop.timedate1.set-timezone" && subject.user == "user") {
        return polkit.Result.YES;
      }
    });
  '';
}
