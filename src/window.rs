//! UI-thread window ownership and checked userdata attachment.
use windows::core::{Error, Result};
use windows::Win32::Foundation::{GetLastError, SetLastError, HWND, WIN32_ERROR};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyWindow, IsWindow, SetWindowLongPtrW, GWLP_USERDATA,
};

pub struct OwnedWindow(pub HWND);

impl OwnedWindow {
    /// The caller keeps state alive until the window has been destroyed.
    pub unsafe fn attach<T>(&self, state: &T) -> Result<()> {
        SetLastError(WIN32_ERROR(0));
        let previous = SetWindowLongPtrW(self.0, GWLP_USERDATA, state as *const T as isize);
        if previous == 0 && GetLastError() != WIN32_ERROR(0) {
            return Err(Error::from_thread());
        }
        Ok(())
    }
}

impl Drop for OwnedWindow {
    fn drop(&mut self) {
        unsafe {
            if IsWindow(Some(self.0)).as_bool() {
                // Destruction can invoke callbacks. Detach before any owner fields
                // are dropped so callbacks cannot reborrow partially dropped state.
                SetWindowLongPtrW(self.0, GWLP_USERDATA, 0);
                let _ = DestroyWindow(self.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::ERROR_INVALID_WINDOW_HANDLE;

    #[test]
    fn userdata_attachment_reports_invalid_window_handles() {
        let window = OwnedWindow(HWND::default());
        let state = 42;
        let error = unsafe { window.attach(&state) }.expect_err("invalid HWND must fail");
        assert_eq!(error.code(), ERROR_INVALID_WINDOW_HANDLE.to_hresult());
    }
}
