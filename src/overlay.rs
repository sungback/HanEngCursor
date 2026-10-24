use crate::{ime::ImeMode, layout, window::OwnedWindow};
use std::cell::Cell;
use windows::core::{w, Error, Result};
use windows::Win32::Foundation::{
    COLORREF, ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, Ellipse, EndPaint,
    FillRect, GetMonitorInfoW, InvalidateRect, MonitorFromPoint, SelectObject, SetBkMode,
    SetTextColor, DT_CENTER, DT_SINGLELINE, DT_VCENTER, FONT_CHARSET, FONT_CLIP_PRECISION,
    FONT_OUTPUT_PRECISION, FONT_QUALITY, FW_BOLD, HFONT, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetClientRect, GetCursorInfo, GetCursorPos, GetIconInfo,
    GetWindowLongPtrW, LoadCursorW, RegisterClassW, SetLayeredWindowAttributes, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, CURSORINFO, GWLP_USERDATA, HWND_TOPMOST, ICONINFO, IDC_ARROW,
    IDC_IBEAM, LWA_ALPHA, LWA_COLORKEY, SWP_NOACTIVATE, SW_SHOWNOACTIVATE, WM_DPICHANGED,
    WM_NCDESTROY, WM_PAINT, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

const COLOR_KEY: COLORREF = COLORREF(0x00FF00FF);
const COLOR_KOREAN_BG: COLORREF = COLORREF(0x0044AA44);
const COLOR_ENGLISH_BG: COLORREF = COLORREF(0x004444AA);
const COLOR_TEXT: COLORREF = COLORREF(0x00FFFFFF);
const OVERLAY_WIDTH: i32 = 24;
const OVERLAY_HEIGHT: i32 = 24;

const CLASS_NAME: windows::core::PCWSTR = w!("HanEngCursorOverlay");

struct PaintState {
    mode: Cell<ImeMode>,
    caps: Cell<bool>,
    font: Cell<HFONT>,
    font_dpi: Cell<u32>,
}

impl Drop for PaintState {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.font.get().into());
        }
    }
}

pub struct Overlay {
    // Destroy the callback's window before freeing its boxed paint state.
    window: OwnedWindow,
    state: Box<PaintState>,
    last_y_offset: Cell<i32>,
}

