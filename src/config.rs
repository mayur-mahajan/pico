//! Compile-time settings.
//!
//! Wi-Fi credentials come from the build environment so they stay out of the source:
//! `WIFI_SSID=... WIFI_PASSWORD=... cargo espflash flash --release --monitor`.

pub const WIFI_SSID: &str = env!("WIFI_SSID", "set WIFI_SSID when building");
pub const WIFI_PASSWORD: &str = env!("WIFI_PASSWORD", "set WIFI_PASSWORD when building");

/// San Francisco International Airport.
/// //37.51986334712008, -122.29881206181209
pub const AIRPORT_NAME: &str = "home";
pub const AIRPORT_LAT: &str = "37.5198";
pub const AIRPORT_LON: &str = "-122.2988";
/// Search radius in nautical miles.
pub const RADIUS_NM: u32 = 25;

/// Wi-Fi maximum transmit power in units of 0.25 dBm (range 8..=84; the driver default of 20 is
/// only 5 dBm). Values above ~65 (16 dBm) have caused authentication failures on some boards.
pub const WIFI_TX_POWER: i8 = 60;

/// Sent with every request: adsb.lol rejects generic or missing User-Agents (403). Add contact
/// details here if you poll heavily.
pub const USER_AGENT: &str = "pico-adsb-display/0.1";

/// Seconds between two `/v2/closest` requests.
pub const POLL_SECS: u64 = 60;

/// Heap sizes: the reclaimed bootloader RAM plus ordinary RAM (Wi-Fi is hungry).
pub const HEAP_RECLAIMED_BYTES: usize = 64 * 1024;
pub const HEAP_BYTES: usize = 120 * 1024;
