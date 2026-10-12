{ lib, modemmanager }:

let
  plugin = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./meson.build
      ./77-mm-mtk-ccci.rules
      (lib.fileset.fileFilter (file: file.hasExt "c" || file.hasExt "h") ./.)
    ];
  };
in
modemmanager.overrideAttrs (old: {
  postPatch = (old.postPatch or "") + ''
    cp -r ${plugin} src/plugins/mtk-ccci
    chmod -R u+w src/plugins/mtk-ccci
    substituteInPlace src/plugins/meson.build \
      --replace-fail "foreach plugin_name, plugin_data: plugins" \
                     "subdir('mtk-ccci')
    foreach plugin_name, plugin_data: plugins"
  '';
})
