{
  runCommand,
  kernel,
  initrd,
  kernelParams,
}:

runCommand "agm-m7-boot-parts" { } ''
  mkdir $out
  gzip -9 -n < ${kernel}/Image > $out/Image.gz
  cp ${kernel}/dtbs/mediatek/mt6739-agm-m7.dtb $out/
  cp ${initrd}/initrd $out/initrd
  echo "${toString kernelParams} init=/nix/var/nix/profiles/system/init" > $out/cmdline
''
