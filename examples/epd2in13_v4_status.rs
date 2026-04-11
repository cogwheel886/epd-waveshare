//! System status display for Pi Zero 2W on Waveshare 2.13" V4 (SSD1680).
//! Reads real system data via sysinfo + pisugar socket and renders to e-paper.
//!
//! Uses partial refresh on subsequent runs to minimize e-paper wear.
//! Two state files in `/tmp` (cleared on reboot) control the sequence:
//!
//! 1. First run  — full refresh, creates `epd_status_initialized`
//! 2. Second run — `display_part_base_image` (establishes base in both RAM
//!    banks with one full refresh), creates `epd_status_base_set`
//! 3. Third+ runs — `display_partial` only (true partial waveform, no flashing)

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyleBuilder},
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use epd_waveshare::{
    epd2in13_v4::{Display2in13, Epd2in13},
    prelude::*,
};
use linux_embedded_hal::{
    gpio_cdev::{Chip, LineRequestFlags},
    spidev::{self, SpidevOptions},
    CdevPin, Delay, SpidevDevice,
};
use std::io::{BufRead, BufReader, Write as IoWrite};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use sysinfo::{Components, Disks, System};

const STATE_FILE: &str = "/tmp/epd_status_initialized";
const BASE_FILE: &str = "/tmp/epd_status_base_set";

// ---- Data collection ----

struct StatusData {
    hostname: String,
    ip: String,
    datetime: String,
    uptime: String,
    cpu: String,
    memory: String,
    disk: String,
    battery: String,
    voltage: String,
}

impl StatusData {
    fn collect() -> Self {
        let mut sys = System::new_all();
        std::thread::sleep(Duration::from_millis(200));
        sys.refresh_cpu_usage();

        let (battery, voltage) = Self::read_battery();

        StatusData {
            hostname: Self::read_hostname(),
            ip: Self::read_ip(),
            datetime: Self::read_datetime(),
            uptime: Self::read_uptime(),
            cpu: Self::read_cpu(&sys),
            memory: Self::read_memory(&sys),
            disk: Self::read_disk(),
            battery,
            voltage,
        }
    }

    fn read_hostname() -> String {
        System::host_name()
            .unwrap_or_else(|| "unknown".into())
            .to_uppercase()
    }

    fn read_ip() -> String {
        if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if sock.connect("8.8.8.8:53").is_ok() {
                if let Ok(addr) = sock.local_addr() {
                    return addr.ip().to_string();
                }
            }
        }
        "no IP".into()
    }

    fn read_datetime() -> String {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let days_since_epoch = secs / 86400;
        let time_of_day = secs % 86400;
        let hours = time_of_day / 3600;
        let minutes = (time_of_day % 3600) / 60;
        let (year, month, day) = days_to_ymd(days_since_epoch);
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            year, month, day, hours, minutes
        )
    }

    fn read_uptime() -> String {
        let total = System::uptime();
        let days = total / 86400;
        let hours = (total % 86400) / 3600;
        let mins = (total % 3600) / 60;
        if days > 0 {
            format!("Up: {}d {}h {}m", days, hours, mins)
        } else {
            format!("Up: {}h {}m", hours, mins)
        }
    }

    fn read_cpu(sys: &System) -> String {
        let usage = sys.global_cpu_usage();
        let temp = Components::new_with_refreshed_list()
            .iter()
            .find(|c| c.label().contains("cpu") || c.label().contains("thermal"))
            .and_then(|c| c.temperature())
            .map(|t| format!("{:.1}C", t))
            .unwrap_or_else(|| {
                std::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp")
                    .ok()
                    .and_then(|s| s.trim().parse::<f64>().ok())
                    .map(|t| format!("{:.1}C", t / 1000.0))
                    .unwrap_or_else(|| "??C".into())
            });
        format!("CPU: {:.0}% {}", usage, temp)
    }

    fn read_memory(sys: &System) -> String {
        let used_mb = sys.used_memory() / (1024 * 1024);
        let total_mb = sys.total_memory() / (1024 * 1024);
        format!("RAM: {}/{}MB", used_mb, total_mb)
    }

    fn read_disk() -> String {
        let disks = Disks::new_with_refreshed_list();
        for disk in disks.list() {
            if disk.mount_point() == Path::new("/") {
                let total = disk.total_space() as f64 / 1_073_741_824.0;
                let used = (disk.total_space() - disk.available_space()) as f64 / 1_073_741_824.0;
                return format!("DSK: {:.1}/{:.1}GB", used, total);
            }
        }
        "DSK: ??".into()
    }

    fn read_battery() -> (String, String) {
        let bat_pct = query_pisugar("get battery").and_then(|r| parse_pisugar_float(&r));
        let charging = query_pisugar("get battery_charging")
            .map(|r| r.contains("true"))
            .unwrap_or(false);

        let bat_line = match bat_pct {
            Some(pct) => {
                let indicator = if charging { " CHG" } else { "" };
                format!("BAT: {:.0}%{}", pct, indicator)
            }
            None => "BAT: N/A".into(),
        };

        let volt_line = query_pisugar("get battery_v")
            .and_then(|r| parse_pisugar_float(&r))
            .map(|v| format!("VOLT: {:.2}V", v))
            .unwrap_or_else(|| "VOLT: N/A".into());

        (bat_line, volt_line)
    }

    fn summary(&self) -> String {
        format!(
            "{} | {} | {} | {} | {} | {} | {} | {}",
            self.hostname,
            self.ip,
            self.datetime,
            self.uptime,
            self.cpu,
            self.memory,
            self.disk,
            self.battery
        )
    }
}

