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

const APP_NAME: &str = "Catchtype";

// Checked before the language words: "보안 입력 때문에 쉬는 중" must not count
// as a language just because a later wording mentions one.
const INACTIVE_WORDS: &[&str] = &[
    "꺼",
    "쉬",
    "비밀번호",
    "보안 입력",
    "권한",
    " off",
    "paused",
    "password",
    "secure",
    "permission",
];
const KOREAN_WORDS: &[&str] = &["(가)", "한글", "Korean"];
const ENGLISH_WORDS: &[&str] = &["(A)", "영어", "English"];

/// Only identifies Catchtype's own icon; the wording after the name is not
/// required, so a reworded tooltip is still found and can be diagnosed.
pub fn is_catchtype_label(label: &str) -> bool {
    label.starts_with(APP_NAME)
}

pub fn parse_label(label: &str) -> Option<Status> {
    // Windows 11 can prepend the icon's original tooltip to its current tooltip:
    // "Catchtype 켜짐 · 영어 (A) Catchtype 켜짐 · 한글 (가)". Only the last
    // label is current. Older builds used "Catchtype: 한글".
    if !is_catchtype_label(label) {
        return None;
    }
    let (_, current) = label.rsplit_once(APP_NAME)?;
    let has = |words: &[&str]| words.iter().any(|word| current.contains(word));
    Some(if has(INACTIVE_WORDS) {
        Status::Inactive
    } else if has(KOREAN_WORDS) {
        Status::Korean
    } else if has(ENGLISH_WORDS) {
        Status::English
    } else {
        // Includes "Catchtype 켜짐" from the logo icon style, which hides 한/영.
        Status::Unknown
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
                        if is_catchtype_label(&element.CurrentName()?.to_string()) {
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

pub struct Monitor {
    running: Arc<AtomicBool>,
    sample: Arc<Mutex<Option<Sample>>>,
}

impl Monitor {
    pub fn start() -> std::io::Result<Self> {
        let running = Arc::new(AtomicBool::new(true));
        let worker_running = Arc::clone(&running);
        let sample = Arc::new(Mutex::new(None));
        let worker_sample = Arc::clone(&sample);
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
                        if let Ok(mut sample) = worker_sample.lock() {
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
        Ok(Self { running, sample })
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        // Do not join: a stalled accessibility provider must never block exit.
        self.running.store(false, Ordering::Relaxed);
    }
}

impl Monitor {
    pub fn current_status(&self) -> Option<Status> {
        let sample = self.sample.try_lock().ok()?;
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
    fn current_tooltip_wording_is_recognized() {
        for (label, expected) in [
            ("Catchtype 켜짐 · 한글 (가)", Status::Korean),
            ("Catchtype 켜짐 · 영어 (A)", Status::English),
            (
                "Catchtype 켜짐 · 영어 (A) Catchtype 켜짐 · 한글 (가)",
                Status::Korean,
            ),
            (
                "Catchtype 켜짐 · 한글 (가) Catchtype 켜짐 · 영어 (A)",
                Status::English,
            ),
            ("Catchtype is on · Korean (가)", Status::Korean),
            ("Catchtype is on · English (A)", Status::English),
            (
                "Catchtype 켜짐 · 한글 (가) Catchtype 꺼짐",
                Status::Inactive,
            ),
            (
                "Catchtype is off · Turn it on from the menu",
                Status::Inactive,
            ),
            ("Catchtype is paused", Status::Inactive),
            ("Catchtype이 쉬는 중이에요", Status::Inactive),
            (
                "Catchtype 켜짐 · 보안 입력 때문에 쉬는 중",
                Status::Inactive,
            ),
            ("Catchtype: 꺼짐", Status::Inactive),
            // Observed with 0.3.0-beta.9.1.
            (
                "Catchtype 켜짐 · 영어 (A) Catchtype 꺼 둠 · 메뉴나 Ctrl+Alt+W로 켜요",
                Status::Inactive,
            ),
            // The resting tooltip has no app name, so it trails the original one.
            (
                "Catchtype 켜짐 · 영어 (A) 글 칸이 아닌 곳(그림판·게임 등)이라 쉬고 있어요",
                Status::Inactive,
            ),
            ("Catchtype 켜짐 · 영어 (A) Catchtype: 영어", Status::English),
            ("Catchtype: 권한 필요", Status::Inactive),
            ("Catchtype: 보안 입력 중", Status::Inactive),
        ] {
            assert_eq!(parse_label(label), Some(expected), "{label}");
        }
    }

    #[test]
    fn logo_icon_style_has_no_language() {
        assert_eq!(parse_label("Catchtype 켜짐"), Some(Status::Unknown));
        assert_eq!(parse_label("Catchtype: 켜짐"), Some(Status::Unknown));
    }

    #[test]
    fn inactive_and_unrecognized_labels_do_not_force_a_language() {
        assert_eq!(
            parse_label("Catchtype: 비밀번호 입력 중"),
            Some(Status::Inactive)
        );
        assert_eq!(parse_label("Catchtype: unknown"), Some(Status::Unknown));
        assert_eq!(parse_label("Another app: 한글"), None);
        assert_eq!(parse_label("Mem Reduct 한글"), None);
    }
}
