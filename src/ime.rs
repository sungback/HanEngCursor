use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::Ime::ImmGetDefaultIMEWnd;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, SendMessageW, GUITHREADINFO,
    WM_IME_CONTROL,
};

/// 한/영 상태를 나타내는 단순한 이름표(열거형)입니다.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImeMode {
    English,
    Korean,
}

/// IMC_GETCONVERSIONMODE = 0x0001 (subcommand for WM_IME_CONTROL)
const IMC_GETCONVERSIONMODE: usize = 0x0001;

/// IME_CMODE_NATIVE = 0x0001 (native/Korean input)
const IME_CMODE_NATIVE: isize = 0x0001;

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
        // Get the default IME window for the target window
        let ime_wnd = ImmGetDefaultIMEWnd(hwnd);
        if ime_wnd.0.is_null() {
            return ImeMode::English;
        }

        // Send WM_IME_CONTROL with IMC_GETCONVERSIONMODE to query conversion mode
        let conversion_mode = SendMessageW(
            ime_wnd,
            WM_IME_CONTROL,
            Some(WPARAM(IMC_GETCONVERSIONMODE)),
            Some(LPARAM(0)),
        );

        // Check IME_CMODE_NATIVE flag
        if conversion_mode.0 & IME_CMODE_NATIVE != 0 {
            ImeMode::Korean
        } else {
            ImeMode::English
        }
    }
}
