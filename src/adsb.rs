//! Client for <https://api.adsb.lol> `/v2/closest`: the aircraft nearest the airport.

use alloc::format;
use alloc::string::{String, ToString};
use embassy_net::Stack;
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, with_timeout};
use log::{info, warn};
use reqwless::client::HttpClient;
use reqwless::request::{Method, RequestBuilder};
use static_cell::ConstStaticCell;
use twine::prelude::Channel;

use crate::clock;
use crate::config::{AIRPORT_LAT, AIRPORT_LON, POLL_SECS, RADIUS_NM, USER_AGENT};

/// Altitude as reported by the aircraft.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Altitude {
    Ground,
    Feet(i32),
}

/// The closest aircraft.
#[derive(Clone, Debug, PartialEq)]
pub struct Aircraft {
    pub callsign: String,
    pub registration: String,
    pub kind: String,
    pub altitude: Option<Altitude>,
    pub speed_kt: Option<f32>,
    pub heading: Option<f32>,
    /// Distance from the airport in nautical miles.
    pub distance_nm: Option<f32>,
}

/// What the poller tells the UI.
#[derive(Clone, Debug, PartialEq)]
pub enum Update {
    Aircraft(Aircraft),
    /// Nothing within the search radius.
    NoAircraft,
    /// The request failed (the UI keeps what it shows).
    Failed,
}

/// Signal it to fetch right away instead of waiting for the next poll.
pub static REFRESH: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// The response is ~0.7 kB per aircraft; leave room for headers and odd fields.
const RX_BUF: usize = 4096;

/// Polls `/v2/closest` every [`POLL_SECS`] (or when [`REFRESH`] is signalled) and sends each
/// result to the UI through `updates`.
#[embassy_executor::task]
pub async fn task(stack: Stack<'static>, updates: &'static Channel<Update, 4>) {
    static RX: ConstStaticCell<[u8; RX_BUF]> = ConstStaticCell::new([0; RX_BUF]);
    let rx = RX.take();

    stack.wait_config_up().await;
    info!("adsb: network is up");

    // Plain HTTP: the data is public and read-only (see `Cargo.toml` for why not HTTPS).
    let url = format!("http://api.adsb.lol/v2/closest/{AIRPORT_LAT}/{AIRPORT_LON}/{RADIUS_NM}");
    let tcp_state = TcpClientState::<1, 2048, 2048>::new();
    let tcp = TcpClient::new(stack, &tcp_state);
    let dns = DnsSocket::new(stack);

    loop {
        let update = match fetch(&tcp, &dns, &url, rx).await {
            Ok(u) => u,
            Err(e) => {
                warn!("adsb: request failed: {e}");
                Update::Failed
            }
        };
        updates.try_send(update).ok();
        // Wait for the next poll, or until the button asks for a refresh.
        with_timeout(Duration::from_secs(POLL_SECS), REFRESH.wait()).await.ok();
        REFRESH.reset();
    }
}

async fn fetch(
    tcp: &TcpClient<'_, 1, 2048, 2048>,
    dns: &DnsSocket<'_>,
    url: &str,
    rx: &mut [u8],
) -> Result<Update, String> {
    let mut client = HttpClient::new(tcp, dns);
    let mut request = client
        .request(Method::GET, url)
        .await
        .map_err(|e| format!("connect: {e:?}"))?
        .headers(&[("User-Agent", USER_AGENT)]);
    let response = request
        .send(rx)
        .await
        .map_err(|e| format!("send: {e:?}"))?;
    let status = response.status;
    let body = response
        .body()
        .read_to_end()
        .await
        .map_err(|e| format!("body: {e:?}"))?;
    let body = core::str::from_utf8(body).map_err(|_| "body is not UTF-8".to_string())?;
    if !status.is_successful() {
        return Err(format!("HTTP {}: {}", status.0, head(body)));
    }
    parse(body).ok_or_else(|| format!("unexpected response: {}", head(body)))
}

/// The start of a body, for error messages.
fn head(body: &str) -> &str {
    let end = (0..=body.len().min(120)).rev().find(|&i| body.is_char_boundary(i)).unwrap_or(0);
    &body[..end]
}

/// Reads the first aircraft out of a `/v2/closest` response and syncs the clock from `now`.
fn parse(body: &str) -> Option<Update> {
    if let Some(now) = number(body, "now") {
        clock::sync(now as u64);
    }
    let ac_start = body.find("\"ac\":[")? + "\"ac\":[".len();
    let ac = body[ac_start..].trim_start();
    if ac.starts_with(']') {
        return Some(Update::NoAircraft);
    }
    // Only the first aircraft object.
    let ac = &ac[..ac.find("\n").unwrap_or(ac.len())];
    let altitude = match text(ac, "alt_baro") {
        Some("ground") => Some(Altitude::Ground),
        _ => number(ac, "alt_baro").map(|v| Altitude::Feet(v as i32)),
    };
    Some(Update::Aircraft(Aircraft {
        callsign: text(ac, "flight").unwrap_or("").trim().to_string(),
        registration: text(ac, "r").unwrap_or("").trim().to_string(),
        kind: text(ac, "t").unwrap_or("").trim().to_string(),
        altitude,
        speed_kt: number(ac, "gs").map(|v| v as f32),
        heading: number(ac, "track").map(|v| v as f32),
        distance_nm: number(ac, "dst").map(|v| v as f32),
    }))
}

/// The raw value after `"key":` up to the next `,`, `}` or line end.
fn raw<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("\"{key}\":");
    let start = s.find(&pat)? + pat.len();
    let rest = s[start..].trim_start();
    let end = rest.find([',', '}', '\n']).unwrap_or(rest.len());
    Some(rest[..end].trim())
}

/// A JSON string value, without the quotes.
fn text<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    raw(s, key)?.strip_prefix('"')?.strip_suffix('"')
}

/// A JSON number value.
fn number(s: &str, key: &str) -> Option<f64> {
    raw(s, key)?.parse().ok()
}
