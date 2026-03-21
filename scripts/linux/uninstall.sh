#!/bin/bash
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

if [ "$EUID" -ne 0 ]; then
  echo -e "${RED}Please run as root: sudo ./uninstall.sh${NC}"
  exit 1
fi

if [ ! -f /usr/local/bin/lecoo-ec-daemon ] && \
   [ ! -f /etc/systemd/system/lecoo-daemon.service ]; then
    echo -e "${YELLOW}Lecoo Control Center is not installed.${NC}"
    exit 0
fi

echo -e "${YELLOW}Uninstalling Lecoo Control Center...${NC}"

if systemctl is-active --quiet lecoo-daemon 2>/dev/null; then
    echo "Stopping lecoo-daemon..."
    systemctl stop lecoo-daemon
fi

if systemctl is-enabled --quiet lecoo-daemon 2>/dev/null; then
    systemctl disable lecoo-daemon
fi

rm -f /etc/systemd/system/lecoo-daemon.service
systemctl daemon-reload

rm -f /usr/local/bin/lecoo-ec-daemon
rm -f /usr/local/bin/lecoo-ctrl

if [ -d /var/lib/lecoo-control ]; then
    read -r -p "Remove saved data in /var/lib/lecoo-control? [y/N] " response
    if [[ "$response" =~ ^[Yy]$ ]]; then
        rm -rf /var/lib/lecoo-control
        echo "Data removed."
    else
        echo "Data kept."
    fi
fi

echo -e "${GREEN}Uninstalled successfully.${NC}"
