#![no_std]
#![no_main]

mod panic;

#[path = "../drivers/rtc_rust/rtc.rs"]
mod rtc_rust;
