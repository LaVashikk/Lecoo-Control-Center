#!/bin/bash
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# ── Check /dev/port ───
check_dev_port() {
    if [ ! -e /dev/port ]; then
        echo -e "${RED}Error: /dev/port does not exist.${NC}"
        echo "Your kernel may be built without CONFIG_DEVPORT."
        echo ""
        echo "Check: zgrep CONFIG_DEVPORT /proc/config.gz 2>/dev/null"
        echo "   or: grep CONFIG_DEVPORT /boot/config-$(uname -r) 2>/dev/null"
        return 1
    fi

    if [ ! -c /dev/port ]; then
        echo -e "${RED}Error: /dev/port exists but is not a character device.${NC}"
        return 1
    fi

    if [ ! -r /dev/port ]; then
        echo -e "${RED}Error: /dev/port is not readable.${NC}"
        echo "Check permissions: ls -la /dev/port"
        return 1
    fi

    if [ ! -w /dev/port ]; then
        echo -e "${RED}Error: /dev/port is not writable.${NC}"
        echo "Check permissions: ls -la /dev/port"
        return 1
    fi

    if ! dd if=/dev/port bs=1 count=1 skip=0x64 2>/dev/null | cat > /dev/null 2>&1; then
        echo -e "${RED}Error: Cannot read from /dev/port.${NC}"
        echo "Possible causes:"
        echo "  - SELinux is blocking access (check: getenforce)"
        echo "  - AppArmor profile is active"
        echo "  - Running inside a container"
        return 1
    fi

    if systemd-detect-virt --quiet 2>/dev/null; then
        local virt_type
        virt_type=$(systemd-detect-virt 2>/dev/null || echo "unknown")
        echo -e "${YELLOW}Warning: Running in virtualized environment ($virt_type).${NC}"
        echo "EC access through /dev/port may not work correctly."
    fi

    echo -e "${GREEN}/dev/port is accessible.${NC}"
    return 0
}

if [ "$EUID" -ne 0 ]; then
  echo -e "${RED}Please run as root: sudo ./install.sh${NC}"
  exit 1
fi

for bin in lecoo-ec-daemon lecoo-ctrl; do
  if [ ! -f "./$bin" ]; then
    echo -e "${RED}Error: ./$bin not found in current directory${NC}"
    exit 1
  fi
done

if ! check_dev_port; then
    echo ""
    read -r -p "Continue installation anyway? [y/N] " response
    if [[ ! "$response" =~ ^[Yy]$ ]]; then
        echo "Installation cancelled."
        exit 1
    fi
    echo -e "${YELLOW}Proceeding at your own risk...${NC}"
fi

echo -e "${YELLOW}Installing/Updating Lecoo Control Center...${NC}"

if systemctl is-active --quiet lecoo-daemon 2>/dev/null; then
    echo "Stopping existing lecoo-daemon service..."
    systemctl stop lecoo-daemon
fi

install -m 755 lecoo-ec-daemon /usr/local/bin/
install -m 755 lecoo-ctrl /usr/local/bin/

mkdir -p /var/lib/lecoo-control
chmod 755 /var/lib/lecoo-control

if [ ! -c /dev/port ]; then
    echo -e "${YELLOW}Warning: /dev/port not found. The daemon may not work on this system.${NC}"
fi

cat <<'EOF' > /etc/systemd/system/lecoo-daemon.service
[Unit]
Description=Lecoo EC Control Daemon
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/lecoo-ec-daemon
Restart=on-failure
RestartSec=5
User=root

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable --now lecoo-daemon

echo ""
echo "Waiting for daemon to start..."
sleep 2

if systemctl is-active --quiet lecoo-daemon; then
    echo -e "${GREEN}========================================"
    echo "Installation/Update complete!"
    echo "Daemon is running."
    echo -e "========================================${NC}"
else
    echo -e "${RED}========================================"
    echo "WARNING: Daemon failed to start!"
    echo -e "========================================${NC}"
    echo ""
    echo "Last log lines:"
    journalctl -u lecoo-daemon -n 15 --no-pager
    echo ""
    echo -e "${YELLOW}Binaries are installed, but the service is not running.${NC}"
    echo "Try: journalctl -u lecoo-daemon -f"
    exit 1
fi

echo ""
echo "You can now use 'lecoo-ctrl help' in your terminal."
