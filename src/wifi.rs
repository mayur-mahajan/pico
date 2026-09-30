//! Wi-Fi station + the embassy-net stack (DHCP, DNS, TCP).

use embassy_executor::Spawner;
use embassy_net::{Runner, Stack, StackResources};
use embassy_time::{Duration, Timer};
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{AuthenticationMethodConfig, Config, Interface, WifiController};
use log::{info, warn};
use static_cell::StaticCell;

use crate::config::{WIFI_PASSWORD, WIFI_SSID, WIFI_TX_POWER};

/// Starts Wi-Fi and the network stack; returns the stack (use `stack.wait_config_up()`).
///
/// The scheduler (`esp_rtos::start`) must be running.
pub fn start(spawner: &Spawner, wifi: esp_hal::peripherals::WIFI<'static>, seed: u64) -> Stack<'static> {
    let mut controller = WifiController::new(wifi, Default::default()).expect("Wi-Fi init failed");
    controller
        .set_config(&Config::Station(
            StationConfig::default()
                .with_ssid(WIFI_SSID.try_into().expect("SSID too long"))
                .with_authentication(AuthenticationMethodConfig::Wpa2Personal(
                    WIFI_PASSWORD.try_into().expect("password too long"),
                )),
        ))
        .expect("Wi-Fi config failed");
    if let Err(e) = controller.set_max_tx_power(WIFI_TX_POWER) {
        warn!("wifi: could not set tx power: {e:?}");
    }

    static RESOURCES: StaticCell<StackResources<5>> = StaticCell::new();
    let (stack, runner) = embassy_net::new(
        Interface::station(),
        embassy_net::Config::dhcpv4(Default::default()),
        RESOURCES.init(StackResources::new()),
        seed,
    );
    spawner.spawn(connection_task(controller).unwrap());
    spawner.spawn(net_task(runner).unwrap());
    stack
}

/// Keeps the station associated with the access point.
#[embassy_executor::task]
async fn connection_task(mut controller: WifiController<'static>) {
    loop {
        info!("wifi: connecting to {WIFI_SSID}");
        match controller.connect_async().await {
            Ok(_) => {
                info!("wifi: connected");
                let reason = controller.wait_for_disconnect_async().await;
                warn!("wifi: disconnected: {reason:?}");
            }
            Err(e) => warn!("wifi: connect failed: {e:?}"),
        }
        Timer::after(Duration::from_secs(5)).await;
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface>) {
    runner.run().await
}
