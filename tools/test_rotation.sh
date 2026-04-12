#!/usr/bin/env bash
# test_rotation.sh — test all four display rotations on a target device
#
# Usage:
#   bash tools/test_rotation.sh ruby
#   bash tools/test_rotation.sh coral
#
# Cycles through 0/90/180/270, waits 20s between each,
# then restores the original rotation from the device.

set -euo pipefail

TARGET="${1:-}"
if [[ -z "$TARGET" ]]; then
    echo "Usage: $0 <host>" >&2
    exit 1
fi

# Read current rotation before we start so we can restore it
ORIGINAL=$(ssh "$TARGET" "grep '^rotation=' /etc/epd-waveshare.conf 2>/dev/null | cut -d= -f2" || echo "0")
echo "Current rotation on $TARGET: ${ORIGINAL}°"
echo ""

for rot in 0 90 180 270; do
    echo "--- Rotation ${rot}° ---"
    ssh "$TARGET" "sudo tee /etc/epd-waveshare.conf > /dev/null << 'CONF'
# /etc/epd-waveshare.conf
rotation=${rot}
color_invert=false
CONF"
    ssh "$TARGET" "sudo /usr/local/bin/epd2in13_v4_status 2>&1" \
        || ssh "$TARGET" "sudo /home/aken/epd3in52_ruby_status 2>&1"
    echo "Waiting 20s..."
    sleep 20
done

# Restore original rotation
echo "--- Restoring rotation ${ORIGINAL}° ---"
ssh "$TARGET" "sudo tee /etc/epd-waveshare.conf > /dev/null << CONF
# /etc/epd-waveshare.conf
rotation=${ORIGINAL}
color_invert=false
CONF"
ssh "$TARGET" "sudo /usr/local/bin/epd2in13_v4_status 2>&1" \
    || ssh "$TARGET" "sudo /home/aken/epd3in52_ruby_status 2>&1"

echo ""
echo "Done. $TARGET restored to rotation=${ORIGINAL}°"
