#![windows_subsystem = "windows"]

mod catchtype;
mod ime;
mod overlay;
mod tray;

use windows::core::{w, HSTRING};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, KillTimer,
    PostQuitMessage, RegisterClassW, SetTimer, TranslateMessage, CS_HREDRAW, CS_VREDRAW,
    CW_USEDEFAULT, MSG, WM_COMMAND, WM_DESTROY, WM_LBUTTONUP, WM_RBUTTONUP, WM_TIMER, WNDCLASSW,
    WS_EX_TOOLWINDOW, WS_OVERLAPPEDWINDOW,
};

use crate::overlay::Overlay;
use crate::tray::{TrayIcon, WM_TRAY_CALLBACK};

// 1초에 20번 (50ms 마다) 상태를 체크하기 위한 타이머 설정값
const TIMER_ID: usize = 1;
const TIMER_INTERVAL_MS: u32 = 50;
const CLASS_NAME: windows::core::PCWSTR = w!("HanEngCursorMainWnd");

// 프로그램 전역에 상태를 저장하는 공간 (오버레이 마술사창, 트레이 팩토리)
// *주의: Rust에서 전역 변수는 `unsafe` 블록 안에서만 안전성을 개발자가 책임지고 써야 합니다.
static mut OVERLAY: Option<Overlay> = None;
static mut TRAY: Option<TrayIcon> = None;

/// 프로그램의 심장 (지휘관) - 가장 먼저 실행되는 함수입니다.
fn main() {
    if let Err(e) = run() {
        unsafe {
            let msg = HSTRING::from(format!("앱 시작 실패:\n{:?}", e));
            windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                None,
                &msg,
                w!("HanEngCursor 오류"),
                windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
            );
        }
    }
}

fn run() -> windows::core::Result<()> {
    // Accessibility work runs independently from the cursor/message loop.
    // If the optional monitor cannot start, Windows IME detection still works.
    let _catchtype_monitor = catchtype::Monitor::start().ok();
    unsafe {
        let hinstance = GetModuleHandleW(None)?;

        // 1. 눈에 보이지 않는 "유령 창(Message-only window)"의 등록증을 만듭니다.
        // 이 창은 화면에는 보이지 않지만, 윈도우 시스템과 통신하며 타이머 신호를 받는 지휘소 역할을 합니다.
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(main_wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        RegisterClassW(&wc);

        // 2. 등록증을 바탕으로 실제 유령 창을 생성합니다.
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            CLASS_NAME,
            w!("HanEngCursor"),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            0,
            0,
            None,
            None,
            Some(hinstance.into()),
            None,
        )?;

        // Initialize overlay
        let overlay_result = Overlay::new();
        match overlay_result {
            Ok(o) => {
                OVERLAY = Some(o);
            }
            Err(e) => {
                eprintln!("Failed to create overlay: {:?}", e);
                return Err(e);
            }
        }

        // Initialize tray icon
        let tray_result = TrayIcon::new(hwnd);
        match tray_result {
            Ok(t) => {
                TRAY = Some(t);
            }
            Err(e) => {
                eprintln!("Failed to create tray icon: {:?}", e);
                return Err(e);
            }
        }

        // 5. 지휘관의 핵심: 50ms마다 알람(타이머)이 울리도록 설정합니다.
        // "WM_TIMER" 라는 메시지가 50ms 마다 날아오게 됩니다.
        SetTimer(Some(hwnd), TIMER_ID, TIMER_INTERVAL_MS, None);

        // 6. 무한 루프 (메시지 루프)
        // 프로그램이 바로 꺼지지 않고 계속 실행되면서 윈도우에서 오는 우편물(클릭, 타이머 알람 등)을 받아 처리합니다.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Cleanup (window already destroyed, ignore KillTimer error)
        let _ = KillTimer(Some(hwnd), TIMER_ID);
        TRAY = None;
        OVERLAY = None;

        Ok(())
    }
}

unsafe extern "system" fn main_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TIMER => {
            if wparam.0 == TIMER_ID {
                // Poll IME status and update overlay & tray
                let mode = match catchtype::current_status() {
                    Some(catchtype::Status::Korean) => ime::ImeMode::Korean,
                    Some(catchtype::Status::English) => ime::ImeMode::English,
                    _ => ime::get_ime_mode(),
                };
                if let Some(ref overlay) = OVERLAY {
                    overlay.update(mode);
                }
                if let Some(ref mut tray) = TRAY {
                    tray.update_icon(mode);
                }
            }
            LRESULT(0)
        }
        WM_TRAY_CALLBACK => {
            let event = (lparam.0 & 0xFFFF) as u32;
            match event {
                WM_RBUTTONUP => {
                    if let Some(ref tray_icon) = TRAY {
                        tray_icon.show_context_menu();
                    }
                }
                WM_LBUTTONUP => {
                    // Left click on tray — could toggle visibility in the future
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let cmd = (wparam.0 & 0xFFFF) as u32;
            if cmd == 1001 {
                // IDM_EXIT
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
