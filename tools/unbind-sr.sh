#!/bin/bash
# sr_mod を unbind してカーネルの disc アクセス（blkid 等のプローブ）を止める。
# /dev/sg0 は残るので friidump の SG_IO は使える。
set -u
D=0:0:0:0
if [ -e "/sys/bus/scsi/drivers/sr/$D" ]; then
  echo -n "$D" > /sys/bus/scsi/drivers/sr/unbind
  echo "unbound sr $D"
else
  echo "sr device $D が見つかりません"
fi
sleep 1
echo "=== devices ==="
ls -l /dev/sr0 /dev/sg0 2>/dev/null || true
