#![windows_subsystem = "windows"]

mod catchtype;
mod ime;
mod layout;
mod overlay;
mod tray;
mod window;

use overlay::Overlay;
use std::cell::{Cell, RefCell};
use tray::{TrayIcon, IDM_EXIT, WM_TRAY_CALLBACK};
use window::OwnedWindow;
use windows::core::{w, Error, Result, HSTRING};
use windows::Win32::Foundation::{ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, KillTimer, PostMessageW, PostQuitMessage, RegisterClassW,
    RegisterWindowMessageW, SetTimer, SetWindowLongPtrW, TranslateMessage, GWLP_USERDATA, MSG,
    WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_NCDESTROY, WM_RBUTTONUP, WM_TIMER, WNDCLASSW,
    WS_EX_TOOLWINDOW, WS_OVERLAPPEDWINDOW,
};

const TIMER_ID: usize = 1;
const TIMER_INTERVAL_MS: u32 = 50;
const CLASS_NAME: windows::core::PCWSTR = w!("HanEngCursorMainWnd");

struct App {
    // Destroy the window before freeing anything its callback can access.
    window: OwnedWindow,
    overlay: Overlay,
    tray: RefCell<TrayIcon>,
    error: RefCell<Option<Error>>,
    updating: Cell<bool>,
    catchtype: Option<catchtype::Monitor>,
    taskbar_created: u32,
    tray_reset: Cell<bool>,
}

impl App {
    fn update(&self) -> Result<()> {
        // Win32 calls can pump nested messages. Never borrow the tray twice.
        if self.updating.replace(true) {
            return Ok(());
        }
        let result = (|| {
            let mode = match self
                .catchtype
                .as_ref()
                .and_then(catchtype::Monitor::current_status)
            {
                Some(catchtype::Status::Korean) => ime::ImeMode::Korean,
                Some(catchtype::Status::English) => ime::ImeMode::English,
                _ => ime::get_ime_mode(),
            };
            self.overlay.update(mode)?;
            let mut tray = self.tray.borrow_mut();
            if self.tray_reset.replace(false) {
                tray.taskbar_created();
            }
            tray.update_icon(mode);
            Ok(())
        })();
        self.updating.set(false);
        result
    }

    fn fail(&self, error: Error) {
        let mut stored = self.error.borrow_mut();
        if stored.is_none() {
            *stored = Some(error);
            // Show errors only after dispatch and resource cleanup have finished.
            unsafe { PostQuitMessage(1) };
        }
    }
}

