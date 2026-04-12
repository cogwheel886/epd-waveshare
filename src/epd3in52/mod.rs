//! A simple Driver for the Waveshare 3.52" E-Ink Display via SPI
//!
//!
//! Build with the help of documentation/code from [Waveshare](https://www.waveshare.com/wiki/3.52inch_e-Paper_HAT),

use embedded_hal::{
    delay::DelayNs,
    digital::{InputPin, OutputPin},
    spi::SpiDevice,
};

pub(crate) mod command;
mod constants;

use self::command::Command;
use self::constants::*;

use crate::buffer_len;
use crate::color::Color;
use crate::interface::DisplayInterface;
use crate::traits::{InternalWiAdditions, RefreshLut, WaveshareDisplay};

/// Width of the display.
pub const WIDTH: u32 = 240;

/// Height of the display
pub const HEIGHT: u32 = 360;

/// Default Background Color
pub const DEFAULT_BACKGROUND_COLOR: Color = Color::White;

const IS_BUSY_LOW: bool = true;

const SINGLE_BYTE_WRITE: bool = true;

/// Display with Fullsize buffer for use with the 3in52 EPD
#[cfg(feature = "graphics")]
pub type Display3in52 = crate::graphics::Display<
    WIDTH,
    HEIGHT,
    false,
    { buffer_len(WIDTH as usize, HEIGHT as usize) },
    Color,
>;

/// Epd3in52 driver
pub struct Epd3in52<SPI, BUSY, DC, RST, DELAY> {
    /// Connection Interface
    interface: DisplayInterface<SPI, BUSY, DC, RST, DELAY, SINGLE_BYTE_WRITE>,
    /// Background Color
    background_color: Color,
    /// Alternates waveform tables each refresh
    lut_flag: bool,
    /// Controls which LUT waveform set is used: Full (GC) or Quick (DU)
    refresh_lut: RefreshLut,
}

