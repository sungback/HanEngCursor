use crate::ime::ImeMode;
use std::time::{Duration, Instant};
use windows::core::{w, Error, Result};
use windows::Win32::Foundation::{
    GetLastError, SetLastError, HWND, LPARAM, POINT, WIN32_ERROR, WPARAM,
};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW, NOTIFY_ICON_MESSAGE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, PostMessageW, SetForegroundWindow,
    TrackPopupMenu, HICON, HMENU, MF_STRING, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_NONOTIFY,
    TPM_RETURNCMD, WM_COMMAND, WM_NULL, WM_USER,
};

pub const WM_TRAY_CALLBACK: u32 = WM_USER + 1;
pub const IDM_EXIT: u32 = 1001;

pub struct TrayIcon {
    nid: NOTIFYICONDATAW,
    // Loaded with LR_SHARED: owned by Windows, never passed to DestroyIcon.
    hicon_en: HICON,
    hicon_ko: HICON,
    registration: Registration,
}

impl TrayIcon {
    pub fn new(hwnd: HWND) -> Result<Self> {
        unsafe {
            let hicon_ko = load_icon_from_resource(201)?;
            let hicon_en = load_icon_from_resource(202)?;
            let mut nid = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: 1,
                uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
                uCallbackMessage: WM_TRAY_CALLBACK,
                hIcon: load_icon_from_resource(203)?,
                ..Default::default()
            };
            let tip: Vec<u16> = "HanEngCursor — 한/영 표시기".encode_utf16().collect();
            let len = tip.len().min(nid.szTip.len() - 1);
            nid.szTip[..len].copy_from_slice(&tip[..len]);
            let mut tray = Self {
                nid,
                hicon_en,
                hicon_ko,
                registration: Registration::new(Instant::now()),
            };
            tray.sync(Instant::now());
            Ok(tray)
        }
    }

    pub fn update_icon(&mut self, mode: ImeMode) {
        self.nid.hIcon = match mode {
            ImeMode::English => self.hicon_en,
            ImeMode::Korean => self.hicon_ko,
        };
        self.sync(Instant::now());
    }

    pub fn taskbar_created(&mut self) {
        self.registration = Registration::new(Instant::now());
    }

    fn sync(&mut self, now: Instant) {
        let nid = &self.nid;
        self.registration.sync(nid.hIcon, now, |message| unsafe {
            Shell_NotifyIconW(message, nid).as_bool()
        });
    }
}

// Shell availability is transient. Retry at most once a second, and periodically
// probe an unchanged icon in case a TaskbarCreated broadcast was missed.
struct Registration {
    published: Option<HICON>,
    retry_at: Instant,
    probe_at: Instant,
}

impl Registration {
    fn new(now: Instant) -> Self {
        Self {
            published: None,
            retry_at: now,
            probe_at: now,
        }
    }

    fn sync(
        &mut self,
        icon: HICON,
        now: Instant,
        mut notify: impl FnMut(NOTIFY_ICON_MESSAGE) -> bool,
    ) {
        if now < self.retry_at || (self.published == Some(icon) && now < self.probe_at) {
            return;
        }
        let (first, fallback) = if self.published.is_some() {
            (NIM_MODIFY, NIM_ADD)
        } else {
            (NIM_ADD, NIM_MODIFY)
        };
        if notify(first) || notify(fallback) {
            self.published = Some(icon);
            self.retry_at = now;
            self.probe_at = now + Duration::from_secs(5);
        } else {
            self.published = None;
            self.retry_at = now + Duration::from_secs(1);
        }
    }
}
impl Drop for TrayIcon {
    fn drop(&mut self) {
        // Best-effort cleanup: the shell might already have removed the icon.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.nid);
        }
    }
}

struct PopupMenu(HMENU);

impl Drop for PopupMenu {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyMenu(self.0);
        }
    }
}

/// Receives a value instead of borrowing app state across a nested message loop.
pub fn show_context_menu(hwnd: HWND) -> Result<()> {
    unsafe {
        let menu = PopupMenu(CreatePopupMenu()?);
        AppendMenuW(menu.0, MF_STRING, IDM_EXIT as usize, w!("종료(&X)"))?;
        let mut pt = POINT::default();
        GetCursorPos(&mut pt)?;
        // Foreground activation may be denied by Windows; the menu still works.
        let _ = SetForegroundWindow(hwnd);
        SetLastError(WIN32_ERROR(0));
        let command = TrackPopupMenu(
            menu.0,
            TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RETURNCMD | TPM_NONOTIFY,
            pt.x,
            pt.y,
            None,
            hwnd,
            None,
        )
        .0 as u32;
        if command == 0 && GetLastError() != WIN32_ERROR(0) {
            return Err(Error::from_thread());
        }
        // Restore normal dismissal behavior after TrackPopupMenu.
        PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0))?;
        if command != 0 {
            PostMessageW(Some(hwnd), WM_COMMAND, WPARAM(command as usize), LPARAM(0))?;
        }
        Ok(())
    }
}

unsafe fn load_icon_from_resource(id: u16) -> Result<HICON> {
    use windows::core::PCWSTR;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        LoadImageW, IMAGE_ICON, LR_DEFAULTSIZE, LR_SHARED,
    };
    let hinstance = GetModuleHandleW(None)?;
    let icon = LoadImageW(
        Some(hinstance.into()),
        PCWSTR(id as *const u16),
        IMAGE_ICON,
        0,
        0,
        LR_DEFAULTSIZE | LR_SHARED,
    )?;
    Ok(HICON(icon.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_shell_is_throttled_and_recovers_with_latest_mode() {
        let now = Instant::now();
        let mut tokens = [0u8; 2];
        let english = HICON(tokens.as_mut_ptr().cast());
        let korean = HICON(tokens.as_mut_ptr().wrapping_add(1).cast());
        let mut state = Registration::new(now);
        let mut calls = Vec::new();
        state.sync(english, now, |message| {
            calls.push(message);
            false
        });
        assert_eq!(calls, [NIM_ADD, NIM_MODIFY]);
        state.sync(korean, now + Duration::from_millis(50), |_| {
            panic!("retry too soon")
        });
        state.sync(korean, now + Duration::from_secs(1), |message| {
            assert_eq!(message, NIM_ADD);
            true
        });
        assert_eq!(state.published, Some(korean));
    }

    #[test]
    fn unchanged_icon_is_probed_and_readded_if_registration_disappears() {
        let now = Instant::now();
        let icon = HICON(std::ptr::dangling_mut());
        let mut state = Registration::new(now);
        state.sync(icon, now, |_| true);
        state.sync(icon, now + Duration::from_secs(4), |_| {
            panic!("probe too soon")
        });
        let mut calls = Vec::new();
        state.sync(icon, now + Duration::from_secs(5), |message| {
            calls.push(message);
            message == NIM_ADD
        });
        assert_eq!(calls, [NIM_MODIFY, NIM_ADD]);
        assert_eq!(state.published, Some(icon));
    }

    #[test]
    fn repeated_taskbar_broadcast_can_update_an_existing_registration() {
        let now = Instant::now();
        let icon = HICON(std::ptr::dangling_mut());
        let mut state = Registration::new(now);
        let mut calls = Vec::new();
        state.sync(icon, now, |message| {
            calls.push(message);
            message == NIM_MODIFY
        });
        assert_eq!(calls, [NIM_ADD, NIM_MODIFY]);
        assert_eq!(state.published, Some(icon));
    }
}
