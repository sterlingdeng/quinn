#!/bin/bash

reset() {
    dnctl -q flush
    pfctl -f /etc/pf.conf
    pfctl -d
}

status() {
  echo
  dnctl list
}

# 1000 Kbit/s link capacity, 75 ms delay, 50 slots in queue
# see man dnctl, pfctl for details
delay() {
  pfctl -e
  dnctl pipe 1 config bw 1000Kbit/s delay 75 queue 50
  dnctl pipe 2 config bw 1000Kbit/s delay 75 queue 50

  (cat /etc/pf.conf && cat) <<__PF__ | pfctl -f -
dummynet-anchor "mop"
anchor "mop"
__PF__
  cat <<__MOP__ | pfctl -a mop -f -
dummynet in proto udp from port 7777:7777 to port 7778:7778 pipe 1
dummynet in proto udp from port 7778:7778 to port 7777:7777 pipe 2
__MOP__

}

case $1 in
delay)
  delay
  ;;
reset)
  reset
  status
  ;;
*)
  echo $0 "delay | reset"
  ;;
esac


