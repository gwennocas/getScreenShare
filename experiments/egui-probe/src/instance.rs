use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{
            CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND, LPARAM, LRESULT, POINT,
            WPARAM,
        },
        System::Threading::CreateMutexW,
        UI::{
            Shell::{
                Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
                NIM_SETVERSION, NIN_SELECT, NOTIFYICONDATAW, NOTIFYICONDATAW_0,
            },
            WindowsAndMessaging::{
                AppendMenuW, CallWindowProcW, CreateIconFromResourceEx, CreatePopupMenu,
                DefWindowProcW, DestroyIcon, DestroyMenu, FindWindowW, GetCursorPos,
                SetForegroundWindow, SetWindowLongPtrW, ShowWindow, TrackPopupMenu, GWLP_WNDPROC,
                IMAGE_FLAGS, MF_STRING, SW_HIDE, SW_RESTORE, TPM_BOTTOMALIGN, TPM_RIGHTALIGN,
                WM_CLOSE, WM_COMMAND, WM_CONTEXTMENU, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_RBUTTONUP,
                WM_USER, WNDPROC,
            },
        },
    },
};

const TRAY_MESSAGE: u32 = WM_USER + 73;
const NIN_KEYSELECT: u32 = WM_USER + 1;
const MENU_OPEN: usize = 1;
const MENU_EXIT: usize = 2;
static OLD_PROC: AtomicIsize = AtomicIsize::new(0);
static ICON: AtomicIsize = AtomicIsize::new(0);
static TRAY_INSTALLED: AtomicBool = AtomicBool::new(false);
static EXITING: AtomicBool = AtomicBool::new(false);
const ICON_DATA: &[u8] = include_bytes!("../assets/getScreenShare.ico");

pub struct Instance(HANDLE);
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn open(name: PCWSTR) -> Result<Option<Instance>, windows::core::Error> {
    unsafe {
        let handle = CreateMutexW(None, false, name)?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(handle);
            Ok(None)
        } else {
            Ok(Some(Instance(handle)))
        }
    }
}

pub fn acquire() -> Result<Option<Instance>, windows::core::Error> {
    let result = open(w!("Local\\getScreenShare-rust"))?;
    if result.is_none() {
        unsafe {
            if let Ok(window) = FindWindowW(PCWSTR::null(), w!("getScreenShare — Windows")) {
                let _ = ShowWindow(window, SW_RESTORE);
                let _ = SetForegroundWindow(window);
            }
        }
    }
    Ok(result)
}

pub fn install_tray() -> Result<bool, windows::core::Error> {
    if TRAY_INSTALLED.load(Ordering::Acquire) {
        return Ok(true);
    }
    unsafe {
        let window = FindWindowW(PCWSTR::null(), w!("getScreenShare — Windows"))?;
        if window.0.is_null() {
            return Ok(false);
        }
        let Some(size) = ICON_DATA
            .get(14..18)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()) as usize)
        else {
            return Ok(false);
        };
        let Some(offset) = ICON_DATA
            .get(18..22)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()) as usize)
        else {
            return Ok(false);
        };
        let Some(icon_data) = ICON_DATA.get(offset..offset.saturating_add(size)) else {
            return Ok(false);
        };
        let Ok(icon) = CreateIconFromResourceEx(icon_data, true, 0x0003_0000, 0, 0, IMAGE_FLAGS(0))
        else {
            return Ok(false);
        };
        let mut data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: window,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: TRAY_MESSAGE,
            hIcon: icon,
            ..Default::default()
        };
        let tip: Vec<u16> = "getScreenShare".encode_utf16().collect();
        data.szTip[..tip.len()].copy_from_slice(&tip);
        if !Shell_NotifyIconW(NIM_ADD, &data).as_bool() {
            let _ = DestroyIcon(icon);
            return Ok(false);
        }
        data.Anonymous = NOTIFYICONDATAW_0 { uVersion: 4 };
        let _ = Shell_NotifyIconW(NIM_SETVERSION, &data);
        let old = SetWindowLongPtrW(
            window,
            GWLP_WNDPROC,
            window_proc as *const () as usize as isize,
        );
        if old == 0 {
            let _ = Shell_NotifyIconW(NIM_DELETE, &data);
            let _ = DestroyIcon(icon);
            return Ok(false);
        }
        OLD_PROC.store(old, Ordering::Release);
        ICON.store(icon.0 as isize, Ordering::Release);
        TRAY_INSTALLED.store(true, Ordering::Release);
        Ok(true)
    }
}

