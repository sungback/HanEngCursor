//! Read Catchtype's tray accessibility label without intercepting any keystrokes.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::core::w;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationCondition, IUIAutomationElement,
    TreeScope_Descendants,
};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, GetForegroundWindow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    English,
    Korean,
    Inactive,
    Unknown,
}

pub fn parse_label(label: &str) -> Option<Status> {
    // Windows 11 can prepend the icon's original tooltip to its current tooltip:
    // "Catchtype: 영어 Catchtype: 한글". Only the last complete label is current.
    if !label.starts_with("Catchtype:") {
        return None;
    }
    let (_, current) = label.rsplit_once("Catchtype:")?;
    Some(match current.trim() {
        "영어" => Status::English,
        "한글" => Status::Korean,
        "쉬는 중" | "비밀번호 입력 중" => Status::Inactive,
        _ => Status::Unknown,
    })
}

/// Construct and use on a COM MTA worker, never on the overlay's message thread.
pub struct Reader {
    automation: IUIAutomation,
    condition: IUIAutomationCondition,
    icons: Vec<IUIAutomationElement>,
    discovered: Instant,
}

impl Reader {
    pub fn new() -> windows::core::Result<Self> {
        unsafe {
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
            let condition = automation.CreateTrueCondition()?;
            Ok(Self {
                automation,
                condition,
                icons: Vec::new(),
                discovered: Instant::now() - Duration::from_secs(10),
            })
        }
    }

    fn discover(&mut self) -> windows::core::Result<()> {
        self.icons.clear();
        unsafe {
            // Limit the search to shell notification containers; never inspect
            // editor contents or traverse every application on the desktop.
            for class in [
                w!("Shell_TrayWnd"),
                w!("Shell_SecondaryTrayWnd"),
                w!("NotifyIconOverflowWindow"),
                w!("TopLevelWindowForOverflowXamlIsland"),
            ] {
                let mut after = None;
                while let Ok(hwnd) = FindWindowExW(None, after, class, None) {
                    after = Some(hwnd);
                    let root = self.automation.ElementFromHandle(hwnd)?;
                    let elements = root.FindAll(TreeScope_Descendants, &self.condition)?;
                    for index in 0..elements.Length()? {
                        let element = elements.GetElement(index)?;
                        if parse_label(&element.CurrentName()?.to_string()).is_some() {
                            self.icons.push(element);
                        }
                    }
                }
            }
        }
        self.discovered = Instant::now();
        Ok(())
    }

    pub fn read(&mut self) -> windows::core::Result<Option<Status>> {
        let mut status = None;
        for label in self.labels()? {
            if let Some(found) = parse_label(&label) {
                status = Some(match status {
                    None => found,
                    Some(previous) if previous == found => found,
                    _ => Status::Unknown,
                });
            }
        }
        Ok(status)
    }

    pub fn labels(&mut self) -> windows::core::Result<Vec<String>> {
        let interval = if self.icons.is_empty() {
            Duration::from_millis(500)
        } else {
            Duration::from_secs(1)
        };
        if self.discovered.elapsed() >= interval {
            self.discover()?;
        }
        let mut labels = Vec::new();
        for icon in &self.icons {
            let label = unsafe { icon.CurrentName() };
            let label = match label {
                Ok(label) => label.to_string(),
                Err(error) => {
                    self.discovered = Instant::now() - Duration::from_secs(10);
                    return Err(error);
                }
            };
            labels.push(label);
        }
        Ok(labels)
    }
}

struct Sample {
    status: Option<Status>,
    foreground: usize,
    updated: Instant,
}

static SAMPLE: Mutex<Option<Sample>> = Mutex::new(None);

pub struct Monitor {
    running: Arc<AtomicBool>,
}

impl Monitor {
    pub fn start() -> std::io::Result<Self> {
        let running = Arc::new(AtomicBool::new(true));
        let worker_running = Arc::clone(&running);
        std::thread::Builder::new()
            .name("catchtype-status".into())
            .spawn(move || unsafe {
                if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
                    return;
                }
                // Drop every COM object before uninitializing this apartment.
                if let Ok(mut reader) = Reader::new() {
                    while worker_running.load(Ordering::Relaxed) {
                        let started = Instant::now();
                        let foreground = GetForegroundWindow().0 as usize;
                        let status = reader.read().ok().flatten();
                        if let Ok(mut sample) = SAMPLE.lock() {
                            *sample = Some(Sample {
                                status,
                                foreground,
                                // Timestamp the start, so a slow provider cannot
                                // make old data appear freshly sampled.
                                updated: started,
                            });
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
                CoUninitialize();
            })?;
        Ok(Self { running })
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        // Do not join: a stalled accessibility provider must never block exit.
        self.running.store(false, Ordering::Relaxed);
    }
}

pub fn current_status() -> Option<Status> {
    let sample = SAMPLE.try_lock().ok()?;
    let sample = sample.as_ref()?;
    let foreground = unsafe { GetForegroundWindow() }.0 as usize;
    if foreground == 0
        || foreground != sample.foreground
        || sample.updated.elapsed() > Duration::from_millis(750)
    {
        return None;
    }
    sample.status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_combined_tooltip_uses_the_last_state() {
        assert_eq!(
            parse_label("Catchtype: 영어 Catchtype: 한글"),
            Some(Status::Korean)
        );
        assert_eq!(
            parse_label("Catchtype: 한글 Catchtype: 영어"),
            Some(Status::English)
        );
        assert_eq!(
            parse_label("Catchtype: 한글 Catchtype: 쉬는 중"),
            Some(Status::Inactive)
        );
    }

    #[test]
    fn inactive_and_unrecognized_labels_do_not_force_a_language() {
        assert_eq!(
            parse_label("Catchtype: 비밀번호 입력 중"),
            Some(Status::Inactive)
        );
        assert_eq!(parse_label("Catchtype: unknown"), Some(Status::Unknown));
        assert_eq!(parse_label("Another app: 한글"), None);
        assert_eq!(parse_label("Catchtype: 영어?"), Some(Status::Unknown));
    }
}
