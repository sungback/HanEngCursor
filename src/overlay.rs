use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, Ellipse, EndPaint,
    FillRect, InvalidateRect, SelectObject, SetBkMode, SetTextColor, DT_CENTER, DT_SINGLELINE,
    DT_VCENTER, FONT_CHARSET, FONT_CLIP_PRECISION, FONT_OUTPUT_PRECISION, FONT_QUALITY, FW_BOLD,
    HFONT, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetClientRect, GetCursorInfo, GetCursorPos, GetIconInfo,
    GetSystemMetrics, LoadCursorW, PostQuitMessage, RegisterClassW, SetLayeredWindowAttributes,
    SetWindowPos, ShowWindow, CS_HREDRAW, CS_VREDRAW, CURSORINFO, CW_USEDEFAULT, HWND_TOPMOST,
    ICONINFO, IDC_ARROW, LWA_ALPHA, LWA_COLORKEY, SM_CXCURSOR, SWP_NOACTIVATE, SWP_NOSIZE,
    SW_SHOWNOACTIVATE, WM_DESTROY, WM_PAINT, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::ime::ImeMode;

// 윈도우에서 사용할 색상표 (0x00BBGGRR 형식: 파랑, 초록, 빨강 순서입니다)
const COLOR_KEY: COLORREF = COLORREF(0x00FF00FF); // 마젠타(자홍색) - 이 색상은 투명하게 뚫려서 뒤가 보이게 됩니다.
const COLOR_KOREAN_BG: COLORREF = COLORREF(0x0044AA44); // 한글 모드일 때 배경색 (초록색 계열)
const COLOR_ENGLISH_BG: COLORREF = COLORREF(0x004444AA); // 영문 모드일 때 배경색 (빨간색 계열)
const COLOR_TEXT: COLORREF = COLORREF(0x00FFFFFF); // 글자색 (흰색)

// Overlay size
const OVERLAY_WIDTH: i32 = 24;
const OVERLAY_HEIGHT: i32 = 24;
const OVERLAY_OFFSET_X: i32 = 20;
const OVERLAY_OFFSET_Y: i32 = 0;

const CLASS_NAME: windows::core::PCWSTR = w!("HanEngCursorOverlay");

/// State stored for the overlay
static mut CURRENT_MODE: ImeMode = ImeMode::English;
static mut CURRENT_CAPS: bool = false;
static mut LAST_Y_OFFSET: i32 = 12;

pub struct Overlay {
    pub hwnd: HWND,
    _font: HFONT,
}

impl Overlay {
    pub fn new() -> windows::core::Result<Self> {
        unsafe {
            let hinstance = GetModuleHandleW(None)?;

            let wc = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(overlay_wnd_proc),
                hInstance: hinstance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                lpszClassName: CLASS_NAME,
                ..Default::default()
            };

            RegisterClassW(&wc);

            // 마술사 창의 가장 마법 같은 부분입니다! 투명하고 클릭이 통과되는 마법의 창문 속성들입니다.
            let ex_style = WS_EX_LAYERED      // 투명도를 조절할 수 있게 합니다
                | WS_EX_TOPMOST               // 항상 다른 창들 위에 떠 있게 합니다
                | WS_EX_TRANSPARENT           // 마우스 클릭이 창을 통과해서 뒤에 있는 앱이 눌리게 합니다
                | WS_EX_TOOLWINDOW            // 작업 표시줄(하단 바)에 이 창의 아이콘이 나타나지 않게 숨깁니다
                | WS_EX_NOACTIVATE; // 이 창이 떠도 사용자가 타자를 치던 창의 포커스를 뺏어오지 않습니다.

            let hwnd = CreateWindowExW(
                ex_style,
                CLASS_NAME,
                w!("IME Indicator"),
                WS_POPUP,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                OVERLAY_WIDTH,
                OVERLAY_HEIGHT,
                None,
                None,
                Some(hinstance.into()),
                None,
            )?;

            // 마젠타(자홍색)를 투명하게 처리하고, 전체 창에 약간의 투명도(180/255)를 적용합니다.
            SetLayeredWindowAttributes(hwnd, COLOR_KEY, 180, LWA_COLORKEY | LWA_ALPHA)?;

            // Create font for the indicator text
            let font = create_indicator_font();

            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

            Ok(Overlay { hwnd, _font: font })
        }
    }

    /// 지휘관(main.rs)이 "야! 상태 업데이트해!" 라고 부를 때마다 실행되는 함수
    pub fn update(&self, mode: ImeMode) {
        unsafe {
            let prev_mode = CURRENT_MODE;
            CURRENT_MODE = mode;

            // 현재 마우스 커서의 X, Y 좌표를 가져옵니다.
            let mut pt = POINT { x: 0, y: 0 };
            let _ = GetCursorPos(&mut pt);

            // Get cursor info for hotspot offset
            let mut cursor_y_offset = LAST_Y_OFFSET;
            let mut ci = CURSORINFO {
                cbSize: std::mem::size_of::<CURSORINFO>() as u32,
                ..Default::default()
            };

            if GetCursorInfo(&mut ci).is_ok() {
                // 커서가 감추어져 있거나 정보가 불분명할 때는 이전 위치를 유지합니다.
                if !ci.hCursor.is_invalid() {
                    // I-beam 커서인지를 먼저 확인합니다 (텍스트 입력 상태)
                    let h_ibeam =
                        LoadCursorW(None, windows::Win32::UI::WindowsAndMessaging::IDC_IBEAM)
                            .unwrap_or_default();
                    if ci.hCursor == h_ibeam {
                        cursor_y_offset = -12;
                    } else {
                        // 다른 커서일 경우 hotspot 정보를 확인합니다.
                        let mut ii = ICONINFO::default();
                        if GetIconInfo(ci.hCursor.into(), &mut ii).is_ok() {
                            // 핫스팟의 Y 위치가 크면 I-beam과 유사한 상황으로 간주합니다.
                            if ii.yHotspot > 5 {
                                cursor_y_offset = -12;
                            } else {
                                cursor_y_offset = 12;
                            }
                            // GDI 리소스를 회수합니다 (중요: 누수 방지)
                            if !ii.hbmMask.is_invalid() {
                                let _ = DeleteObject(ii.hbmMask.into());
                            }
                            if !ii.hbmColor.is_invalid() {
                                let _ = DeleteObject(ii.hbmColor.into());
                            }
                        }
                    }
                }
            }

            LAST_Y_OFFSET = cursor_y_offset;

            // Get cursor size for offset
            let _cursor_w = GetSystemMetrics(SM_CXCURSOR);

            // 마우스 커서 위치에서 살짝 오른쪽(X: +20)으로 비켜난 곳으로 목표 위치를 잡습니다.
            let x = pt.x + OVERLAY_OFFSET_X;
            let y = pt.y + OVERLAY_OFFSET_Y + cursor_y_offset;

            // 마술사 창을 그 목표 좌표로 찰싹 이동시킵니다.
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE,
            );

            let caps_lock_on = (windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(
                windows::Win32::UI::Input::KeyboardAndMouse::VK_CAPITAL.0 as i32,
            ) & 0x0001)
                != 0;

            let prev_caps = CURRENT_CAPS;
            CURRENT_CAPS = caps_lock_on;

            // Redraw if mode changed or caps lock changed
            if prev_mode != mode || (mode == ImeMode::English && prev_caps != caps_lock_on) {
                let _ = InvalidateRect(Some(self.hwnd), None, true);
            }
        }
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        unsafe {
            if !self._font.is_invalid() {
                let _ = DeleteObject(self._font.into());
            }
        }
    }
}

