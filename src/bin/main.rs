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
use twine::embassy::UiBuilderExt;
use twine::engine::{EngineConfig, HeapPeak, MemInfo};
use twine::hal::AsyncDisplayDriver;
use twine::prelude::*;

// const CONNECTIONS_MAX: usize = 1;
// const L2CAP_CHANNELS_MAX: usize = 1;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

twine::draw_buffers! {
    /// Two DMA-pipelined partial draw buffers: 40 rows of the 320 px wide (landscape) panel.
    static BUFS: 2 x 40 rows x 320 px @ Rgb565Swapped;
}

/// The UI's ports (see [`ui::Ports`]): written by the ADS-B and clock tasks, read by the UI.
static UPDATES: Channel<adsb::Update, 4> = Channel::new();
static LOCAL_SECS: Latest<Option<u32>> = Latest::new(None);

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
    spawner.spawn(adsb::task(stack, &UPDATES).unwrap());
    spawner.spawn(clock::task(&LOCAL_SECS).unwrap());
    // BOOT button (GPIO9): refresh the aircraft now.
    spawner.spawn(button::task(p.GPIO9).unwrap());

    // let radio_init = ...; BLE: re-enable together with the `esp-radio` "ble"/"coex" features,
    // `bt-hci` and `trouble-host` (see Cargo.toml):
    // let transport = BleConnector::new(peripherals.BT, Default::default()).unwrap();
    // let ble_controller = ExternalController::<_, 1>::new(transport);
    // let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> =
    //     HostResources::new();
    // let _stack = trouble_host::new(ble_controller, &mut resources);

    // The UI, drawing into the two DMA-pipelined `BUFS`.
    let engine = EngineConfig {
        mem_info: Some(mem_info),
        hires_timer: Some(twine::embassy::hires_now),
        ..EngineConfig::default()
    };
    // The runtime belongs to this task: the `!Send` token keeps the UI and every reactive handle
    // here; the other tasks only reach the UI through the ports.
    let rt = Runtime::take().expect("runtime already taken");
    let ports = ui::Ports { updates: &UPDATES, local_secs: &LOCAL_SECS };
    let ui = Ui::builder_async(display)
        .runtime(rt)
        .buffers(BufferMode::partial_double_from(BUFS.take().expect("draw buffers already taken")))
        .config(engine)
        .theme(DefaultTheme::dark())
        .with_embassy_platform()
        .build(move |cx| ui::app(cx, ports));
    twine::embassy::run(ui).await
}

/// Heap statistics for the `twine::perf` log line; warns when the heap is more than 90 % full.
fn mem_info() -> MemInfo {
    static PEAK: HeapPeak = HeapPeak::new();
    let m = PEAK.sample(esp_alloc::HEAP.used(), esp_alloc::HEAP.free());
    if m.used_percent() > 90 {
        log::warn!("heap above 90%: {} of {} bytes", m.used, m.used + m.free);
    }
    m
}
