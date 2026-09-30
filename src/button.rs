//! The BOOT button: pressing it asks the ADS-B task for an immediate refresh.

use embassy_time::{Duration, Timer};
use esp_hal::gpio::{Input, InputConfig, Pull};
use esp_hal::peripherals::GPIO9;
use log::info;

use crate::adsb::REFRESH;

const DEBOUNCE: Duration = Duration::from_millis(50);

/// The button pulls its pin to ground when pressed.
#[embassy_executor::task]
pub async fn task(pin: GPIO9<'static>) {
    let mut button = Input::new(pin, InputConfig::default().with_pull(Pull::Up));
    loop {
        button.wait_for_falling_edge().await;
        Timer::after(DEBOUNCE).await;
        if button.is_low() {
            info!("button: refresh requested");
            REFRESH.signal(());
            button.wait_for_high().await;
        }
    }
}
