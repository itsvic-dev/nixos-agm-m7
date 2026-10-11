#!/usr/bin/env bash
log=${1:-/dev/stdout}
{
	echo "== interfaces"
	for i in $(ifconfig -l); do
		ifconfig "$i" | grep -q 'status: active' || continue
		echo "-- $i"; ifconfig "$i" | grep -E 'ether|inet|status'
	done
	echo "== new ifaces (CDC/ECM)"
	networksetup -listallhardwareports 2>/dev/null | grep -B1 -A2 -iE 'cdc|ecm|gadget|linux|usb' || true
	ping -c 3 -t 5 172.16.42.1
	for i in $(ifconfig -l); do
		ifconfig "$i" 2>/dev/null | grep -q 'status: active' || continue
		ping6 -c 2 -i 1 "fe80::1%$i" >/dev/null 2>&1
	done
	ndp -an 2>/dev/null | grep -i 'fe80' | head
} > "$log" 2>&1
cat "$log"