fn query_pisugar(cmd: &str) -> Option<String> {
    let mut stream = UnixStream::connect("/tmp/pisugar-server.sock").ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    write!(stream, "{}\n", cmd).ok()?;
    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response).ok()?;
    Some(response.trim().to_string())
}

fn parse_pisugar_float(response: &str) -> Option<f64> {
    response.split(':').nth(1)?.trim().parse().ok()
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    loop {
        let diy = if is_leap(year) { 366 } else { 365 };
        if days < diy {
            break;
        }
        days -= diy;
        year += 1;
    }
    let leap = is_leap(year);
    let md: [u64; 12] = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1u64;
    for &m in &md {
        if days < m {
            break;
        }
        days -= m;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

// ---- Rendering ----

fn render(display: &mut Display2in13, data: &StatusData) {
    let stroke = PrimitiveStyle::with_stroke(Color::Black, 1);
    let fill_black = PrimitiveStyle::with_fill(Color::Black);

    let white_on_black = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(Color::White)
        .background_color(Color::Black)
        .build();
    let black_on_white = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(Color::Black)
        .background_color(Color::White)
        .build();
    let right_align = TextStyleBuilder::new()
        .alignment(Alignment::Right)
        .baseline(Baseline::Top)
        .build();

    // Header bar: hostname left, IP right
    Rectangle::new(Point::new(0, 0), Size::new(122, 13))
        .into_styled(fill_black)
        .draw(display)
        .ok();
    Text::with_baseline(
        &data.hostname,
        Point::new(2, 2),
        white_on_black,
        Baseline::Top,
    )
    .draw(display)
    .ok();
    Text::with_text_style(&data.ip, Point::new(120, 2), white_on_black, right_align)
        .draw(display)
        .ok();

    let mut y = 14;

    // Divider
    Line::new(Point::new(0, y), Point::new(121, y))
        .into_styled(stroke)
        .draw(display)
        .ok();
    y += 2;

    // Date/time
    Text::with_baseline(
        &data.datetime,
        Point::new(2, y),
        black_on_white,
        Baseline::Top,
    )
    .draw(display)
    .ok();
    y += 14;

    // Uptime
    Text::with_baseline(
        &data.uptime,
        Point::new(2, y),
        black_on_white,
        Baseline::Top,
    )
    .draw(display)
    .ok();
    y += 14;

    // Divider
    Line::new(Point::new(0, y), Point::new(121, y))
        .into_styled(stroke)
        .draw(display)
        .ok();
    y += 2;

    // CPU
    Text::with_baseline(&data.cpu, Point::new(2, y), black_on_white, Baseline::Top)
        .draw(display)
        .ok();
    y += 14;

    // RAM
    Text::with_baseline(
        &data.memory,
        Point::new(2, y),
        black_on_white,
        Baseline::Top,
    )
    .draw(display)
    .ok();
    y += 14;

    // Disk
    Text::with_baseline(&data.disk, Point::new(2, y), black_on_white, Baseline::Top)
        .draw(display)
        .ok();
    y += 14;

    // Divider
    Line::new(Point::new(0, y), Point::new(121, y))
        .into_styled(stroke)
        .draw(display)
        .ok();
    y += 2;

    // Battery
    Text::with_baseline(
        &data.battery,
        Point::new(2, y),
        black_on_white,
        Baseline::Top,
    )
    .draw(display)
    .ok();
    y += 14;

    // Voltage
    Text::with_baseline(
        &data.voltage,
        Point::new(2, y),
        black_on_white,
        Baseline::Top,
    )
    .draw(display)
    .ok();
}

// ---- Main ----

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = StatusData::collect();

    // EPD setup
    let mut spi = SpidevDevice::open("/dev/spidev0.0")?;
    let options = SpidevOptions::new()
        .bits_per_word(8)
        .max_speed_hz(4_000_000)
        .mode(spidev::SpiModeFlags::SPI_MODE_0)
        .build();
    spi.configure(&options)?;

    let mut chip = Chip::new("/dev/gpiochip0")?;
    let busy = CdevPin::new(
        chip.get_line(24)?
            .request(LineRequestFlags::INPUT, 0, "epd-busy")?,
    )?;
    let dc = CdevPin::new(
        chip.get_line(25)?
            .request(LineRequestFlags::OUTPUT, 0, "epd-dc")?,
    )?;
    let rst = CdevPin::new(
        chip.get_line(17)?
            .request(LineRequestFlags::OUTPUT, 1, "epd-rst")?,
    )?;
    let pwr = CdevPin::new(
        chip.get_line(18)?
            .request(LineRequestFlags::OUTPUT, 0, "epd-pwr")?,
    )?;

    let mut delay = Delay;
    let mut epd = Epd2in13::new_with_pwr(&mut spi, busy, dc, rst, &mut delay, None, pwr)?;

    // Render to framebuffer
    let mut display = Display2in13::default();
    display.clear(Color::White).ok();
    render(&mut display, &data);

    let buf = display.buffer();
    let initialized = Path::new(STATE_FILE).exists();
    let base_set = Path::new(BASE_FILE).exists();

    if !initialized {
        // First run since boot — full refresh
        epd.update_frame(&mut spi, buf, &mut delay)?;
        epd.display_frame(&mut spi, &mut delay)?;
        std::fs::write(STATE_FILE, "")?;
        println!("full | {}", data.summary());
    } else if !base_set {
        // Second run — establish partial base (writes both RAM banks, one full refresh)
        epd.display_part_base_image(&mut spi, buf, &mut delay)?;
        std::fs::write(BASE_FILE, "")?;
        println!("base | {}", data.summary());
    } else {
        // All subsequent runs — true partial refresh only
        epd.display_partial(&mut spi, buf, &mut delay)?;
        println!("partial | {}", data.summary());
    }

    Ok(())
}
