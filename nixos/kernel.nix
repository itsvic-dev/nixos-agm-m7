{
  lib,
  stdenv,
  linuxManualConfig,
  bison,
  flex,
  bc,
  perl,
  src,
}:

let
  configfile = stdenv.mkDerivation {
    name = "agm-m7-kconfig";
    inherit src;
    nativeBuildInputs = [
      bison
      flex
      bc
      perl
    ];
    dontConfigure = true;
    buildPhase = ''
      make ARCH=arm64 allnoconfig
      make ARCH=arm64 agm-m7.config
    '';
    installPhase = "cp .config $out";
  };
in
linuxManualConfig {
  inherit src configfile;
  version = "7.2.9-agm-m7";
  modDirVersion = "7.2.9-agm-m7";
  allowImportFromDerivation = true;
}