pub fn remove_tray() {
    if !TRAY_INSTALLED.swap(false, Ordering::AcqRel) {
        return;
    }
    unsafe {
        if let Ok(window) = FindWindowW(PCWSTR::null(), w!("getScreenShare — Windows")) {
            let old = OLD_PROC.swap(0, Ordering::AcqRel);
            if old != 0 {
                let _ = SetWindowLongPtrW(window, GWLP_WNDPROC, old);
            }
            let data = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: window,
                uID: 1,
                ..Default::default()
            };
            let _ = Shell_NotifyIconW(NIM_DELETE, &data);
        }
        let icon = ICON.swap(0, Ordering::AcqRel);
        if icon != 0 {
            let _ = DestroyIcon(windows::Win32::UI::WindowsAndMessaging::HICON(
                icon as *mut _,
            ));
        }
    }
}

pub fn allow_exit() {
    EXITING.store(true, Ordering::Release);
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_CLOSE && !EXITING.load(Ordering::Acquire) {
        let _ = ShowWindow(hwnd, SW_HIDE);
        return LRESULT(0);
    }
    if message == TRAY_MESSAGE {
        match tray_event(lparam) {
            NIN_SELECT | NIN_KEYSELECT | WM_LBUTTONUP | WM_LBUTTONDBLCLK => restore(hwnd),
            WM_CONTEXTMENU | WM_RBUTTONUP => show_menu(hwnd),
            _ => {}
        }
        return LRESULT(0);
    }
    if message == WM_COMMAND {
        match wparam.0 & 0xffff {
            MENU_OPEN => restore(hwnd),
            MENU_EXIT => {
                EXITING.store(true, Ordering::Release);
                let old = OLD_PROC.load(Ordering::Acquire);
                if old != 0 {
                    let proc: WNDPROC = Some(std::mem::transmute(old));
                    return CallWindowProcW(proc, hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
            _ => {}
        }
    }
    let old = OLD_PROC.load(Ordering::Acquire);
    if old != 0 {
        let proc: WNDPROC = Some(std::mem::transmute(old));
        CallWindowProcW(proc, hwnd, message, wparam, lparam)
    } else {
        DefWindowProcW(hwnd, message, wparam, lparam)
    }
}

fn tray_event(lparam: LPARAM) -> u32 {
    (lparam.0 as u16).into()
}

unsafe fn restore(hwnd: HWND) {
    let _ = ShowWindow(hwnd, SW_RESTORE);
    let _ = SetForegroundWindow(hwnd);
}

unsafe fn show_menu(hwnd: HWND) {
    let Ok(menu) = CreatePopupMenu() else {
        return;
    };
    let _ = AppendMenuW(menu, MF_STRING, MENU_OPEN, w!("Abrir getScreenShare"));
    let _ = AppendMenuW(menu, MF_STRING, MENU_EXIT, w!("Sair"));
    let _ = SetForegroundWindow(hwnd);
    let mut point = POINT::default();
    if GetCursorPos(&mut point).is_ok() {
        let _ = TrackPopupMenu(
            menu,
            TPM_RIGHTALIGN | TPM_BOTTOMALIGN,
            point.x,
            point.y,
            None,
            hwnd,
            None,
        );
    }
    let _ = DestroyMenu(menu);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_four_tray_events_ignore_the_icon_id_word() {
        assert_eq!(tray_event(LPARAM(0x0001_0202)), WM_LBUTTONUP);
        assert_eq!(tray_event(LPARAM(0x0001_0205)), WM_RBUTTONUP);
        assert_eq!(tray_event(LPARAM(0x0001_0400)), NIN_SELECT);
    }

    #[test]
    fn only_one_instance_owns_the_named_mutex() {
        let name: Vec<u16> = format!("Local\\gss-test-{}\0", uuid::Uuid::new_v4())
            .encode_utf16()
            .collect();
        let first = open(PCWSTR(name.as_ptr())).unwrap().unwrap();
        assert!(open(PCWSTR(name.as_ptr())).unwrap().is_none());
        drop(first);
        assert!(open(PCWSTR(name.as_ptr())).unwrap().is_some());
    }
}
