#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BINARY="${SCRIPT_DIR}/../target/aarch64-unknown-linux-gnu/release/examples/epd2in13_v4_status"

if [ ! -f "$BINARY" ]; then
    echo "Error: Binary not found at $BINARY"
    echo "Build first with:"
    echo "  cargo build --target aarch64-unknown-linux-gnu --example epd2in13_v4_status --features epd2in13_v4 --release"
    exit 1
fi

echo "Installing epd-status display service..."

echo "  Copying binary to /usr/local/bin/epd2in13_v4_status"
cp "$BINARY" /usr/local/bin/epd2in13_v4_status
chmod 755 /usr/local/bin/epd2in13_v4_status

echo "  Copying unit files to /etc/systemd/system/"
cp "$SCRIPT_DIR/epd-status.service" /etc/systemd/system/epd-status.service
cp "$SCRIPT_DIR/epd-status.timer" /etc/systemd/system/epd-status.timer

echo "  Reloading systemd daemon"
systemctl daemon-reload

echo "  Enabling and starting timer"
systemctl enable epd-status.timer
systemctl start epd-status.timer

echo ""
echo "Installed. Status:"
systemctl status epd-status.timer --no-pager

echo ""
echo "To run immediately: sudo systemctl start epd-status.service"
echo "To view logs:       journalctl -u epd-status.service -n 20"
echo "To stop:            sudo systemctl stop epd-status.timer"
echo "To uninstall:       sudo systemctl disable epd-status.timer && sudo rm /etc/systemd/system/epd-status.{service,timer} /usr/local/bin/epd2in13_v4_status"
