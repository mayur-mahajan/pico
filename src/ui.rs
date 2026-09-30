//! The screen: a header with the airport and the local time, and below it the closest aircraft
//! as a callsign row plus four stat tiles ("Connecting..." shows until the first response).

use alloc::format;
use alloc::string::String;
use twine::prelude::*;

use crate::adsb::{Aircraft, Altitude, UPDATES, Update};
use crate::clock::LOCAL_SECS;
use crate::config::AIRPORT_NAME;

const BACKGROUND: Color = Color::hex(0x0B_12_20);
const CARD: Color = Color::hex(0x16_23_3A);
const TEXT: Color = Color::hex(0xFF_FF_FF);
const MUTED: Color = Color::hex(0x8F_A3_BF);
const TIME: Color = Color::hex(0x4D_D0_E1);
const CHIP: Color = Color::hex(0x29_62_FF);
const ALTITUDE: Color = Color::hex(0xFF_B7_4D);
const SPEED: Color = Color::hex(0x69_F0_AE);
const HEADING: Color = Color::hex(0xCE_93_D8);
const DISTANCE: Color = Color::hex(0x64_B5_F6);

/// From this altitude (feet) up, altitudes are shown as flight levels: 35000 ft is FL350.
const TRANSITION_FT: i32 = 18_000;

#[derive(Clone, Debug, PartialEq)]
enum Screen {
    /// Waiting for the first response.
    Loading,
    Aircraft(Aircraft),
    NoAircraft,
}

pub fn app(cx: Scope) -> impl View {
    let screen = cx.signal(Screen::Loading);
    let secs = cx.signal(None::<u32>);
    // A failed request keeps the last screen; the first success replaces "Connecting...".
    cx.on_message(&UPDATES, move |u: Update| match u {
        Update::Aircraft(a) => screen.set(Screen::Aircraft(a)),
        Update::NoAircraft => screen.set(Screen::NoAircraft),
        Update::Failed => {}
    });
    cx.on_message(&LOCAL_SECS, move |s: u32| secs.set(Some(s)));

    let loading = move || screen.get() == Screen::Loading;
    let has_aircraft = move || matches!(screen.get(), Screen::Aircraft(_));
    column((
        header(secs),
        when(loading, |_| message(String::from("Connecting..."))).otherwise(move |_| {
            when(has_aircraft, move |_| aircraft_card(screen))
                .otherwise(|_| message(format!("No aircraft near {AIRPORT_NAME}")))
        }),
    ))
    .gap(8)
    .padding(10)
    .bg(BACKGROUND)
    .size(Length::Pct(100), Length::Pct(100))
}

/// The airport on the left, the local time on the right.
fn header(secs: Signal<Option<u32>>) -> impl View {
    row((
        label(text!("Nearest aircraft to {AIRPORT_NAME}"))
            .font(&fonts::MONTSERRAT_14)
            .text_color(MUTED),
        label(text!("{}", clock_text(secs.get())))
            .font(&fonts::MONTSERRAT_20)
            .text_color(TIME)
            .test_id("time"),
    ))
    .justify(FlexAlign::SpaceBetween)
    .align_items(FlexAlign::Center)
    .size(Length::Pct(100), Length::Content)
}

/// A centered status message filling the space under the header.
fn message(text: String) -> impl View {
    column((label(text).font(&fonts::MONTSERRAT_20).text_color(MUTED),))
        .justify(FlexAlign::Center)
        .align_items(FlexAlign::Center)
        .flex_grow(1)
        .size(Length::Pct(100), Length::Content)
}

/// The closest aircraft: callsign, type chip and registration, then the stat tiles.
fn aircraft_card(screen: Signal<Screen>) -> impl View {
    let field = move |f: fn(&Aircraft) -> String| {
        move || match screen.get() {
            Screen::Aircraft(a) => f(&a),
            _ => String::new(),
        }
    };
    column((
        row((
            label(text!("{}", field(callsign)()))
                .font(&fonts::MONTSERRAT_28)
                .text_color(TEXT)
                .test_id("callsign"),
            label(text!("{}", field(|a| a.kind.clone())()))
                .font(&fonts::MONTSERRAT_14)
                .text_color(TEXT)
                .bg(CHIP)
                .radius(6)
                .padding_hor(8)
                .padding_ver(3),
            label(text!("{}", field(|a| a.registration.clone())()))
                .font(&fonts::MONTSERRAT_14)
                .text_color(MUTED),
        ))
        .gap(10)
        .align_items(FlexAlign::Center)
        .size(Length::Pct(100), Length::Content),
        row((
            stat(|| String::from("ALT"), ALTITUDE, field(altitude)),
            stat(|| String::from("SPEED"), SPEED, field(speed)),
            stat(|| String::from("HDG"), HEADING, field(heading)),
            stat(|| String::from("DIST"), DISTANCE, field(distance)),
        ))
        .gap(6)
        .size(Length::Pct(100), Length::Content),
    ))
    .gap(12)
    .flex_grow(1)
    .justify(FlexAlign::Center)
    .size(Length::Pct(100), Length::Content)
}

/// A rounded tile: a small caption over a colored value.
fn stat(caption: impl Fn() -> String + 'static, color: Color, value: impl Fn() -> String + 'static) -> impl View {
    column((
        label(text!("{}", caption())).font(&fonts::MONTSERRAT_14).text_color(MUTED),
        label(text!("{}", value())).font(&fonts::MONTSERRAT_20).text_color(color),
    ))
    .gap(2)
    .padding_ver(8)
    .padding_hor(4)
    .bg(CARD)
    .radius(8)
    .align_items(FlexAlign::Center)
    .flex_grow(1)
}

fn clock_text(secs: Option<u32>) -> String {
    match secs {
        Some(s) => format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60),
        None => String::from("--:--:--"),
    }
}

fn callsign(a: &Aircraft) -> String {
    if a.callsign.is_empty() { String::from("(no callsign)") } else { a.callsign.clone() }
}

fn altitude(a: &Aircraft) -> String {
    match a.altitude {
        Some(Altitude::Ground) => String::from("GND"),
        Some(Altitude::Feet(ft)) if ft >= TRANSITION_FT => format!("FL{:03}", ft / 100),
        Some(Altitude::Feet(ft)) => format!("{ft}"),
        None => String::from("-"),
    }
}

fn speed(a: &Aircraft) -> String {
    a.speed_kt.map_or(String::from("-"), |v| format!("{v:.0}"))
}

fn heading(a: &Aircraft) -> String {
    a.heading.map_or(String::from("-"), |v| format!("{v:.0}°"))
}

fn distance(a: &Aircraft) -> String {
    a.distance_nm.map_or(String::from("-"), |v| format!("{v:.1}"))
}
