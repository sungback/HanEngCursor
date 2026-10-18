//! Read-only diagnostic: prints only Catchtype tray labels, never typed text.
#[allow(dead_code)]
#[path = "../src/catchtype.rs"]
mod catchtype;

use std::time::{Duration, Instant};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

fn main() -> windows::core::Result<()> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
    let result = probe();
    unsafe { CoUninitialize() };
    result
}

fn probe() -> windows::core::Result<()> {
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
        if !watch || started.elapsed() >= Duration::from_secs(60) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