fn main() -> std::process::ExitCode {
    if let Err(error) = run() {
        unsafe {
            let message = HSTRING::from(format!(
                "HanEngCursor 실행 중 오류가 발생했습니다.\n{error}"
            ));
            windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                None,
                &message,
                w!("HanEngCursor 오류"),
                windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
            );
        }
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

fn run() -> Result<()> {
    // Optional integration: IME detection remains available if it cannot start.
    let catchtype = catchtype::Monitor::start().ok();
    let app = create_app(catchtype)?;
    unsafe {
        if SetTimer(Some(app.window.0), TIMER_ID, TIMER_INTERVAL_MS, None) == 0 {
            return Err(Error::from_thread());
        }
        let result = message_loop();
        let _ = KillTimer(Some(app.window.0), TIMER_ID);
        if let Some(error) = app.error.borrow_mut().take() {
            return Err(error);
        }
        result
    }
}

fn create_app(catchtype: Option<catchtype::Monitor>) -> Result<Box<App>> {
    unsafe {
        let hinstance = GetModuleHandleW(None)?;
        let taskbar_created = RegisterWindowMessageW(w!("TaskbarCreated"));
        if taskbar_created == 0 {
            return Err(Error::from_thread());
        }
        let wc = WNDCLASSW {
            lpfnWndProc: Some(main_wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            let error = Error::from_thread();
            if error.code() != ERROR_CLASS_ALREADY_EXISTS.to_hresult() {
                return Err(error);
            }
        }
        let window = OwnedWindow(CreateWindowExW(
            WS_EX_TOOLWINDOW,
            CLASS_NAME,
            w!("HanEngCursor"),
            WS_OVERLAPPEDWINDOW,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinstance.into()),
            None,
        )?);
        let overlay = Overlay::new()?;
        let tray = TrayIcon::new(window.0)?;
        let app = Box::new(App {
            window,
            overlay,
            tray: RefCell::new(tray),
            error: RefCell::new(None),
            updating: Cell::new(false),
            catchtype,
            taskbar_created,
            tray_reset: Cell::new(false),
        });
        // The Box stays alive throughout dispatch and window destruction.
        app.window.attach(app.as_ref())?;
        Ok(app)
    }
}

unsafe fn message_loop() -> Result<()> {
    let mut msg = MSG::default();
    loop {
        match GetMessageW(&mut msg, None, 0, 0).0 {
            -1 => return Err(Error::from_thread()),
            0 => return Ok(()),
            _ => {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

unsafe extern "system" fn main_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // Only shared references cross Win32 calls; mutations use UI-thread cells.
    let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const App;
    if !state.is_null() && msg == (&*state).taskbar_created {
        // Shell calls may reenter while the tray is borrowed by update().
        (&*state).tray_reset.set(true);
        return LRESULT(0);
    }
    match msg {
        WM_TIMER if wparam.0 == TIMER_ID && !state.is_null() => {
            let app = &*state;
            if let Err(error) = app.update() {
                app.fail(error);
            }
            LRESULT(0)
        }
        WM_TRAY_CALLBACK if (lparam.0 & 0xFFFF) as u32 == WM_RBUTTONUP => {
            // The modal menu receives a copied HWND, never a borrowed TrayIcon.
            if let Err(error) = tray::show_context_menu(hwnd) {
                if !state.is_null() {
                    (&*state).fail(error);
                }
            }
            LRESULT(0)
        }
        WM_COMMAND if (wparam.0 & 0xFFFF) as u32 == IDM_EXIT => {
            if let Err(error) = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) {
                if !state.is_null() {
                    (&*state).fail(error);
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            if let Err(error) = DestroyWindow(hwnd) {
                if !state.is_null() {
                    (&*state).fail(error);
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = KillTimer(Some(hwnd), TIMER_ID);
            // OwnedWindow detaches before cleanup. Only a live application's
            // external close should quit; error cleanup must not leave WM_QUIT
            // queued ahead of the error dialog.
            if !state.is_null() {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        WM_NCDESTROY => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::{IsWindow, PeekMessageW, PM_REMOVE, WM_QUIT};

    #[test]
    #[ignore = "requires an interactive Windows desktop with Explorer"]
    fn shell_registration_recovers_after_loss_and_repeated_taskbar_messages() {
        use windows::Win32::UI::Shell::{
            Shell_NotifyIconW, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
        };
        use windows::Win32::UI::WindowsAndMessaging::SendMessageW;
        unsafe {
            let app = create_app(None).expect("initialize app");
            let hwnd = app.window.0;
            let nid = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: 1,
                ..Default::default()
            };
            for _ in 0..3 {
                assert!(Shell_NotifyIconW(NIM_DELETE, &nid).as_bool());
                SendMessageW(hwnd, app.taskbar_created, None, None);
                assert!(app.tray_reset.get());
                SendMessageW(hwnd, WM_TIMER, Some(WPARAM(TIMER_ID)), None);
                assert!(!app.tray_reset.get());
                assert!(app.error.borrow().is_none());
                assert!(
                    Shell_NotifyIconW(NIM_MODIFY, &nid).as_bool(),
                    "icon should be registered again"
                );
            }
            // A duplicate broadcast while the icon still exists is idempotent.
            SendMessageW(hwnd, app.taskbar_created, None, None);
            SendMessageW(hwnd, WM_TIMER, Some(WPARAM(TIMER_ID)), None);
            assert!(Shell_NotifyIconW(NIM_MODIFY, &nid).as_bool());
            drop(app);
            assert!(!IsWindow(Some(hwnd)).as_bool());
            assert!(!Shell_NotifyIconW(NIM_MODIFY, &nid).as_bool());
            let mut message = MSG::default();
            assert!(!PeekMessageW(&mut message, None, WM_QUIT, WM_QUIT, PM_REMOVE).as_bool());
        }
    }

    #[test]
    fn initialization_cleanup_destroys_window_without_leaving_quit_message() {
        unsafe {
            let hinstance = GetModuleHandleW(None).unwrap();
            let class = w!("HanEngCursorCleanupTest");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(main_wnd_proc),
                hInstance: hinstance.into(),
                lpszClassName: class,
                ..Default::default()
            };
            assert_ne!(RegisterClassW(&wc), 0);
            let window = OwnedWindow(
                CreateWindowExW(
                    WS_EX_TOOLWINDOW,
                    class,
                    w!("cleanup test"),
                    WS_OVERLAPPEDWINDOW,
                    0,
                    0,
                    0,
                    0,
                    None,
                    None,
                    Some(hinstance.into()),
                    None,
                )
                .unwrap(),
            );
            let hwnd = window.0;
            // Models a failure before App has been constructed and attached.
            drop(window);
            assert!(!IsWindow(Some(hwnd)).as_bool());
            let mut message = MSG::default();
            assert!(!PeekMessageW(&mut message, None, WM_QUIT, WM_QUIT, PM_REMOVE).as_bool());
        }
    }
}