/// Create the font used for the indicator
fn create_indicator_font() -> HFONT {
    unsafe {
        CreateFontW(
            15,                       // height
            0,                        // width (auto)
            0,                        // escapement
            0,                        // orientation
            FW_BOLD.0 as i32,         // weight
            0,                        // italic
            0,                        // underline
            0,                        // strikeout
            FONT_CHARSET(1),          // DEFAULT_CHARSET (supports Korean)
            FONT_OUTPUT_PRECISION(0), // OUT_DEFAULT_PRECIS
            FONT_CLIP_PRECISION(0),   // CLIP_DEFAULT_PRECIS
            FONT_QUALITY(0),          // DEFAULT_QUALITY
            0,                        // pitch and family
            w!("맑은 고딕"),          // Malgun Gothic — Korean support
        )
    }
}

/// Window procedure for the overlay
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

            // Fill background with transparent color key
            let bg_brush = CreateSolidBrush(COLOR_KEY);
            let mut client_rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut client_rect);
            let _ = FillRect(hdc, &client_rect, bg_brush);
            let _ = DeleteObject(bg_brush.into());

            // Draw rounded rectangle background based on mode
            let mode = CURRENT_MODE;
            let bg_color = match mode {
                ImeMode::Korean => COLOR_KOREAN_BG,
                ImeMode::English => COLOR_ENGLISH_BG,
            };

            let fill_brush = CreateSolidBrush(bg_color);
            let old_brush = SelectObject(hdc, fill_brush.into());

            // Draw circle
            let _ = Ellipse(hdc, 0, 0, OVERLAY_WIDTH, OVERLAY_HEIGHT);

            SelectObject(hdc, old_brush);
            let _ = DeleteObject(fill_brush.into());

            // Draw text
            let _ = SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, COLOR_TEXT);

            // Create and select font
            let font = create_indicator_font();
            let old_font = SelectObject(hdc, font.into());

            let text: Vec<u16> = match mode {
                ImeMode::Korean => "한".encode_utf16().collect(),
                ImeMode::English => {
                    let caps_lock_on = (windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(
                        windows::Win32::UI::Input::KeyboardAndMouse::VK_CAPITAL.0 as i32,
                    ) & 0x0001)
                        != 0;
                    if caps_lock_on {
                        "A".encode_utf16().collect()
                    } else {
                        "a".encode_utf16().collect()
                    }
                }
            };

            let mut text_rect = RECT {
                left: 0,
                top: 0,
                right: OVERLAY_WIDTH,
                bottom: OVERLAY_HEIGHT,
            };

            let _ = DrawTextW(
                hdc,
                &mut text.clone(),
                &mut text_rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );

            SelectObject(hdc, old_font);
            let _ = DeleteObject(font.into());

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
