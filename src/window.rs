//! Main window implementation using Win32 API
//! Features: layered transparent window, drag-on-left-click, right-click context menu

use crate::config::AppConfig;
use crate::stock::{self, StockInfo};
use std::ffi::c_void;
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{SetCapture, ReleaseCapture};
use windows::Win32::UI::Controls::InitCommonControls;
use windows::Win32::System::SystemServices::MK_LBUTTON;

const TIMER_REFRESH_ID: usize = 1;

fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((r as u32) | (g as u32) << 8 | (b as u32) << 16)
}

const IDM_ADD_STOCK: usize = 1000;
const IDM_REFRESH_NOW: usize = 1002;
const IDM_EXIT: usize = 1003;
const IDM_ADJUST_OPACITY: usize = 1004;
const IDM_REMOVE_STOCK_BASE: usize = 2000;
const IDC_INPUT_EDIT: u32 = 2001;
const IDC_INPUT_OK: u32 = IDOK.0 as u32;
const IDC_INPUT_CANCEL: u32 = IDCANCEL.0 as u32;
const INPUT_DLG_WIDTH: i32 = 320;
const INPUT_DLG_HEIGHT: i32 = 150;

// Trackbar control constants
const TBM_SETRANGEMIN: u32 = 0x0407;
const TBM_SETRANGEMAX: u32 = 0x0408;
const TBM_SETPOS: u32 = 0x0405;
const TBM_GETPOS: u32 = 0x0400;

// Opacity dialog control IDs
const IDC_OPACITY_TRACKBAR: u32 = 4001;
const IDC_OPACITY_OK: u32 = 4002;
const IDC_OPACITY_CANCEL: u32 = 4003;
const IDC_OPACITY_TEXT: u32 = 4004;
const OPACITY_DLG_WIDTH: i32 = 340;
const OPACITY_DLG_HEIGHT: i32 = 140;
const MIN_OPACITY: u8 = 26;

fn clamp_opacity(opacity: u8) -> u8 {
    opacity.max(MIN_OPACITY)
}

struct InputDialogParams {
    prompt: String,
    default_val: String,
    result: std::sync::Arc<std::sync::Mutex<String>>,
    done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    edit_hwnd: HWND,
}

struct OpacityDlgData {
    result: std::cell::Cell<Option<u8>>,
    trackbar_hwnd: HWND,
    text_hwnd: HWND,
    stock_window: *mut StockWindow,
    original_opacity: u8,
}

const MARGIN_X: i32 = 14;
const MARGIN_Y: i32 = 6;
const ROW_HEIGHT: i32 = 26;
const MIN_WIDTH: i32 = 280;

pub struct StockWindow {
    hwnd: HWND,
    hinstance: HINSTANCE,
    config: Arc<Mutex<AppConfig>>,
    stocks: Vec<StockInfo>,
    is_dragging: Cell<bool>,
    is_resizing: Cell<bool>,
    drag_start: Cell<(i32, i32)>,
    resize_start_x: Cell<i32>,
    resize_start_width: Cell<i32>,
    width: Cell<i32>,
    height: Cell<i32>,
    input_dialog_registered: AtomicBool,
    opacity: Cell<u8>,
    opacity_dialog_registered: AtomicBool,
}

impl StockWindow {
    pub fn new(hinstance: HINSTANCE, mut config: AppConfig) -> Self {
        let opacity_val = clamp_opacity(config.opacity);
        config.opacity = opacity_val;
        let window_width = config.window.width as i32;
        Self {
            hwnd: HWND::default(),
            hinstance,
            config: Arc::new(Mutex::new(config)),
            stocks: Vec::new(),
            is_dragging: Cell::new(false),
            is_resizing: Cell::new(false),
            drag_start: Cell::new((0, 0)),
            resize_start_x: Cell::new(0),
            resize_start_width: Cell::new(window_width),
            width: Cell::new(window_width),
            height: Cell::new(100),
            input_dialog_registered: AtomicBool::new(false),
            opacity: Cell::new(opacity_val),
            opacity_dialog_registered: AtomicBool::new(false),
        }
    }

    pub fn get_hwnd(&self) -> HWND {
        self.hwnd
    }

    fn get_ptr(hwnd: HWND) -> Option<*mut StockWindow> {
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
        if ptr != 0 {
            Some(ptr as *mut StockWindow)
        } else {
            None
        }
    }