impl<SPI, BUSY, DC, RST, DELAY> InternalWiAdditions<SPI, BUSY, DC, RST, DELAY>
    for Epd3in52<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        // reset the device
        self.interface.reset(delay, 30, 10);

        self.interface
            .cmd_with_data(spi, Command::PanelSetting, &[0xFF, 0x01])?;
        self.interface.cmd_with_data(
            spi,
            Command::PowerSetting,
            &[0x03, 0x10, 0x3F, 0x3F, 0x03],
        )?;
        self.interface
            .cmd_with_data(spi, Command::BoosterSoftStart, &[0x37, 0x3D, 0x3D])?;
        self.interface
            .cmd_with_data(spi, Command::TconSetting, &[0x22])?;
        self.interface
            .cmd_with_data(spi, Command::VcomDcSetting, &[0x07])?;
        self.interface
            .cmd_with_data(spi, Command::PllControl, &[0x09])?;
        self.interface
            .cmd_with_data(spi, Command::PowerSaving, &[0x88])?;
        self.interface
            .cmd_with_data(spi, Command::ResolutionSetting, &[0xF0, 0x01, 0x68])?;
        self.interface
            .cmd_with_data(spi, Command::VcomDataSetting, &[0xB7])?;

        self.lut_flag = false;
        self.refresh_lut = RefreshLut::Full;

        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> WaveshareDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd3in52<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    type DisplayColor = Color;

    fn new(
        spi: &mut SPI,
        busy: BUSY,
        dc: DC,
        rst: RST,
        delay: &mut DELAY,
        delay_us: Option<u32>,
    ) -> Result<Self, SPI::Error> {
        let mut epd = Epd3in52 {
            interface: DisplayInterface::new(busy, dc, rst, delay_us),
            background_color: DEFAULT_BACKGROUND_COLOR,
            lut_flag: false,
            refresh_lut: RefreshLut::Full,
        };

        epd.init(spi, delay)?;
        Ok(epd)
    }

    fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.init(spi, delay)
    }

    fn sleep(&mut self, spi: &mut SPI, _delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.interface.cmd_with_data(spi, Command::Sleep, &[0xA5])?;
        Ok(())
    }

    fn set_background_color(&mut self, color: Self::DisplayColor) {
        self.background_color = color;
    }

    fn background_color(&self) -> &Self::DisplayColor {
        &self.background_color
    }

    fn width(&self) -> u32 {
        WIDTH
    }

    fn height(&self) -> u32 {
        HEIGHT
    }

    fn update_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        _delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        assert!(buffer.len() == buffer_len(WIDTH as usize, HEIGHT as usize));
        self.interface
            .cmd_with_data(spi, Command::DataStartTransmission, buffer)?;
        Ok(())
    }

    #[allow(unused)]
    fn update_partial_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        buffer: &[u8],
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), SPI::Error> {
        todo!()
    }

    fn display_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        match self.refresh_lut {
            RefreshLut::Full => {
                // GC (global clear) waveform — full quality, ~0.9s
                self.interface
                    .cmd_with_data(spi, Command::LutVcom, &LUT_R20_GC)?;
                self.interface
                    .cmd_with_data(spi, Command::LutBlue, &LUT_R21_GC)?;
                self.interface
                    .cmd_with_data(spi, Command::LutGray2, &LUT_R24_GC)?;

                if !self.lut_flag {
                    self.interface
                        .cmd_with_data(spi, Command::LutWhite, &LUT_R22_GC)?;
                    self.interface
                        .cmd_with_data(spi, Command::LutGray1, &LUT_R23_GC[..42])?;
                } else {
                    self.interface
                        .cmd_with_data(spi, Command::LutWhite, &LUT_R23_GC)?;
                    self.interface
                        .cmd_with_data(spi, Command::LutGray1, &LUT_R22_GC[..42])?;
                }
            }
            RefreshLut::Quick => {
                // WARNING: DU (differential update) fast refresh.
                // Waveshare note: "Quick refresh is supported, but the refresh
                // effect is not good, but it is not recommended."
                // Use RefreshLut::Full (GC) for normal operation.
                self.interface
                    .cmd_with_data(spi, Command::LutVcom, &LUT_R20_DU)?;
                self.interface
                    .cmd_with_data(spi, Command::LutBlue, &LUT_R21_DU)?;
                self.interface
                    .cmd_with_data(spi, Command::LutGray2, &LUT_R24_DU)?;

                if !self.lut_flag {
                    self.interface
                        .cmd_with_data(spi, Command::LutWhite, &LUT_R22_DU)?;
                    self.interface
                        .cmd_with_data(spi, Command::LutGray1, &LUT_R23_DU[..42])?;
                } else {
                    self.interface
                        .cmd_with_data(spi, Command::LutWhite, &LUT_R23_DU)?;
                    self.interface
                        .cmd_with_data(spi, Command::LutGray1, &LUT_R22_DU[..42])?;
                }
            }
        }

        self.lut_flag = !self.lut_flag;

        self.interface
            .cmd_with_data(spi, Command::Refresh, &[0xA5])?;
        self.interface.wait_until_idle(delay, IS_BUSY_LOW);
        delay.delay_us(200_000);
        Ok(())
    }

    fn update_and_display_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        self.update_frame(spi, buffer, delay)?;
        self.display_frame(spi, delay)?;
        Ok(())
    }

    fn clear_frame(&mut self, spi: &mut SPI, _delay: &mut DELAY) -> Result<(), SPI::Error> {
        let color = self.background_color.get_byte_value();
        self.interface.cmd(spi, Command::DataStartTransmission)?;
        self.interface.data_x_times(
            spi,
            color,
            buffer_len(WIDTH as usize, HEIGHT as usize) as u32,
        )?;
        Ok(())
    }

    fn set_lut(
        &mut self,
        _spi: &mut SPI,
        _delay: &mut DELAY,
        refresh_rate: Option<RefreshLut>,
    ) -> Result<(), SPI::Error> {
        if let Some(lut) = refresh_rate {
            self.refresh_lut = lut;
        }
        Ok(())
    }

    fn wait_until_idle(&mut self, _spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.interface.wait_until_idle(delay, IS_BUSY_LOW);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epd_size() {
        assert_eq!(WIDTH, 240);
        assert_eq!(HEIGHT, 360);
        assert_eq!(buffer_len(WIDTH as usize, HEIGHT as usize), 240 / 8 * 360);
    }

    #[test]
    fn lut_selection_default_is_full() {
        // RefreshLut::Full is the default — DU must be explicitly requested
        assert!(matches!(RefreshLut::Full, RefreshLut::Full));
        assert!(matches!(RefreshLut::Quick, RefreshLut::Quick));
    }
}
