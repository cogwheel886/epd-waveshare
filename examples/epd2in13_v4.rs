use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyleBuilder},
    prelude::*,
    primitives::{Circle, Line, PrimitiveStyle, Rectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use embedded_hal::delay::DelayNs;
use epd_waveshare::{
    epd2in13_v4::{Display2in13, Epd2in13},
    prelude::*,
};
use linux_embedded_hal::{
    gpio_cdev::{Chip, LineRequestFlags},
    spidev::{self, SpidevOptions},
    CdevPin, Delay, SpidevDevice,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    delay.delay_ms(100);

    epd.reinit(&mut spi, &mut delay)?;
    delay.delay_ms(100);

    // Prepare framebuffer — white background
    let mut display = Display2in13::default();
    display.clear(Color::White).ok();

    let stroke = PrimitiveStyle::with_stroke(Color::Black, 1);
    let fill_black = PrimitiveStyle::with_fill(Color::Black);

    // --- Bottom border: outline around full display ---
    Rectangle::new(Point::new(0, 0), Size::new(122, 250))
        .into_styled(stroke)
        .draw(&mut display)?;

    // --- Header bar: filled black with white text ---
    Rectangle::new(Point::new(0, 0), Size::new(122, 21))
        .into_styled(fill_black)
        .draw(&mut display)?;

    let white_text = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(Color::White)
        .background_color(Color::Black)
        .build();
    let center = TextStyleBuilder::new()
        .alignment(Alignment::Center)
        .baseline(Baseline::Top)
        .build();
    Text::with_text_style("EPD V4 RUST", Point::new(61, 6), white_text, center)
        .draw(&mut display)?;

    // --- Horizontal divider ---
    Line::new(Point::new(0, 21), Point::new(121, 21))
        .into_styled(stroke)
        .draw(&mut display)?;

    // --- Outline rectangle ---
    Rectangle::new(Point::new(0, 25), Size::new(51, 51))
        .into_styled(stroke)
        .draw(&mut display)?;

    // --- Diagonal lines through outline rectangle ---
    Line::new(Point::new(0, 25), Point::new(50, 75))
        .into_styled(stroke)
        .draw(&mut display)?;
    Line::new(Point::new(50, 25), Point::new(0, 75))
        .into_styled(stroke)
        .draw(&mut display)?;

    // --- Filled rectangle ---
    Rectangle::new(Point::new(55, 25), Size::new(51, 51))
        .into_styled(fill_black)
        .draw(&mut display)?;

    // --- Filled circle ---
    Circle::new(Point::new(5, 80), 40)
        .into_styled(fill_black)
        .draw(&mut display)?;

    // --- Outline circle ---
    Circle::new(Point::new(55, 80), 40)
        .into_styled(stroke)
        .draw(&mut display)?;

    // --- Text rows ---
    let black_text = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(Color::Black)
        .background_color(Color::White)
        .build();

    Text::with_baseline("SSD1680 OK", Point::new(2, 130), black_text, Baseline::Top)
        .draw(&mut display)?;
    Text::with_baseline("122x250px", Point::new(2, 145), black_text, Baseline::Top)
        .draw(&mut display)?;
    Text::with_baseline("Driver test", Point::new(2, 160), black_text, Baseline::Top)
        .draw(&mut display)?;

    // Buffer summary
    let buf = display.buffer();
    let non_white = buf.iter().filter(|&&b| b != 0xFF).count();
    println!("Buffer: {} bytes, {} non-white", buf.len(), non_white);

    epd.update_frame(&mut spi, buf, &mut delay)?;
    epd.display_frame(&mut spi, &mut delay)?;

    // No sleep — PWR stays high, image persists
    println!("Done. Waiting 10s...");
    delay.delay_ms(10_000);

    Ok(())
}
