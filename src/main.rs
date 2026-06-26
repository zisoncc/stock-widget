#![windows_subsystem = "windows"]
//! Stock Widget - Semi-transparent floating stock/ETF ticker for Windows

mod config;
mod stock;
mod window;

use windows::Win32::Foundation::{HINSTANCE, CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::PCWSTR;

use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, SetTimer,
    TranslateMessage,
};

const TIMER_REFRESH_ID: usize = 1;

fn main() -> windows::core::Result<()> {
    // Single instance check
    unsafe {
        let mutex_name = "StockWidget_SingleInstance_Mutex\0";
        let name_w: Vec<u16> = mutex_name.encode_utf16().collect();
        let h_mutex = CreateMutexW(None, false, PCWSTR(name_w.as_ptr()));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if let Ok(h) = h_mutex {
                let _ = CloseHandle(h);
            }
            eprintln!("Another instance is already running.");
            return Err(windows::core::Error::from_win32());
        }
        // HANDLE is isize - dropping it doesn't close it, the OS auto-cleans on exit
    }

    // Initialize COM
    let _ = unsafe {
        windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
        )
    };

    let hinstance: HINSTANCE = unsafe {
        windows::Win32::System::LibraryLoader::GetModuleHandleW(None)?.into()
    };
    let cfg = config::AppConfig::load();
    let mut win = window::StockWindow::new(hinstance, cfg);

    if let Err(e) = win.create() {
        eprintln!("Failed to create window: {}", e);
        return Err(windows::core::Error::from_win32());
    }

    let hwnd = win.get_hwnd();
    unsafe {
        SetTimer(hwnd, TIMER_REFRESH_ID, 1000, None);
    }

    let mut msg: MSG = unsafe { std::mem::zeroed() };
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
        let _ = unsafe { TranslateMessage(&msg) };
        unsafe { DispatchMessageW(&msg) };
    }

    unsafe { windows::Win32::System::Com::CoUninitialize() };
    Ok(())
}