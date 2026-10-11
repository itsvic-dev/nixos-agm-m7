{
  pkgs,
  kernel,
}:

let
  inherit (pkgs) pkgsStatic;

  dmctl = pkgsStatic.stdenv.mkDerivation {
    name = "dmctl";
    src = ../tools;
    dontConfigure = true;
    buildPhase = "$CC -O2 -static -Wall -o dmctl dmctl.c";
    installPhase = "install -D dmctl $out/bin/dmctl";
  };

  initrd = pkgs.makeInitrd {
    compressor = "gzip";
    contents = [
      {
        object = ../initramfs/init;
        symlink = "/init";
      }
      {
        object = "${pkgsStatic.busybox}/bin/busybox";
        symlink = "/bin/busybox";
      }
      {
        object = "${dmctl}/bin/dmctl";
        symlink = "/bin/dmctl";
      }
    ];
  };
in
pkgs.runCommand "agm-m7-boot-parts" { } ''
  mkdir $out
  gzip -9 -n < ${kernel}/Image > $out/Image.gz
  cp ${kernel}/dtbs/mediatek/mt6739-agm-m7.dtb $out/
  cp ${initrd}/initrd $out/initrd.cpio.gz
''
