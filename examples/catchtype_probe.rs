//! Read-only diagnostic: prints only Catchtype tray labels, never typed text.
#[allow(dead_code)]
#[path = "../src/catchtype.rs"]
mod catchtype;
#[allow(dead_code)]
#[path = "../src/ime.rs"]
mod ime;

use std::time::{Duration, Instant};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

fn main() -> windows::core::Result<()> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
    let result = probe();
    unsafe { CoUninitialize() };
    result
}

fn probe() -> windows::core::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let seconds = args
        .windows(2)
        .find(|pair| pair[0] == "--seconds")
        .and_then(|pair| pair[1].parse::<u64>().ok())
        .unwrap_or(60);
    if std::env::args().any(|arg| arg == "--monitor") {
        let monitor = catchtype::Monitor::start().expect("start status monitor");
        let started = Instant::now();
        let mut previous = None;
        while started.elapsed() < Duration::from_secs(seconds) {
            let caps = unsafe {
                windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(
                    windows::Win32::UI::Input::KeyboardAndMouse::VK_CAPITAL.0 as i32,
                ) & 1
                    != 0
            };
            let current = (monitor.current_status(), ime::get_ime_mode(), caps);
            if previous != Some(current) {
                println!(
                    "t={}ms monitor={:?} ime={:?} caps={}",
                    started.elapsed().as_millis(),
                    current.0,
                    current.1,
                    current.2
                );
                previous = Some(current);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        return Ok(());
    }
    let watch = std::env::args().any(|arg| arg == "--watch");
    let mut reader = catchtype::Reader::new()?;
    let started = Instant::now();
    let mut previous = None;
    loop {
        let labels = reader.labels()?;
        let status = reader.read()?;
        let current = (labels, status);
        if previous.as_ref() != Some(&current) {
            println!(
                "t={}ms status={:?} labels={:?}",
                started.elapsed().as_millis(),
                current.1,
                current.0
            );
            if current.0.is_empty() {
                println!("No accessible Catchtype icon. Show its icon on the taskbar and retry.");
            }
            previous = Some(current);
        }
        if !watch || started.elapsed() >= Duration::from_secs(seconds) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
