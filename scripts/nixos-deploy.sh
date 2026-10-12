#!/usr/bin/env bash
# usage: nixos-deploy.sh [switch|boot]
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
mode=${1:-boot}
host=root@172.16.42.1
export NIX_SSHOPTS="-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null"

sys=$(nix build "$here#nixosConfigurations.agm-m7.config.system.build.toplevel" --no-link --print-out-paths)
nix copy --no-check-sigs --to "ssh-ng://$host" "$sys"
ssh $NIX_SSHOPTS "$host" "nix-env -p /nix/var/nix/profiles/system --set $sys && $sys/bin/switch-to-configuration $mode"
