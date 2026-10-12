{
  runCommand,
  python3,
  bootParts,
}:

runCommand "agm-m7-boot.img" { nativeBuildInputs = [ python3 ]; } ''
  python3 ${../scripts/mkboot.py} \
    --kernel ${bootParts}/Image.gz \
    --dtb ${bootParts}/mt6739-agm-m7.dtb \
    --ramdisk ${bootParts}/initrd \
    --cmdline "$(cat ${bootParts}/cmdline)" \
    -o $out
  size=$(stat -c %s $out)
  max=25165824
  [ "$size" -le "$max" ] || { echo "boot.img is $size bytes, the boot partition holds $max"; exit 1; }
''
