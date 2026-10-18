use crate::ime::ImeMode;
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyIcon, DestroyMenu, GetCursorPos, PostMessageW,
    SetForegroundWindow, TrackPopupMenu, HICON, MF_STRING, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
    WM_COMMAND, WM_USER,
};

pub const WM_TRAY_CALLBACK: u32 = WM_USER + 1;
const IDM_EXIT: u32 = 1001;

pub struct TrayIcon {
    nid: NOTIFYICONDATAW,
    hicon_en: HICON,
    hicon_ko: HICON,
    hicon_init: HICON,
    initialized: bool,
}

impl TrayIcon {
    /// 이모지 공장장 - 처음 프로그램이 켜지면 딱 한 번 실행됩니다.
    pub fn new(hwnd: HWND) -> windows::core::Result<Self> {
        unsafe {
            // 리소스에 포함된 국기 및 키보드 아이콘을 불러옵니다.
            let hicon_ko = load_icon_from_resource(201)?;
            let hicon_en = load_icon_from_resource(202)?;
            let hicon_init = load_icon_from_resource(203)?;

            let mut nid = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: 1,
                uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
                uCallbackMessage: WM_TRAY_CALLBACK,
                hIcon: hicon_init, // 처음엔 키보드 보여주기
                ..Default::default()
            };

            // Show the app name and purpose in the tray tooltip.
            let tip = "HanEngCursor — 한/영 표시기";
            let tip_utf16: Vec<u16> = tip.encode_utf16().collect();
            let len = tip_utf16.len().min(nid.szTip.len() - 1);
            nid.szTip[..len].copy_from_slice(&tip_utf16[..len]);
            nid.szTip[len] = 0;

            let _ = Shell_NotifyIconW(NIM_ADD, &nid);

            Ok(TrayIcon {
                nid,
                hicon_en,
                hicon_ko,
                hicon_init,
                initialized: false,
            })
        }
    }

    /// 지휘관(main.rs)이 보내는 신호에 따라 시스템 트레이(우측 하단)의 아이콘 도장을 바꿔치기 합니다.
    pub fn update_icon(&mut self, mode: ImeMode) {
        unsafe {
            let icon = match mode {
                ImeMode::English => self.hicon_en,
                ImeMode::Korean => self.hicon_ko,
            };

            // 초기화된 이후에는 항상 모드에 맞는 아이콘으로 변경,
            // (만약 초기화 전이라도 아이콘이 다르면 변경, 여기서는 바로 아이콘 변경을 위해)
            if self.nid.hIcon != icon || !self.initialized {
                self.nid.hIcon = icon;
                let _ = Shell_NotifyIconW(NIM_MODIFY, &self.nid);
                self.initialized = true;
            }
        }
    }

    /// Show context menu on right click
    pub fn show_context_menu(&self) {
        unsafe {
            let hmenu = CreatePopupMenu().unwrap();
            let _ = AppendMenuW(hmenu, MF_STRING, IDM_EXIT as usize, w!("종료(&X)"));

            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);

            // Required to make the menu disappear when clicking elsewhere
            let _ = SetForegroundWindow(self.nid.hWnd);

            let _ = TrackPopupMenu(
                hmenu,
                TPM_LEFTALIGN | TPM_BOTTOMALIGN,
                pt.x,
                pt.y,
                None,
                self.nid.hWnd,
                None,
            );

            let _ = DestroyMenu(hmenu);
        }
    }

    /// Handle WM_COMMAND from the tray menu
    #[allow(dead_code)]
    pub fn handle_command(wparam: WPARAM) -> bool {
        let cmd = (wparam.0 & 0xFFFF) as u32;
        if cmd == IDM_EXIT {
            unsafe {
                let _ = PostMessageW(None, WM_COMMAND, WPARAM(IDM_EXIT as usize), LPARAM(0));
            }
            return true;
        }
        false
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.nid);
            let _ = DestroyIcon(self.hicon_en);
            let _ = DestroyIcon(self.hicon_ko);
            let _ = DestroyIcon(self.hicon_init);
        }
    }
}

unsafe fn load_icon_from_resource(id: u16) -> windows::core::Result<HICON> {
    use windows::core::PCWSTR;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        LoadImageW, IMAGE_ICON, LR_DEFAULTSIZE, LR_SHARED,
    };

    let hinstance = GetModuleHandleW(None)?;
    let hicon = LoadImageW(
        Some(hinstance.into()),
        PCWSTR(id as *const u16),
        IMAGE_ICON,
        0,
        0,
        LR_DEFAULTSIZE | LR_SHARED,
    )?;

    Ok(HICON(hicon.0))
}
