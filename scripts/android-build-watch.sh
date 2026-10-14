#!/usr/bin/env bash
# usage: [VM=android-build] android-build-watch.sh [announce-seconds] [check-seconds] [eta-window-seconds]  (says the Android build progress, ETA and result)
set -uo pipefail
announce=${1:-600}
check=${2:-15}
window=${3:-1800}
vm=${VM:-android-build}
out=/Volumes/Android/lineage/out

progress() {
	orb run -m $vm python3 - $out/build_progress.pb <<'EOF'
import sys
d = open(sys.argv[1], 'rb').read()
i, f = 0, {}
while i < len(d):
    k = d[i]; i += 1
    v = s = 0
    while True:
        b = d[i]; i += 1; v |= (b & 0x7f) << s; s += 7
        if b < 0x80: break
    f[k >> 3] = v
print(f.get(1, 0), f.get(2, 0), f.get(4, 0))
EOF
}

started() {
	orb run -m $vm bash -c 'pid=$(pgrep -f "[s]oong_ui --build-mode") && stat -c %Y /proc/$pid'
}

main() {
	orb run -m $vm bash -c 'pid=$(pgrep -f "[s]iso .*combined") &&
		[ "$(stat -c %Y '$out'/build_progress.pb)" -ge "$(stat -c %Y /proc/$pid)" ]'
}

errors() {
	orb run -m $vm bash -c '[ -s '$out'/error.log ] && [ "$(stat -c %Y '$out'/error.log)" -ge '$1' ] &&
		grep -m1 "^FAILED: " '$out'/error.log | cut -c9- | cut -d" " -f1'
}

notify() {
	echo "$(date +%H:%M) $1"
	say "$1"
}

ended() {
	if [ -n "$failed_name" ]; then
		notify "Android build failed${pct:+ at $pct%} in ${failed_name##*:}"
	elif [ -n "$failed_said" ]; then
		notify "Android build failed${pct:+ at $pct%}"
	else
		notify "Android build finished"
	fi
}

until start=$(started); do sleep "$check"; done
echo "$(date +%H:%M) build started"
pct= last=0 failed_said= failed_name=
ts=() ds=()
while :; do
	if ! now_start=$(started); then
		sleep 5
		failed_name=$(errors "$start") || true
		[ -z "$failed_name" ] && [ -n "$failed_said" ] && failed_name=$(errors 0)
		ended
		exit 0
	fi
	if [ "$now_start" != "$start" ]; then
		ended
		echo "$(date +%H:%M) build restarted"
		start=$now_start pct= last=0 failed_said= failed_name=
		ts=() ds=()
	fi
	if main; then
		[ ${#ts[@]} -eq 0 ] && last=$(($(date +%s) - announce + 120))
		total=0
		for _ in 1 2 3 4 5; do
			read -r total done failed < <(progress 2>/dev/null) && [ "${total:-0}" -gt 0 ] && break
			total=0
			sleep 2
		done
		if [ "$total" -gt 0 ]; then
			now=$(date +%s)
			pct=$((100 * done / total))
			if [ "$failed" -gt 0 ] && [ -z "$failed_said" ]; then
				failed_name=$(errors "$start") || true
				notify "Android build has $failed failed steps at $pct%${failed_name:+ in ${failed_name##*:}}"
				failed_said=1
			fi
			ts+=("$now") ds+=("$done")
			while [ $((now - ts[0])) -gt "$window" ]; do
				ts=("${ts[@]:1}") ds=("${ds[@]:1}")
			done
			if [ $((now - last)) -ge "$announce" ]; then
				eta=
				if [ $((now - ts[0])) -ge 300 ] && [ "$done" -gt "${ds[0]}" ]; then
					mins=$(((total - done) * (now - ts[0]) / (done - ds[0]) / 60))
					eta=$(printf ', ETA %dh %dm' $((mins / 60)) $((mins % 60)))
				fi
				echo "$(date +%H:%M) $done/$total, $(((done - ds[0]) * 60 / (now - ts[0] + 1)))/min over $(((now - ts[0]) / 60))m"
				notify "Android build $pct% complete$eta"
				last=$now
			fi
		fi
	else
		ts=() ds=()
	fi
	sleep "$check"
done
