{ pkgs, ... }:

let
  # Overriding alsa-ucm-conf would rebuild alsa-lib and everything above it, so point ALSA at a merged tree instead.
  ucm2 = pkgs.runCommand "agm-m7-ucm2" { } ''
    mkdir -p $out
    cp -rs ${pkgs.alsa-ucm-conf}/share/alsa/ucm2/. $out/
    chmod -R u+w $out
    cp -r ${./ucm2}/. $out/
  '';
in
{
  security.rtkit.enable = true;

  services.pipewire = {
    enable = true;
    alsa.enable = true;
    pulse.enable = true;
  };

  environment.sessionVariables.ALSA_CONFIG_UCM2 = "${ucm2}";
  systemd.user.services.pipewire.environment.ALSA_CONFIG_UCM2 = "${ucm2}";
  systemd.user.services.wireplumber.environment.ALSA_CONFIG_UCM2 = "${ucm2}";

  environment.systemPackages = [ pkgs.alsa-utils ];
}
