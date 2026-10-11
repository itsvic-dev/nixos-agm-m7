#!/usr/bin/env bash
# usage: tnet.sh <wait s> < commands   (runs commands over telnet on 172.16.42.1)
w=${1:-5}
{ sleep 2; cat; sleep "$w"; } | nc -w "$((w + 5))" 172.16.42.1 23 | LC_ALL=C tr -d '\377\373\375\001\003\037\033'
