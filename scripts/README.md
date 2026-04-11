# EPD Status Display

Systemd service and timer for the Waveshare 2.13" V4 e-paper status display on Pi Zero 2W nodes.

## What it does

Runs `epd2in13_v4_status` on a schedule to display system info on the e-paper:
hostname, IP, date/time, uptime, CPU/temp, RAM, disk, and PiSugar battery status.

## Hardware

- Waveshare 2.13" e-Paper HAT V4 (SSD1680 controller)
- GPIO: PWR=18, RST=17, DC=25, BUSY=24, SPI CE0
- SPI: `/dev/spidev0.0`, 4MHz, mode 0

## Build

From the repo root on the build host:

```bash
cargo build --target aarch64-unknown-linux-gnu --example epd2in13_v4_status --features epd2in13_v4 --release
```

## Install

Copy the repo (or at minimum `scripts/` and the built binary) to the Pi, then:

```bash
sudo ./scripts/install_epd_status.sh
```

This installs the binary to `/usr/local/bin/`, copies the systemd units, and enables the timer.

## Timing

| Setting | Default | Description |
|---------|---------|-------------|
| `OnBootSec` | 45s | Delay after boot before first update (wait for network) |
| `OnUnitActiveSec` | 5min | Interval between updates |
| `AccuracySec` | 30s | Allows systemd to batch timer wakeups for power efficiency |

To change the refresh interval, edit `/etc/systemd/system/epd-status.timer`:

```bash
sudo systemctl edit epd-status.timer
```

Add an override:

```ini
[Timer]
OnUnitActiveSec=10min
```

Then reload: `sudo systemctl daemon-reload`

## Commands

```bash
# Run immediately (don't wait for timer)
sudo systemctl start epd-status.service

# Check timer status
systemctl status epd-status.timer

# View recent logs
journalctl -u epd-status.service -n 20

# Stop the timer
sudo systemctl stop epd-status.timer

# Disable (won't start on boot)
sudo systemctl disable epd-status.timer

# Uninstall everything
sudo systemctl disable epd-status.timer
sudo rm /etc/systemd/system/epd-status.{service,timer}
sudo rm /usr/local/bin/epd2in13_v4_status
sudo systemctl daemon-reload
```
