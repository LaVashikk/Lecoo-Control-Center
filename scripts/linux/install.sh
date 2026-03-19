#!/bin/bash
set -e

if [ "$EUID" -ne 0 ]; then
  echo "Please run as root: sudo ./install.sh"
  exit 1
fi

echo "Installing Lecoo Control Center..."

cp lecoo-ec-daemon /usr/local/bin/
cp lecoo-ctrl /usr/local/bin/
chmod +x /usr/local/bin/lecoo-ec-daemon
chmod +x /usr/local/bin/lecoo-ctrl

mkdir -p /var/lib/lecoo-control
chmod 755 /var/lib/lecoo-control

cat <<EOF > /etc/systemd/system/lecoo-daemon.service
[Unit]
Description=Lecoo EC Control Daemon
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/lecoo-ec-daemon
Restart=on-failure
RestartSec=5
User=root
# Демону нужны права root для доступа к /dev/port

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable lecoo-daemon
systemctl restart lecoo-daemon

echo "========================================"
echo "Installation complete!"
echo "Service status:"
systemctl is-active lecoo-daemon
echo ""
echo "You can now use 'lecoo-ctrl help' in your terminal."
echo "========================================"