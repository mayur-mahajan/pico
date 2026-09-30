#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

// use bt_hci::controller::ExternalController;
// use esp_radio::ble::controller::BleConnector;
// use trouble_host::prelude::*;
use esp_backtrace as _;
use esp_hal::{clock::CpuClock, rng::Rng, timer::timg::TimerGroup};
use log::info;
use pico::{adsb, button, clock, config, display, ui, wifi};
use static_cell::ConstStaticCell;
use twine::engine::{EngineConfig, MemInfo};
use twine::hal::AsyncDisplayDriver;
use twine::prelude::*;
use twine_embassy::UiBuilderExt;

// const CONNECTIONS_MAX: usize = 1;
// const L2CAP_CHANNELS_MAX: usize = 1;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

/// Bytes of one partial draw buffer: 40 rows of 320 px (RGB565).
const BUFFER_BYTES: usize = 320 * 40 * 2;

/// A 4-byte aligned draw buffer (the engine needs word-aligned buffers).
#[repr(C, align(4))]
struct DrawBuffer([u8; BUFFER_BYTES]);

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: embassy_executor::Spawner) -> ! {
    esp_println::logger::init_logger_from_env();

    let p = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: config::HEAP_RECLAIMED_BYTES);
    esp_alloc::heap_allocator!(size: config::HEAP_BYTES);

    let timg0 = TimerGroup::new(p.TIMG0);
    esp_rtos::start(timg0.timer0, p.FROM_CPU_INTR0);
    info!("Embassy initialized!");

    display::backlight(p.LEDC, p.GPIO22, 20);
    let display = display::init(display::Pins {
        spi: p.SPI2,
        dma: p.DMA_CH0,
        sck: p.GPIO7,
        mosi: p.GPIO6,
        cs: p.GPIO14,
        dc: p.GPIO15,
        rst: p.GPIO21,
    })
    .await;
    let info = display.info();
    info!("Display initialized: {}x{}", info.width, info.height);

    // Wi-Fi + network, then the tasks that feed the UI through channels.
    let rng = Rng::new();
    let seed = (u64::from(rng.random()) << 32) | u64::from(rng.random());
    let stack = wifi::start(&spawner, p.WIFI, seed);
    spawner.spawn(adsb::task(stack).unwrap());
    spawner.spawn(clock::task().unwrap());
    // BOOT button (GPIO9): refresh the aircraft now.
    spawner.spawn(button::task(p.GPIO9).unwrap());

    // let radio_init = ...; BLE: re-enable together with the `esp-radio` "ble"/"coex" features,
    // `bt-hci` and `trouble-host` (see Cargo.toml):
    // let transport = BleConnector::new(peripherals.BT, Default::default()).unwrap();
    // let ble_controller = ExternalController::<_, 1>::new(transport);
    // let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> =
    //     HostResources::new();
    // let _stack = trouble_host::new(ble_controller, &mut resources);

    // The UI: two DMA-pipelined partial buffers of 40 rows.
    static BUF_A: ConstStaticCell<DrawBuffer> = ConstStaticCell::new(DrawBuffer([0; BUFFER_BYTES]));
    static BUF_B: ConstStaticCell<DrawBuffer> = ConstStaticCell::new(DrawBuffer([0; BUFFER_BYTES]));
    let engine = EngineConfig {
        mem_info: Some(mem_info),
        hires_timer: Some(twine_embassy::hires_now),
        ..EngineConfig::default()
    };
    // SAFETY: the UI and every reactive handle live in this task on the executor and are never
    // touched from an interrupt handler or another executor; other contexts only use channels
    // and the UI waker.
    let builder = unsafe { Ui::builder_async(display).bind_to_current_context() };
    let ui = builder
        .buffers(BufferMode::partial_double(&mut BUF_A.take().0, &mut BUF_B.take().0))
        .config(engine)
        .theme(DefaultTheme::dark())
        .with_embassy_clock()
        .build(ui::app);
    twine_embassy::run(ui).await
}

/// Heap statistics for the `twine::perf` log line; warns when the heap is more than 90 % full.
fn mem_info() -> MemInfo {
    use core::sync::atomic::{AtomicU32, Ordering};
    static PEAK: AtomicU32 = AtomicU32::new(0);
    let used = esp_alloc::HEAP.used() as u32;
    let free = esp_alloc::HEAP.free() as u32;
    let peak = PEAK.load(Ordering::Relaxed).max(used);
    PEAK.store(peak, Ordering::Relaxed);
    if u64::from(used) * 10 > u64::from(used + free) * 9 {
        log::warn!("heap above 90%: {used} of {} bytes", used + free);
    }
    MemInfo { used, peak, free }
}
