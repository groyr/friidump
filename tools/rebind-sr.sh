#!/bin/bash
# sr_mod を bind して /dev/sr0 を復帰させる（unbind の逆。dump 後の後始末用）。
# 使い方: sudo bash rebind-sr.sh
set -u
if [ -e /dev/sr0 ]; then
  echo "既に /dev/sr0 があります: $(ls -l /dev/sr0)"
  exit 0
fi
id="$(ls /sys/bus/scsi/devices/ 2>/dev/null | grep -m1 -E '^[0-9]+:[0-9]+:[0-9]+:[0-9]+$')"
if [ -z "$id" ]; then
  echo "SCSI デバイス（host:target:lun）が見つかりません" >&2
  exit 1
fi
echo -n "$id" > /sys/bus/scsi/drivers/sr/bind
sleep 1
if [ -e /dev/sr0 ]; then
  echo "rebind 成功 ($id): $(ls -l /dev/sr0)"
else
  echo "rebind 失敗 ($id)" >&2
  exit 1
fi
