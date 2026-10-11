{ pkgsStatic }:

pkgsStatic.stdenv.mkDerivation {
  name = "dmctl";
  src = ../tools;
  dontConfigure = true;
  buildPhase = "$CC -O2 -static -Wall -o dmctl dmctl.c";
  installPhase = "install -D dmctl $out/bin/dmctl";
}
