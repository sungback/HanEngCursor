use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::Ime::{ImmGetDefaultIMEWnd, IME_CMODE_NATIVE};
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayout;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, SendMessageTimeoutW,
    GUITHREADINFO, SMTO_ABORTIFHUNG, SMTO_BLOCK, SMTO_ERRORONEXIT, WM_IME_CONTROL,
};

/// 한/영 상태를 나타내는 단순한 이름표(열거형)입니다.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImeMode {
    English,
    Korean,
}

/// IMC_GETCONVERSIONMODE = 0x0001 (subcommand for WM_IME_CONTROL)
const IMC_GETCONVERSIONMODE: usize = 0x0001;

/// WM_IME_CONTROL subcommand for the IME's open/closed state.
const IMC_GETOPENSTATUS: usize = 0x0005;
const IME_QUERY_TIMEOUT_MS: u32 = 20;
const LANG_KOREAN: usize = 0x12;

/// A successful query can return zero; it must not be confused with a timeout.
fn query_ime(ime_wnd: HWND, command: usize) -> Option<usize> {
    let mut result = 0;
    let succeeded = unsafe {
        SendMessageTimeoutW(
            ime_wnd,
            WM_IME_CONTROL,
            WPARAM(command),
            LPARAM(0),
            SMTO_ABORTIFHUNG | SMTO_BLOCK | SMTO_ERRORONEXIT,
            IME_QUERY_TIMEOUT_MS,
            Some(&mut result),
        )
    };
    (succeeded.0 != 0).then_some(result)
}

fn read_ime_mode(mut query: impl FnMut(usize) -> Option<usize>) -> Option<ImeMode> {
    // Conversion flags can retain the last native mode while the IME is closed.
    if query(IMC_GETOPENSTATUS)? == 0 {
        return Some(ImeMode::English);
    }
    let conversion_mode = query(IMC_GETCONVERSIONMODE)?;
    Some(if conversion_mode & IME_CMODE_NATIVE.0 as usize != 0 {
        ImeMode::Korean
    } else {
        ImeMode::English
    })
}

/// 탐정 1단계: 지금 사용자가 글을 쓰고 있는(가장 맨 앞에 활성화된) 창이 누구인지 찾아냅니다.
fn get_focused_window() -> Option<HWND> {
    unsafe {
        // 현재 활성화된(맨 앞에 있는) 창을 가져옵니다. (ex: 크롬 창)
        let fg_hwnd = GetForegroundWindow();
        if fg_hwnd.0.is_null() {
            return None;
        }

        // Get the thread ID of the foreground window
        let thread_id = GetWindowThreadProcessId(fg_hwnd, None);
        if thread_id == 0 {
            return Some(fg_hwnd);
        }

        // Get the focused window within that thread
        let mut gui_info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };

        if GetGUIThreadInfo(thread_id, &mut gui_info).is_ok() {
            let hwnd = gui_info.hwndFocus;
            if !hwnd.0.is_null() {
                return Some(hwnd);
            }
        }

        // Fallback to the foreground window itself
        Some(fg_hwnd)
    }
}

/// 탐정 2단계: 그 창의 입력기(IME)에게 "너 지금 한글이야 영어야?" 라고 물어봅니다.
///
/// 윈도우 시스템 깊숙한 곳의 ImmGetDefaultIMEWnd 함수를 호출해서 입력기를 찾은 뒤,
/// WM_IME_CONTROL 메시지를 던져 상태를 알아냅니다.
pub fn get_ime_mode() -> ImeMode {
    let hwnd = match get_focused_window() {
        Some(h) => h,
        None => return ImeMode::English,
    };

    unsafe {
        // Query the target thread, not this indicator's keyboard layout. A
        // non-Korean layout must not inherit a stale native conversion flag.
        let thread_id = GetWindowThreadProcessId(hwnd, None);
        if thread_id == 0 {
            return ImeMode::English;
        }
        let layout = GetKeyboardLayout(thread_id);
        if layout.0 as usize & 0x03ff != LANG_KOREAN {
            return ImeMode::English;
        }

        // Get the default IME window for the target window.
        let ime_wnd = ImmGetDefaultIMEWnd(hwnd);
        if ime_wnd.0.is_null() {
            return ImeMode::English;
        }

        read_ime_mode(|command| query_ime(ime_wnd, command)).unwrap_or(ImeMode::English)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_ime_does_not_use_stale_conversion_flags() {
        assert_eq!(
            read_ime_mode(|command| {
                assert_eq!(command, IMC_GETOPENSTATUS);
                Some(0)
            }),
            Some(ImeMode::English)
        );
    }

    #[test]
    fn open_ime_distinguishes_native_and_alphanumeric_modes() {
        for (flags, expected) in [
            (0, ImeMode::English),
            (8, ImeMode::English),
            (1, ImeMode::Korean),
            (9, ImeMode::Korean),
        ] {
            assert_eq!(
                read_ime_mode(|command| Some(if command == IMC_GETOPENSTATUS {
                    1
                } else {
                    flags
                })),
                Some(expected)
            );
        }
    }

    #[test]
    fn failed_queries_are_not_successful_english_responses() {
        assert_eq!(read_ime_mode(|_| None), None);
        assert_eq!(
            read_ime_mode(|command| (command == IMC_GETOPENSTATUS).then_some(1)),
            None
        );
    }
}
