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

# this causes a 500ms delay for udp packets from port 20000:20100
delay() {
  pfctl -e
  dnctl pipe 1 config delay 500

  (cat /etc/pf.conf && cat) <<__PF__ | pfctl -f -
dummynet-anchor "mop"
anchor "mop"
__PF__
  cat <<__MOP__ | pfctl -a mop -f -
dummynet in proto udp from port 20000:20100 to any pipe 1
dummynet in proto udp from any to port 20000:20100 pipe 1
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
