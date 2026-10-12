# SPDX-License-Identifier: GPL-2.0-or-later

plugins += {'plugin-mtk-ccci': {
  'plugin': true,
  'module': {
    'sources': files(
      'mm-bearer-mtk-ccci.c',
      'mm-broadband-modem-mtk-ccci.c',
      'mm-plugin-mtk-ccci.c',
    ),
    'include_directories': plugins_incs,
    'c_args': '-DMM_MODULE_NAME="mtk-ccci"',
  },
}}

plugins_udev_rules += files('77-mm-mtk-ccci.rules')
