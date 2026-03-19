#!/bin/bash
set -e

if [ "$EUID" -ne 0 ]; then
  echo "Please run as root: sudo ./uninstall.sh"
  exit 1
fi

echo "Uninstalling Lecoo Control Center..."

systemctl stop lecoo-daemon || true
systemctl disable lecoo-daemon || true

rm -f /etc/systemd/system/lecoo-daemon.service
systemctl daemon-reload

rm -f /usr/local/bin/lecoo-ec-daemon
rm -f /usr/local/bin/lecoo-ctrl

rm -rf /var/lib/lecoo-control

echo "Uninstalled successfully."