impl Overlay {
    pub fn new() -> Result<Self> {
        unsafe {
            let hinstance = GetModuleHandleW(None)?;
            let wc = WNDCLASSW {
                lpfnWndProc: Some(overlay_wnd_proc),
                hInstance: hinstance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                lpszClassName: CLASS_NAME,
                ..Default::default()
            };
            if RegisterClassW(&wc) == 0 {
                let error = Error::from_thread();
                if error.code() != ERROR_CLASS_ALREADY_EXISTS.to_hresult() {
                    return Err(error);
                }
            }
            let font = create_indicator_font(96);
            if font.is_invalid() {
                return Err(Error::new(
                    windows::core::HRESULT(0x80004005u32 as i32),
                    "표시 글꼴을 만들지 못했습니다.",
                ));
            }
            let state = Box::new(PaintState {
                mode: Cell::new(ImeMode::English),
                caps: Cell::new(false),
                font: Cell::new(font),
                font_dpi: Cell::new(96),
            });
            let window = OwnedWindow(CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TOPMOST
                    | WS_EX_TRANSPARENT
                    | WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE,
                CLASS_NAME,
                w!("IME Indicator"),
                WS_POPUP,
                0,
                0,
                OVERLAY_WIDTH,
                OVERLAY_HEIGHT,
                None,
                None,
                Some(hinstance.into()),
                None,
            )?);
            // Construct the owner before attaching: all failure paths destroy the
            // HWND before freeing the state, including layered-window setup failure.
            let overlay = Self {
                window,
                state,
                last_y_offset: Cell::new(12),
            };
            overlay.window.attach(overlay.state.as_ref())?;
            SetLayeredWindowAttributes(overlay.window.0, COLOR_KEY, 180, LWA_COLORKEY | LWA_ALPHA)?;
            let _ = ShowWindow(overlay.window.0, SW_SHOWNOACTIVATE);
            Ok(overlay)
        }
    }

    pub fn update(&self, mode: ImeMode) -> Result<()> {
        unsafe {
            let mut pt = POINT::default();
            // Cursor queries can temporarily fail on a different input desktop.
            // Keep the last position rather than moving the indicator to (0, 0).
            if GetCursorPos(&mut pt).is_err() {
                return Ok(());
            }
            let mut offset = self.last_y_offset.get();
            let mut ci = CURSORINFO {
                cbSize: std::mem::size_of::<CURSORINFO>() as u32,
                ..Default::default()
            };
            if GetCursorInfo(&mut ci).is_ok() && !ci.hCursor.is_invalid() {
                if LoadCursorW(None, IDC_IBEAM).is_ok_and(|ibeam| ci.hCursor == ibeam) {
                    offset = -12;
                } else {
                    let mut ii = ICONINFO::default();
                    if GetIconInfo(ci.hCursor.into(), &mut ii).is_ok() {
                        offset = if ii.yHotspot > 5 { -12 } else { 12 };
                        if !ii.hbmMask.is_invalid() {
                            let _ = DeleteObject(ii.hbmMask.into());
                        }
                        if !ii.hbmColor.is_invalid() {
                            let _ = DeleteObject(ii.hbmColor.into());
                        }
                    }
                }
            }
            self.last_y_offset.set(offset);
            let monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            // Display topology may be changing; retry on the next timer tick.
            if !GetMonitorInfoW(monitor, &mut info).as_bool() {
                return Ok(());
            }
            let mut resized = false;
            // A cross-monitor move changes the HWND's DPI synchronously. The
            // second pass applies its new scale within the same timer tick.
            for _ in 0..2 {
                let dpi = GetDpiForWindow(self.window.0).max(96);
                if self.state.font_dpi.get() != dpi {
                    let font = create_indicator_font(dpi);
                    if font.is_invalid() {
                        return Err(Error::new(
                            windows::core::HRESULT(0x80004005u32 as i32),
                            "표시 글꼴 크기를 변경하지 못했습니다.",
                        ));
                    }
                    let previous = self.state.font.replace(font);
                    self.state.font_dpi.set(dpi);
                    let _ = DeleteObject(previous.into());
                    resized = true;
                }
                let rect = layout::indicator_rect(pt, info.rcWork, dpi, offset);
                SetWindowPos(
                    self.window.0,
                    Some(HWND_TOPMOST),
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOACTIVATE,
                )?;
                if GetDpiForWindow(self.window.0).max(96) == dpi {
                    break;
                }
            }
            let caps = windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(
                windows::Win32::UI::Input::KeyboardAndMouse::VK_CAPITAL.0 as i32,
            ) & 1
                != 0;
            let previous_mode = self.state.mode.replace(mode);
            let previous_caps = self.state.caps.replace(caps);
            if resized
                || previous_mode != mode
                || (mode == ImeMode::English && previous_caps != caps)
            {
                InvalidateRect(Some(self.window.0), None, true).ok()?;
            }
            Ok(())
        }
    }
}

fn create_indicator_font(dpi: u32) -> HFONT {
    unsafe {
        CreateFontW(
            layout::scale(15, dpi),
            0,
            0,
            0,
            FW_BOLD.0 as i32,
            0,
            0,
            0,
            FONT_CHARSET(1),
            FONT_OUTPUT_PRECISION(0),
            FONT_CLIP_PRECISION(0),
            FONT_QUALITY(0),
            0,
            w!("맑은 고딕"),
        )
    }
}

unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const PaintState;
            if !state.is_null() && !hdc.is_invalid() {
                let state = &*state;
                let bg = CreateSolidBrush(COLOR_KEY);
                let mut rect = RECT::default();
                if GetClientRect(hwnd, &mut rect).is_ok() {
                    let _ = FillRect(hdc, &rect, bg);
                }
                let _ = DeleteObject(bg.into());
                let mode = state.mode.get();
                let fill = CreateSolidBrush(match mode {
                    ImeMode::Korean => COLOR_KOREAN_BG,
                    ImeMode::English => COLOR_ENGLISH_BG,
                });
                let old_brush = SelectObject(hdc, fill.into());
                let _ = Ellipse(hdc, 0, 0, rect.right, rect.bottom);
                SelectObject(hdc, old_brush);
                let _ = DeleteObject(fill.into());
                let _ = SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, COLOR_TEXT);
                let old_font = SelectObject(hdc, state.font.get().into());
                let mut text: Vec<u16> = match mode {
                    ImeMode::Korean => "한",
                    ImeMode::English if state.caps.get() => "A",
                    ImeMode::English => "a",
                }
                .encode_utf16()
                .collect();
                let mut text_rect = rect;
                let _ = DrawTextW(
                    hdc,
                    &mut text,
                    &mut text_rect,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
                SelectObject(hdc, old_font);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        // Cursor anchoring and work-area limits replace the suggested rectangle.
        // update() reflows immediately after SetWindowPos changes the DPI.
        WM_DPICHANGED => LRESULT(0),
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
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, IsWindow, PeekMessageW, MSG, PM_REMOVE, WM_QUIT,
    };

    #[test]
    #[ignore = "requires an interactive Windows input desktop"]
    fn native_window_uses_monitor_dpi_and_stays_in_the_work_area() {
        use windows::Win32::UI::HiDpi::{
            GetAwarenessFromDpiAwarenessContext, GetThreadDpiAwarenessContext,
            DPI_AWARENESS_PER_MONITOR_AWARE,
        };
        unsafe {
            assert_eq!(
                GetAwarenessFromDpiAwarenessContext(GetThreadDpiAwarenessContext()),
                DPI_AWARENESS_PER_MONITOR_AWARE
            );
            let overlay = Overlay::new().expect("overlay");
            overlay.update(ImeMode::Korean).expect("reflow");
            let dpi = GetDpiForWindow(overlay.window.0);
            let mut rect = RECT::default();
            GetWindowRect(overlay.window.0, &mut rect).unwrap();
            let mut cursor = POINT::default();
            GetCursorPos(&mut cursor).unwrap();
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            assert!(GetMonitorInfoW(
                MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST),
                &mut info
            )
            .as_bool());
            assert_eq!(rect.right - rect.left, layout::scale(24, dpi));
            assert_eq!(overlay.state.font_dpi.get(), dpi);
            assert!(rect.left >= info.rcWork.left && rect.top >= info.rcWork.top);
            assert!(rect.right <= info.rcWork.right && rect.bottom <= info.rcWork.bottom);
            println!("native DPI={dpi}, badge={}px", rect.right - rect.left);
        }
    }

    #[test]
    fn overlays_keep_independent_state_and_release_windows_without_quitting() {
        unsafe {
            let first = Overlay::new().expect("first overlay");
            let second = Overlay::new().expect("second overlay");
            first.state.mode.set(ImeMode::Korean);
            first.state.caps.set(true);
            assert_eq!(second.state.mode.get(), ImeMode::English);
            assert!(!second.state.caps.get());
            let hwnd = first.window.0;
            drop(first);
            assert!(!IsWindow(Some(hwnd)).as_bool());
            assert!(IsWindow(Some(second.window.0)).as_bool());
            drop(second);
            let mut message = MSG::default();
            assert!(!PeekMessageW(&mut message, None, WM_QUIT, WM_QUIT, PM_REMOVE).as_bool());
        }
    }
}
