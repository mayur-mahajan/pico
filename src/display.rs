//! The 1.47" 172 x 320 ST7789 panel on SPI2 + DMA, and its PWM backlight.

use alloc::boxed::Box;
use embassy_embedded_hal::shared_bus::asynch::spi::SpiDeviceWithConfig;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use esp_hal::gpio::{DriveMode, Level, Output, OutputConfig};
use esp_hal::ledc::channel::ChannelIFace;
use esp_hal::ledc::timer::TimerIFace;
use esp_hal::ledc::{LSGlobalClkSource, Ledc, LowSpeed, channel, timer};
use esp_hal::peripherals::{DMA_CH0, GPIO6, GPIO7, GPIO14, GPIO15, GPIO21, GPIO22, LEDC, SPI2};
use esp_hal::spi::master::{Config as SpiConfig, Spi, SpiDma};
use esp_hal::time::Rate;
use static_cell::StaticCell;
use twine::core::Rotation;
use twine::hal::AsyncDisplayDriver;

/// The shared SPI bus.
type Bus = Mutex<NoopRawMutex, SpiDma<'static, esp_hal::Async>>;

const LCD_MHZ: u32 = 40;
/// Landscape; use `Deg270` if it is upside down.
const ROTATION: Rotation = Rotation::Deg90;

/// An ST7789 whose visible 172 x 320 window sits at column 34 of the controller's
/// 240 x 320 memory. Other panels on the same driver only need another size/offset.
static PANEL: twine_drivers::mipi_dcs::PanelSpec = twine_drivers::st7789::ST7789.with_size(172, 320, 34, 0);

/// The display pins and peripherals: SCK = GPIO7, MOSI = GPIO6, CS = GPIO14, DC = GPIO15,
/// RST = GPIO21.
pub struct Pins {
    pub spi: SPI2<'static>,
    pub dma: DMA_CH0<'static>,
    pub sck: GPIO7<'static>,
    pub mosi: GPIO6<'static>,
    pub cs: GPIO14<'static>,
    pub dc: GPIO15<'static>,
    pub rst: GPIO21<'static>,
}

/// Turns the backlight (GPIO22) on at `duty_pct` percent.
pub fn backlight(ledc: LEDC<'static>, pin: GPIO22<'static>, duty_pct: u8) {
    let pin = Output::new(pin, Level::Low, OutputConfig::default());
    // The PWM hardware keeps running; leak the drivers so they are never dropped.
    let ledc: &'static mut Ledc = Box::leak(Box::new(Ledc::new(ledc)));
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);
    let ledc: &'static Ledc = ledc;
    let lstimer0: &'static mut timer::Timer<'static, LowSpeed> =
        Box::leak(Box::new(ledc.timer::<LowSpeed>(timer::Number::Timer0)));
    lstimer0
        .configure(timer::config::Config {
            duty: timer::config::Duty::Duty5Bit,
            clock_source: timer::LSClockSource::APBClk,
            frequency: Rate::from_khz(24),
        })
        .expect("backlight timer init failure");
    let lstimer0: &'static timer::Timer<'static, LowSpeed> = lstimer0;
    let mut channel0 = ledc.channel(channel::Number::Channel0, pin);
    channel0
        .configure(channel::config::Config {
            timer: lstimer0,
            duty_pct,
            drive_mode: DriveMode::PushPull,
        })
        .expect("backlight channel init failure");
    Box::leak(Box::new(channel0));
}

/// Initializes the panel.
pub async fn init(pins: Pins) -> impl AsyncDisplayDriver {
    let lcd_config = SpiConfig::default().with_frequency(Rate::from_mhz(LCD_MHZ));

    // The bus: SPI2 + DMA (the display writes its buffers in place, without a copy).
    static BUS: StaticCell<Bus> = StaticCell::new();
    let spi = Spi::new(pins.spi, lcd_config)
        .unwrap()
        .with_sck(pins.sck)
        .with_mosi(pins.mosi)
        .with_dma(pins.dma)
        .into_async();
    let bus: &'static Bus = BUS.init(Mutex::new(spi));

    let spi = SpiDeviceWithConfig::new(bus, Output::new(pins.cs, Level::High, OutputConfig::default()), lcd_config);
    let dc = Output::new(pins.dc, Level::Low, OutputConfig::default());
    let rst = Output::new(pins.rst, Level::High, OutputConfig::default());
    twine_drivers::st7789::new_async(spi, dc, Some(rst), &PANEL, ROTATION, &mut embassy_time::Delay)
        .await
        .unwrap_or_else(|e| panic!("display init failed: {e:?}"))
}
