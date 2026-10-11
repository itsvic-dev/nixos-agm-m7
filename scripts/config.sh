#!/usr/bin/env bash
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
frag=$here/${KSRC:-linux}/arch/arm64/configs/agm-m7.config

"$here/scripts/kmake" allnoconfig >/dev/null
"$here/scripts/kmake" agm-m7.config >/dev/null 2>&1
missing=0
while read -r line; do
	[ -z "$line" ] && continue
	grep -qxF "$line" "$here/${KOUT:-out}/.config" || { echo "not applied: $line"; missing=1; }
done < "$frag"
exit $missing