    fn refresh(&mut self) {
        let symbols: Vec<String>;
        {
            let cfg = self.config.lock().unwrap();
            symbols = cfg.symbols.clone();
        }
        self.stocks = stock::fetch_stocks(&symbols);
        {
            let mut cfg = self.config.lock().unwrap();
            for s in &self.stocks {
                if !s.name.is_empty() && !cfg.names.contains_key(&s.symbol) {
                    cfg.names.insert(s.symbol.clone(), s.name.clone());
                }
            }
        }
        let count = self.stocks.len() as i32;
        let h = if count == 0 { 40 } else { MARGIN_Y * 2 + count * ROW_HEIGHT };
        self.height.set(h as i32);
        // Keep current width (preserved from user resize / config)

        unsafe {
            let mut r = RECT::default();
            let _ = GetWindowRect(self.hwnd, &mut r);
            let cx = r.right - r.left;
            let cy = h as i32;
            let _ = SetWindowPos(
                self.hwnd, None,
                r.left, r.top, cx, cy,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        unsafe { let _ = InvalidateRect(self.hwnd, None, true); };
    }

    fn handle_paint(&self, hwnd: HWND) {
        unsafe {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);

            let mut rect: RECT = std::mem::zeroed();
            let _ = GetClientRect(hwnd, &mut rect);
            let w = rect.right - rect.left;
            let h = rect.bottom - rect.top;

            let mem_dc = CreateCompatibleDC(hdc);
            let hbm = CreateCompatibleBitmap(hdc, w, h);
            if hbm.is_invalid() {
                let _ = DeleteDC(mem_dc);
                let _ = EndPaint(hwnd, &ps);
                return;
            }
            let hbm_old = SelectObject(mem_dc, hbm);

            let bg_brush = CreateSolidBrush(rgb(30, 30, 30));
            let fr = rect;
            FillRect(mem_dc, &fr, bg_brush);
            let _ = DeleteObject(HGDIOBJ(bg_brush.0 as *mut std::ffi::c_void));

            let mut y = MARGIN_Y;
            for si in &self.stocks {
                let fh = -(ROW_HEIGHT * 3 / 5);
                let hf = CreateFontW(
                    fh, 0, 0, 0, 400i32,
                    0, 0, 0,
                    1u32, 0u32,
                    0u32, 6u32,
                    2u32, None,
                );
                let hf_old = SelectObject(mem_dc, hf);
                let text_color = if si.is_up {
                    rgb(230, 70, 70)
                } else {
                    rgb(50, 180, 90)
                };
                SetTextColor(mem_dc, text_color);
                SetBkMode(mem_dc, TRANSPARENT);

                let line = format!(
                    "{} {} {:.3} {:+.3} ({:+.2}%)",
                    si.symbol, si.name, si.price, si.change, si.change_percent
                );
                let enc: Vec<u16> = line.encode_utf16().chain([0]).collect();
                let _ = TextOutW(mem_dc, MARGIN_X, y, &enc);

                SelectObject(mem_dc, hf_old);
                let _ = DeleteObject(HGDIOBJ(hf.0 as *mut std::ffi::c_void));
                y += ROW_HEIGHT;
            }

            let _ = BitBlt(hdc, 0, 0, w, h, mem_dc, 0, 0, SRCCOPY);
            SelectObject(mem_dc, hbm_old);
            let _ = DeleteObject(HGDIOBJ(hbm.0 as *mut std::ffi::c_void));
            let _ = DeleteDC(mem_dc);
            let _ = EndPaint(hwnd, &ps);
        }
    }

    fn handle_timer(&mut self, wparam: WPARAM) {
        if wparam.0 as usize == TIMER_REFRESH_ID {
            self.refresh();
        }
    }

    fn handle_rbutton_up(&self, hwnd: HWND, point: POINT) {
        let menu = match unsafe { CreatePopupMenu() } {
            Ok(m) => m,
            Err(_) => return,
        };

        let t1 = "添加股票/ETF...  (Add)";
        let w1: Vec<u16> = t1.encode_utf16().chain([0]).collect();
        unsafe { AppendMenuW(menu, MF_STRING, IDM_ADD_STOCK, PCWSTR(w1.as_ptr())).ok() };

        let t2 = "立即刷新  (Refresh)";
        let w2: Vec<u16> = t2.encode_utf16().chain([0]).collect();
        unsafe { AppendMenuW(menu, MF_STRING, IDM_REFRESH_NOW, PCWSTR(w2.as_ptr())).ok() };

        {
            let cfg = self.config.lock().unwrap();
            if !cfg.symbols.is_empty() {
                unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, None).ok() };
                for (i, sym) in cfg.symbols.iter().enumerate() {
                    let lbl = format!("删除 {}  ({})", sym, i + 1);
                    let ww: Vec<u16> = lbl.encode_utf16().chain([0]).collect();
                    unsafe { AppendMenuW(menu, MF_STRING, IDM_REMOVE_STOCK_BASE + i, PCWSTR(ww.as_ptr())).ok() };
                }
            }
        }

        unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, None).ok() };
        let t_opacity = "透明度调节  (Opacity)";
        let w_opacity: Vec<u16> = t_opacity.encode_utf16().chain([0]).collect();
        unsafe { AppendMenuW(menu, MF_STRING, IDM_ADJUST_OPACITY, PCWSTR(w_opacity.as_ptr())).ok() };

        unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, None).ok() };
        let t3 = "退出  (Exit)";
        let w3: Vec<u16> = t3.encode_utf16().chain([0]).collect();
        unsafe { AppendMenuW(menu, MF_STRING, IDM_EXIT, PCWSTR(w3.as_ptr())).ok() };

        unsafe { let _ = SetForegroundWindow(hwnd).ok(); };
        let sel = unsafe { TrackPopupMenuEx(menu, TPM_RETURNCMD.0, point.x, point.y, hwnd, None) };
        if sel.0 != 0 {
            unsafe { let _ = PostMessageW(hwnd, WM_COMMAND, WPARAM(sel.0 as usize), LPARAM(0)); };
        }
        unsafe { DestroyMenu(menu).ok() };
    }

    fn show_input_dialog(&self, title: &str, prompt: &str, default_val: &str) -> String {
        if !self.input_dialog_registered.load(Ordering::Acquire) {
            let dlg_class_name = "InputDialogClass";
            let dlg_class_name_w: Vec<u16> = dlg_class_name.encode_utf16().chain([0]).collect();
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(Self::input_dialog_proc),
                cbClsExtra: 0,
                cbWndExtra: std::mem::size_of::<isize>() as i32,
                hInstance: self.hinstance,
                hIcon: unsafe { LoadIconW(self.hinstance, PCWSTR(101usize as *const u16)) }.unwrap_or(HICON::default()),
                hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or(HCURSOR::default()),
                hbrBackground: HBRUSH::default(),
                lpszMenuName: PCWSTR::null(),
                lpszClassName: PCWSTR(dlg_class_name_w.as_ptr()),
                hIconSm: unsafe { LoadIconW(self.hinstance, PCWSTR(101usize as *const u16)) }.unwrap_or(HICON::default()),
            };
            let atom = unsafe { RegisterClassExW(&wc) };
            if atom != 0 {
                self.input_dialog_registered.store(true, Ordering::Release);
            }
        }

        let result = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let done = std::sync::Arc::new(AtomicBool::new(false));
        let r2 = result.clone();
        let d2 = done.clone();

        unsafe {
            let params = Box::into_raw(Box::new(InputDialogParams {
                prompt: prompt.to_string(),
                default_val: default_val.to_string(),
                result: result,
                done: done,
                edit_hwnd: HWND(std::ptr::null_mut()),
            }));

            let mut parent_rect: RECT = std::mem::zeroed();
            let _ = GetWindowRect(self.hwnd, &mut parent_rect);
            let dlg_x = parent_rect.left + (parent_rect.right - parent_rect.left - INPUT_DLG_WIDTH) / 2;
            let dlg_y = parent_rect.top + (parent_rect.bottom - parent_rect.top - INPUT_DLG_HEIGHT) / 2;

            let tw: Vec<u16> = title.encode_utf16().chain([0]).collect();
            let dc: Vec<u16> = "InputDialogClass".encode_utf16().chain([0]).collect();

            let dlg_hwnd = CreateWindowExW(
                WINDOW_EX_STYLE((WS_EX_DLGMODALFRAME.0) as u32),
                PCWSTR(dc.as_ptr()),
                PCWSTR(tw.as_ptr()),
                WINDOW_STYLE(WS_POPUP.0 | WS_CAPTION.0 | WS_SYSMENU.0),
                dlg_x.max(0), dlg_y.max(0), INPUT_DLG_WIDTH, INPUT_DLG_HEIGHT,
                self.hwnd,
                HMENU(std::ptr::null_mut()),
                self.hinstance,
                Some(params as *mut c_void),
            );

            if dlg_hwnd.is_err() {
                let _ = Box::from_raw(params);
                return String::new();
            }

            let dlg_hwnd = dlg_hwnd.unwrap();
            let _ = ShowWindow(dlg_hwnd, SW_SHOW);
            let _ = SetForegroundWindow(dlg_hwnd).ok();
            let _ = UpdateWindow(dlg_hwnd);

            let mut msg: MSG = std::mem::zeroed();
            while !d2.load(Ordering::Acquire) {
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    if msg.message == WM_QUIT {
                        PostQuitMessage(msg.wParam.0 as i32);
                        let _ = Box::from_raw(params);
                        return String::new();
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                let _ = WaitMessage();
            }

            let _ = SetForegroundWindow(self.hwnd).ok();
            let _ = Box::from_raw(params);
        }

        { let v = r2.lock().unwrap().clone(); v }
    }

    extern "system" fn input_dialog_proc(
        hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_CREATE => {
                let cs = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
                let params_ptr = cs.lpCreateParams as *mut InputDialogParams;
                if params_ptr.is_null() {
                    return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
                }
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, params_ptr as isize) };

                // Create prompt static text
                unsafe {
                    let prompt = &(*params_ptr).prompt;
                    let pw: Vec<u16> = prompt.encode_utf16().chain([0]).collect();
                    let static_hwnd = CreateWindowExW(
                        WINDOW_EX_STYLE(0),
                        PCWSTR("Static\0".encode_utf16().chain([0]).collect::<Vec<u16>>().as_ptr()),
                        PCWSTR(pw.as_ptr()),
                        WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0),
                        12, 12, INPUT_DLG_WIDTH - 24, 20,
                        hwnd,
                        None,
                        cs.hInstance,
                        None,
                    );
                    let _ = static_hwnd;

                    // Create edit control
                    let default_val = &(*params_ptr).default_val;
                    let dv: Vec<u16> = default_val.encode_utf16().chain([0]).collect();
                    let edit_hwnd = CreateWindowExW(
                        WINDOW_EX_STYLE(WS_EX_CLIENTEDGE.0 as u32),
                        PCWSTR("Edit\0".encode_utf16().chain([0]).collect::<Vec<u16>>().as_ptr()),
                        PCWSTR(dv.as_ptr()),
                        WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | 0x0080u32),
                        12, 36, INPUT_DLG_WIDTH - 24, 24,
                        hwnd,
                        HMENU(IDC_INPUT_EDIT as usize as *mut c_void),
                        cs.hInstance,
                        None,
                    );
                    if let Ok(edit) = &edit_hwnd {
                        (*params_ptr).edit_hwnd = *edit;
                    }

                    // Create OK button
                    let ok_text = "OK\0";
                    CreateWindowExW(
                        WINDOW_EX_STYLE(0),
                        PCWSTR("Button\0".encode_utf16().chain([0]).collect::<Vec<u16>>().as_ptr()),
                        PCWSTR(ok_text.encode_utf16().chain([0]).collect::<Vec<u16>>().as_ptr()),
                        WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | 0x0001u32),
                        INPUT_DLG_WIDTH - 180, 80, 80, 28,
                        hwnd,
                        HMENU(IDC_INPUT_OK as usize as *mut c_void),
                        cs.hInstance,
                        None,
                    ).ok();

                    // Create Cancel button
                    let cancel_text = "Cancel\0";
                    CreateWindowExW(
                        WINDOW_EX_STYLE(0),
                        PCWSTR("Button\0".encode_utf16().chain([0]).collect::<Vec<u16>>().as_ptr()),
                        PCWSTR(cancel_text.encode_utf16().chain([0]).collect::<Vec<u16>>().as_ptr()),
                        WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | 0x0000u32),
                        INPUT_DLG_WIDTH - 90, 80, 80, 28,
                        hwnd,
                        HMENU(IDC_INPUT_CANCEL as usize as *mut c_void),
                        cs.hInstance,
                        None,
                    ).ok();
                }

                LRESULT(0)
            }
            WM_SETFOCUS => {
                unsafe {
                    let params_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut InputDialogParams;
                    if !params_ptr.is_null() && !(*params_ptr).edit_hwnd.is_invalid() {
                        SendMessageW((*params_ptr).edit_hwnd, WM_SETFOCUS, WPARAM(0), LPARAM(0));
                    }
                }
                LRESULT(0)
            }
            WM_KEYDOWN => {
                unsafe {
                    let params_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut InputDialogParams;
                    if !params_ptr.is_null() && !(*params_ptr).edit_hwnd.is_invalid() {
                        SendMessageW((*params_ptr).edit_hwnd, msg, wparam, lparam);
                    }
                }
                unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
            }
            WM_CHAR => {
                unsafe {
                    let params_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut InputDialogParams;
                    if !params_ptr.is_null() && !(*params_ptr).edit_hwnd.is_invalid() {
                        SendMessageW((*params_ptr).edit_hwnd, msg, wparam, lparam);
                    }
                }
                LRESULT(0)
            }
            WM_COMMAND => {
                let id = (wparam.0 as u32) & 0xFFFF;
                match id {
                    IDC_INPUT_OK => {
                        unsafe {
                            let edit_hwnd = GetDlgItem(hwnd, IDC_INPUT_EDIT as i32).unwrap_or_else(|_| HWND(std::ptr::null_mut()));
                            if !edit_hwnd.is_invalid() {
                                let len = SendMessageW(edit_hwnd, WM_GETTEXTLENGTH, WPARAM(0), LPARAM(0)).0;
                                if len > 0 {
                                    let mut buf = vec![0u16; len as usize + 1];
                                    SendMessageW(edit_hwnd, WM_GETTEXT, WPARAM(buf.len()), LPARAM(buf.as_mut_ptr() as isize));
                                    let text = String::from_utf16_lossy(&buf[..len as usize]);
                                    let params_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut InputDialogParams;
                                    if !params_ptr.is_null() {
                                        *(*params_ptr).result.lock().unwrap() = text;
                                    }
                                }
                            }
                            let params_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut InputDialogParams;
                            if !params_ptr.is_null() {
                                (*params_ptr).done.store(true, Ordering::Release);
                            }
                            let _ = DestroyWindow(hwnd);
                        }
                    }
                    IDC_INPUT_CANCEL => {
                        unsafe {
                            let params_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut InputDialogParams;
                            if !params_ptr.is_null() {
                                (*params_ptr).done.store(true, Ordering::Release);
                            }
                            let _ = DestroyWindow(hwnd);
                        }
                    }
                    _ => { return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }; }
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                unsafe {
                    let params_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut InputDialogParams;
                    if !params_ptr.is_null() {
                        (*params_ptr).done.store(true, Ordering::Release);
                    }
                    let _ = DestroyWindow(hwnd);
                }
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    fn show_opacity_dialog(&self, current_opacity: u8) -> Option<u8> {
        let hwnd_owner = self.hwnd;
        let current_opacity = clamp_opacity(current_opacity);

        // Register dialog window class (once)
        let class_name = "OpacityDialogClass";
        if !self.opacity_dialog_registered.swap(true, std::sync::atomic::Ordering::SeqCst) {
            let class_name_w: Vec<u16> = class_name.encode_utf16().chain([0]).collect();
            unsafe { InitCommonControls(); }
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(Self::opacity_dlg_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: self.hinstance,
                hIcon: HICON::default(),
                hCursor: unsafe { LoadCursorW(None, IDC_ARROW).unwrap_or(HCURSOR::default()) },
                hbrBackground: unsafe { GetSysColorBrush(COLOR_BTNFACE) },
                lpszMenuName: PCWSTR::null(),
                lpszClassName: PCWSTR(class_name_w.as_ptr()),
                hIconSm: HICON::default(),
            };
            if unsafe { RegisterClassExW(&wc) } == 0 {
                self.opacity_dialog_registered.store(false, std::sync::atomic::Ordering::SeqCst);
                return None;
            }
        }

        // Create shared state
        let state = Box::into_raw(Box::new(OpacityDlgData {
            result: std::cell::Cell::new(None),
            trackbar_hwnd: HWND::default(),
            text_hwnd: HWND::default(),
            stock_window: self as *const StockWindow as *mut StockWindow,
            original_opacity: current_opacity,
        }));

        // Create dialog window
        let title = "透明度调节";
        let tw: Vec<u16> = title.encode_utf16().chain([0]).collect();
        let cn_w: Vec<u16> = class_name.encode_utf16().chain([0]).collect();

        let dlg_hwnd = match unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR(cn_w.as_ptr()),
                PCWSTR(tw.as_ptr()),
                WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
                0, 0, OPACITY_DLG_WIDTH, OPACITY_DLG_HEIGHT,
                hwnd_owner, None, self.hinstance, Some(std::mem::transmute(state)),
            )
        } {
            Ok(h) => h,
            Err(_) => {
                self.opacity_dialog_registered.store(false, std::sync::atomic::Ordering::SeqCst);
                unsafe { drop(Box::from_raw(state)) };
                return None;
            }
        };

        // Also store state in GWLP_USERDATA for dialog proc access
        unsafe { SetWindowLongPtrW(dlg_hwnd, GWLP_USERDATA, state as isize) };

        // Center dialog on owner
        unsafe {
            let mut owner_rect = RECT::default();
            let mut dlg_rect = RECT::default();
            let _ = GetWindowRect(hwnd_owner, &mut owner_rect);
            let _ = GetWindowRect(dlg_hwnd, &mut dlg_rect);
            let dlg_w = dlg_rect.right - dlg_rect.left;
            let dlg_h = dlg_rect.bottom - dlg_rect.top;
            let cx = (owner_rect.left + owner_rect.right) / 2 - dlg_w / 2;
            let cy = (owner_rect.top + owner_rect.bottom) / 2 - dlg_h / 2;
            let _ = SetWindowPos(dlg_hwnd, None, cx.max(0), cy.max(0), dlg_w, dlg_h, SWP_NOSIZE | SWP_NOZORDER);
        }

        // Create trackbar
        let trackbar_hwnd = match unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                windows::core::w!("msctls_trackbar32"),
                PCWSTR::null(),
                WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0),
                20, 30, 280, 30,
                dlg_hwnd, HMENU(IDC_OPACITY_TRACKBAR as _), self.hinstance, None,
            )
        } {
            Ok(h) => h,
            Err(_) => {
                unsafe { let _ = DestroyWindow(dlg_hwnd); };
                unsafe { drop(Box::from_raw(state)) };
                return None;
            }
        };

        // Set trackbar range (10% to fully opaque) and initial position.
        unsafe {
            SendMessageW(trackbar_hwnd, TBM_SETRANGEMIN, WPARAM(1), LPARAM(MIN_OPACITY as isize));
            SendMessageW(trackbar_hwnd, TBM_SETRANGEMAX, WPARAM(1), LPARAM(255));
            SendMessageW(trackbar_hwnd, TBM_SETPOS, WPARAM(1), LPARAM(current_opacity as isize));
        }
        unsafe { (*state).trackbar_hwnd = trackbar_hwnd };

        // Create percentage label
        let pct = (current_opacity as u32 * 100 + 127) / 255;
        let pct_text = format!("透明度: {}%", pct);
        let pct_w: Vec<u16> = pct_text.encode_utf16().chain([0]).collect();
        let text_hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                windows::core::w!("STATIC"),
                PCWSTR(pct_w.as_ptr()),
                WS_CHILD | WS_VISIBLE,
                20, 10, 200, 20,
                dlg_hwnd, HMENU(IDC_OPACITY_TEXT as _), self.hinstance, None,
            )
        }.unwrap_or(HWND::default());
        unsafe { (*state).text_hwnd = text_hwnd };

        // Create OK button
        unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                windows::core::w!("BUTTON"),
                windows::core::w!("确定"),
                WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | 0x0001),
                80, 70, 80, 28,
                dlg_hwnd, HMENU(IDC_OPACITY_OK as _), self.hinstance, None,
            ).ok();
        }

        // Create Cancel button
        unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                windows::core::w!("BUTTON"),
                windows::core::w!("取消"),
                WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0),
                180, 70, 80, 28,
                dlg_hwnd, HMENU(IDC_OPACITY_CANCEL as _), self.hinstance, None,
            ).ok();
        }

        // Run modal loop


        let mut msg = MSG::default();
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
            if !unsafe { IsDialogMessageW(dlg_hwnd, &mut msg) }.as_bool() {
                unsafe { let _ = TranslateMessage(&msg); };
                unsafe { DispatchMessageW(&msg) };
            }
            if !unsafe { IsWindow(dlg_hwnd) }.as_bool() {
                break;
            }
        }



        // Read result and clean up
        let result = unsafe { (*state).result.get() };
        unsafe { drop(Box::from_raw(state)) };
        result
    }

    unsafe extern "system" fn opacity_dlg_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_HSCROLL => {
                let data = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut OpacityDlgData;
                if !data.is_null() {
                    let pos = SendMessageW((*data).trackbar_hwnd, TBM_GETPOS, WPARAM(0), LPARAM(0));
                    let opacity = clamp_opacity(pos.0 as u8);
                    let pct = (opacity as u32 * 100 + 127) / 255;
                    let text = format!("透明度: {}%", pct);
                    let w: Vec<u16> = text.encode_utf16().chain([0]).collect();
                    let _ = SetWindowTextW((*data).text_hwnd, PCWSTR(w.as_ptr()));

                    // Preview opacity on main window
                    let win = &*(*data).stock_window;
                    let _ = SetLayeredWindowAttributes(win.get_hwnd(), COLORREF(0), opacity, LWA_ALPHA);
                }
                LRESULT(0)
            }
            WM_COMMAND => {
                let id = (wparam.0 as u32) & 0xFFFF;
                if id == IDC_OPACITY_OK || id == IDC_OPACITY_CANCEL {
                    let data = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut OpacityDlgData;
                    if !data.is_null() {
                        if id == IDC_OPACITY_OK {
                            let pos = SendMessageW((*data).trackbar_hwnd, TBM_GETPOS, WPARAM(0), LPARAM(0));
                            (*data).result.set(Some(clamp_opacity(pos.0 as u8)));
                        } else {
                            // Cancel: restore original opacity on main window
                            let win = &*(*data).stock_window;
                            let _ = SetLayeredWindowAttributes(win.get_hwnd(), COLORREF(0), (*data).original_opacity, LWA_ALPHA);
                        }
                    }
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_CLOSE => {
                // Close via X button: restore original opacity
                let data = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut OpacityDlgData;
                if !data.is_null() {
                    let win = &*(*data).stock_window;
                    let _ = SetLayeredWindowAttributes(win.get_hwnd(), COLORREF(0), (*data).original_opacity, LWA_ALPHA);
                }
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    fn handle_command(&mut self, wp: WPARAM, _lp: LPARAM) {
        let cmd = wp.0 as usize;
        match cmd {
            IDM_ADD_STOCK => {
                let result = self.show_input_dialog("添加股票/ETF", "请输入股票/ETF代码:", "");
                if !result.is_empty() {
                    let mut cfg = self.config.lock().unwrap();
                    cfg.add_symbol(&result);
                    if let Err(e) = cfg.save() { eprintln!("Failed to save config: {}", e); }
                    drop(cfg);
                    self.refresh();
                }
            }
            IDM_REFRESH_NOW => { self.refresh(); }
            c if c >= IDM_REMOVE_STOCK_BASE => {
                let idx = c - IDM_REMOVE_STOCK_BASE;
                let mut cfg = self.config.lock().unwrap();
                if idx < cfg.symbols.len() {
                    let sym = cfg.symbols[idx].clone();
                    cfg.remove_symbol(&sym);
                    if let Err(e) = cfg.save() { eprintln!("Failed to save config: {}", e); }
                    drop(cfg);
                    self.refresh();
                }
            }
            IDM_ADJUST_OPACITY => {
                let current = clamp_opacity(self.opacity.get());
                if let Some(new_opacity) = self.show_opacity_dialog(current) {
                    let new_opacity = clamp_opacity(new_opacity);
                    self.opacity.set(new_opacity);
                    unsafe { let _ = SetLayeredWindowAttributes(self.hwnd, COLORREF(0), new_opacity, LWA_ALPHA); }
                    {
                        let mut cfg = self.config.lock().unwrap();
                        cfg.opacity = new_opacity;
                        if let Err(e) = cfg.save() { eprintln!("Failed to save config: {e}"); }
                    }
                    unsafe { let _ = InvalidateRect(self.hwnd, None, true); };
                }
            }
            IDM_EXIT => { unsafe { let _ = DestroyWindow(self.hwnd); }; }
            _ => {}
        }
    }

    pub fn create(&mut self) -> Result<(), String> {
        let class_name = "StockWidgetClass";
        let class_name_w: Vec<u16> = class_name.encode_utf16().chain([0]).collect();

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(Self::wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: self.hinstance,
            hIcon: unsafe { LoadIconW(self.hinstance, PCWSTR(101usize as *const u16)) }.unwrap_or(HICON::default()),
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or(HCURSOR::default()),
            hbrBackground: HBRUSH::default(),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: PCWSTR(class_name_w.as_ptr()),
            hIconSm: unsafe { LoadIconW(self.hinstance, PCWSTR(101usize as *const u16)) }.unwrap_or(HICON::default()),
        };

        unsafe {
            let atom = RegisterClassExW(&wc);
            if atom == 0 {
                return Err(format!("RegisterClassExW failed, error={:?}", GetLastError()));
            }
        }

        let ws;
        {
            let cfg = self.config.lock().unwrap();
            ws = cfg.window.clone();
        }
        let cx = self.width.get();
        let cy = self.height.get();

        let title = "Stock Widget";
        let tw: Vec<u16> = title.encode_utf16().chain([0]).collect();

        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE((WS_EX_TOPMOST.0 | WS_EX_TOOLWINDOW.0 | WS_EX_LAYERED.0) as u32),
                PCWSTR(class_name_w.as_ptr()),
                PCWSTR(tw.as_ptr()),
                WINDOW_STYLE(WS_POPUP.0 | WS_BORDER.0),
                ws.x, ws.y, cx as i32, cy as i32,
                None, None, self.hinstance,
                Some(self as *const StockWindow as isize as *mut _),
            )
        };

        let hwnd = hwnd.map_err(|e| format!("CreateWindowExW failed: {:?}", e))?;
        if hwnd.is_invalid() {
            return Err(format!("CreateWindowExW failed, error={:?}", unsafe { GetLastError() }));
        }

        self.hwnd = hwnd;
        // Set semi-transparency (~82% opacity)
        unsafe { let _ = SetLayeredWindowAttributes(self.hwnd, COLORREF(0), self.opacity.get(), LWA_ALPHA); }
        self.refresh();



        // Show the window
        unsafe { let _ = ShowWindow(self.hwnd, SW_SHOW); }
        unsafe { let _ = UpdateWindow(self.hwnd); }

        Ok(())
    }

    extern "system" fn wnd_proc(
        hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_CREATE {
            let cs = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
            let pthis = cs.lpCreateParams as *mut StockWindow;
            if !pthis.is_null() {
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, pthis as isize) };
            }
            return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
        }

        let pthis = match Self::get_ptr(hwnd) {
            Some(p) => unsafe { &mut *p },
            None => return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        };

        match msg {
            WM_PAINT => { pthis.handle_paint(hwnd); LRESULT(0) }
            WM_TIMER => { pthis.handle_timer(wparam); LRESULT(0) }
            WM_LBUTTONDOWN => {
                let x = (lparam.0 as i32) as i16 as i32;
                let y = (((lparam.0 as i32) >> 16) as i16) as i32;
                if x >= pthis.width.get() - 8 {
                    // Start resize mode
                    pthis.is_resizing.set(true);
                    pthis.resize_start_x.set(x);
                    pthis.resize_start_width.set(pthis.width.get());
                } else {
                    // Start drag mode
                    pthis.is_dragging.set(true);
                    pthis.drag_start.set((x, y));
                }
                unsafe { SetCapture(hwnd) };
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let was_dragging = pthis.is_dragging.get();
                let was_resizing = pthis.is_resizing.get();
                pthis.is_dragging.set(false);
                pthis.is_resizing.set(false);
                unsafe { let _ = ReleaseCapture(); };

                // Save window position to config on drag/resize end
                if was_dragging || was_resizing {
                    let mut r = RECT::default();
                    unsafe { let _ = GetWindowRect(hwnd, &mut r); }
                    {
                        let mut cfg = pthis.config.lock().unwrap();
                        cfg.window.x = r.left;
                        cfg.window.y = r.top;
                        cfg.window.width = (r.right - r.left) as u32;
                        cfg.window.height = (r.bottom - r.top) as u32;
                        if let Err(e) = cfg.save() { eprintln!("Failed to save window position: {e}"); }
                    }
                }
                LRESULT(0)
            }
            WM_RBUTTONUP => {
                let client_x = (lparam.0 as i32) as i16 as i32;
                let client_y = (((lparam.0 as i32) >> 16) as i16) as i32;
                let mut wr = RECT::default();
                unsafe { let _ = GetWindowRect(hwnd, &mut wr); }
                let screen_x = wr.left + 1 + client_x;
                let screen_y = wr.top + 1 + client_y;
                pthis.handle_rbutton_up(hwnd, POINT { x: screen_x, y: screen_y });
                LRESULT(0)
            }
            WM_CONTEXTMENU => {
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                let f = wparam.0 as u32;
                let cx = (lparam.0 as i32) as i16 as i32;
                let cy = (((lparam.0 as i32) >> 16) as i16) as i32;
                if f & MK_LBUTTON.0 != 0 {
                    if pthis.is_resizing.get() {
                        let new_w = (pthis.resize_start_width.get() + cx - pthis.resize_start_x.get()).max(MIN_WIDTH);
                        pthis.width.set(new_w);
                        let mut r = RECT::default();
                        unsafe { let _ = GetWindowRect(hwnd, &mut r); }
                        unsafe { let _ = SetWindowPos(hwnd, None, r.left, r.top, new_w, r.bottom - r.top, SWP_NOZORDER); }
                        unsafe { let _ = InvalidateRect(hwnd, None, true); };
                    } else if pthis.is_dragging.get() {
                        let dx = cx - pthis.drag_start.get().0;
                        let dy = cy - pthis.drag_start.get().1;
                        let mut r = RECT::default();
                        unsafe { let _ = GetWindowRect(hwnd, &mut r); }
                        unsafe { let _ = SetWindowPos(hwnd, None, r.left + dx, r.top + dy, 0, 0, SWP_NOSIZE | SWP_NOZORDER); }
                    }
                } else {
                    pthis.is_dragging.set(false);
                    pthis.is_resizing.set(false);
                    // Change cursor when near right edge
                    if cx >= pthis.width.get() - 8 {
                        unsafe { SetCursor(LoadCursorW(None, IDC_SIZEWE).unwrap_or(HCURSOR::default())); }
                    }
                }
                LRESULT(0)
            }
            WM_COMMAND => { pthis.handle_command(wparam, lparam); LRESULT(0) }
            WM_DESTROY => { unsafe { PostQuitMessage(0) }; LRESULT(0) }
            WM_SIZE => {
                unsafe { let _ = InvalidateRect(hwnd, None, true); };
                LRESULT(0)
            }
            WM_CLOSE => { unsafe { let _ = DestroyWindow(hwnd); }; LRESULT(0) }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }
}
