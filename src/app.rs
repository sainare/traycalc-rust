#![allow(unsafe_op_in_unsafe_fn)]

use std::{
    mem::{size_of, transmute, zeroed},
    ptr::{null, null_mut},
    sync::{Mutex, OnceLock},
};
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM, ERROR_ALREADY_EXISTS},
    Graphics::Gdi::{
        BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect,
        GetStockObject, RoundRect, SelectObject, SetBkMode, SetTextColor, CLIP_DEFAULT_PRECIS,
        DEFAULT_CHARSET, DEFAULT_PITCH, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_RIGHT,
        DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, FW_NORMAL, HDC, HFONT, OUT_DEFAULT_PRECIS,
        PAINTSTRUCT, PROOF_QUALITY, TRANSPARENT, NULL_PEN, InvalidateRect, UpdateWindow,
    },
    System::{
        LibraryLoader::GetModuleHandleW,
        Registry::{RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ},
        Threading::CreateMutexW,
    },
    UI::{
        Controls::{EM_GETSEL, EM_REPLACESEL, EM_SETSEL},
        Input::KeyboardAndMouse::{GetAsyncKeyState, SetFocus, VK_BACK, VK_CONTROL, VK_ESCAPE, VK_MENU, VK_SHIFT},
        Shell::{Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW},
        WindowsAndMessaging::{
            AppendMenuW, CallNextHookEx, CallWindowProcW, CreateIcon, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW,
            FindWindowW, GetClientRect, GetForegroundWindow, GetMessageW, GetWindowLongPtrW, GetWindowRect,
            GetWindowTextLengthW, GetWindowTextW, GetCursorPos, IsWindowVisible, LoadCursorW,
            MoveWindow, PostMessageW, PostQuitMessage, RegisterClassExW, SendMessageW,
            SetForegroundWindow, SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowsHookExW,
            SetWindowPos, SetWindowTextW, ShowWindow, TranslateMessage, GetSystemMetrics,
            CREATESTRUCTW, CS_DBLCLKS, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT,
            ES_AUTOHSCROLL, ES_RIGHT, GWL_EXSTYLE, GWLP_WNDPROC, HMENU,
            IDC_ARROW, LWA_ALPHA, MINMAXINFO, MSG, SW_HIDE, SW_SHOW, SW_SHOWNORMAL,
            SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOZORDER, TPM_RETURNCMD, TPM_RIGHTBUTTON, MF_STRING,
            WH_KEYBOARD_LL, KBDLLHOOKSTRUCT, WINDOW_EX_STYLE, WM_APP,
            WM_CHAR, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_CTLCOLORSTATIC, WM_CTLCOLOREDIT,
            WM_ACTIVATE, WM_DESTROY, WM_ERASEBKGND, WM_GETMINMAXINFO, WM_KEYDOWN, WM_LBUTTONUP,
            WM_KEYUP, WM_NCLBUTTONDBLCLK, WM_PAINT, WM_RBUTTONUP, WM_SETFONT, WM_SETICON, WM_SHOWWINDOW, WM_SYSKEYDOWN, WM_SYSKEYUP,
            WM_MOVE, WM_SIZE, WNDCLASSEXW, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_EX_APPWINDOW,
            WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_THICKFRAME,
            WS_VISIBLE, HWND_NOTOPMOST, HWND_TOPMOST, SM_CXSCREEN, SM_CYSCREEN,
        },
    },
};

use crate::{calc, config::Settings, history};

const MAIN_CLASS: &str = "TrayCalcRustWindow";
const SETTINGS_CLASS: &str = "TrayCalcRustSettings";
const TITLE: &str = "TrayCalc";
const ID_EDIT: usize = 100;
const ID_OVERFLOW_LEFT: usize = 101;
const ID_OVERFLOW_RIGHT: usize = 102;
const WM_SHOW_ME: u32 = WM_APP + 77;
const WM_TRAY: u32 = WM_APP + 78;
const WM_TOGGLE: u32 = WM_APP + 79;
const TRAY_ID: u32 = 1;
const MENU_TOGGLE: usize = 1;
const MENU_SETTINGS: usize = 2;
const MENU_EXIT: usize = 3;

struct AppState {
    settings: Settings,
    entries: Vec<history::Entry>,
    hwnd_main: isize,
    hwnd_edit: isize,
    hwnd_overflow_left: isize,
    hwnd_overflow_right: isize,
    hwnd_settings: isize,
    old_edit_proc: isize,
    edit_font: isize,
    bg_brush: isize,
    icon: isize,
    hook: isize,
    tray_added: bool,
    preview: String,
    just_evaluated: bool,
    setting_text: bool,
    history_open: bool,
    last_tail: Option<String>,
    last_result: String,
    draft: Option<Settings>,
    capture_hotkey: bool,
}

impl AppState {
    fn new(settings: Settings) -> Self {
        let just_evaluated = !settings.expression.is_empty()
            && settings.history_line.ends_with(&format!(" = {}", settings.expression));
        Self {
            last_result: if just_evaluated { settings.expression.clone() } else { String::new() },
            settings,
            entries: history::load(),
            hwnd_main: 0,
            hwnd_edit: 0,
            hwnd_overflow_left: 0,
            hwnd_overflow_right: 0,
            hwnd_settings: 0,
            old_edit_proc: 0,
            edit_font: 0,
            bg_brush: 0,
            icon: 0,
            hook: 0,
            tray_added: false,
            preview: String::new(),
            just_evaluated,
            setting_text: false,
            history_open: false,
            last_tail: None,
            draft: None,
            capture_hotkey: false,
        }
    }
}

static APP: OnceLock<Mutex<AppState>> = OnceLock::new();
static HOTKEY_DOWN: AtomicBool = AtomicBool::new(false);
static HOTKEY_BLOCKED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
struct Palette {
    bg: u32,
    panel: u32,
    button: u32,
    operation: u32,
    foreground: u32,
    dim: u32,
    border: u32,
    accent: u32,
    accent_foreground: u32,
}

impl Palette {
    fn from(settings: &Settings) -> Self {
        let accent = colorref_from_rgb(settings.accent);
        let r = (settings.accent >> 16) & 0xff;
        let g = (settings.accent >> 8) & 0xff;
        let b = settings.accent & 0xff;
        let accent_foreground = if 299 * r + 587 * g + 114 * b > 160_000 {
            rgb(18, 18, 20)
        } else {
            rgb(255, 255, 255)
        };
        if settings.theme == 1 {
            Self {
                bg: rgb(245, 247, 251), panel: rgb(255, 255, 255), button: rgb(255, 255, 255),
                operation: rgb(232, 237, 245), foreground: rgb(28, 31, 38), dim: rgb(102, 111, 126),
                border: rgb(220, 225, 234), accent, accent_foreground,
            }
        } else {
            Self {
                bg: rgb(20, 22, 27), panel: rgb(27, 30, 37), button: rgb(34, 38, 47),
                operation: rgb(43, 47, 57), foreground: rgb(244, 246, 250), dim: rgb(145, 153, 168),
                border: rgb(52, 57, 68), accent, accent_foreground,
            }
        }
    }
}

pub fn run() {
    unsafe {
        // A distinct name prevents old C# editions from blocking this Rust release.
        // Re-opening this edition still reveals its already-running window.
        let mutex_name = wide("TrayCalc_Rust_SingleInstance_2f0");
        let singleton = CreateMutexW(null(), 1, mutex_name.as_ptr());
        if !singleton.is_null() && GetLastError() == ERROR_ALREADY_EXISTS {
            let class = wide(MAIN_CLASS);
            let existing = FindWindowW(class.as_ptr(), null());
            if !existing.is_null() {
                PostMessageW(existing, WM_SHOW_ME, 0, 0);
            }
            CloseHandle(singleton);
            return;
        }

        let instance = GetModuleHandleW(null());
        register_window_classes(instance);
        let settings = Settings::load();
        APP.set(Mutex::new(AppState::new(settings))).ok();

        let (x, y, width, height, topmost) = {
            let state = APP.get().unwrap().lock().unwrap();
            let x = if state.settings.x < 0 { CW_USEDEFAULT } else { state.settings.x };
            let y = if state.settings.y < 0 { CW_USEDEFAULT } else { state.settings.y };
            (x, y, state.settings.width, state.settings.height, state.settings.on_top)
        };
        let class = wide(MAIN_CLASS);
        let title = wide(TITLE);
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_APPWINDOW,
            class.as_ptr(), title.as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN,
            x, y, width, height, null_mut(), null_mut(), instance, null(),
        );
        if hwnd.is_null() {
            if !singleton.is_null() { CloseHandle(singleton); }
            return;
        }
        {
            let mut state = APP.get().unwrap().lock().unwrap();
            state.hwnd_main = hwnd as isize;
        }
        create_edit(hwnd, instance);
        recreate_theme_resources();
        apply_window_settings();
        let icon = create_app_icon();
        if !icon.is_null() {
            SendMessageW(hwnd, WM_SETICON, 0, icon as LPARAM);
            SendMessageW(hwnd, WM_SETICON, 1, icon as LPARAM);
            APP.get().unwrap().lock().unwrap().icon = icon as isize;
        }
        add_tray_icon();
        install_keyboard_hook();

        let start_hidden = std::env::args().any(|arg| arg == "--hidden");
        if !start_hidden {
            ShowWindow(hwnd, SW_SHOWNORMAL);
            UpdateWindow(hwnd);
            SetForegroundWindow(hwnd);
            SetFocus(hwnd_from_state(|s| s.hwnd_edit));
        }

        let mut message: MSG = zeroed();
        while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }

        cleanup_resources();
        if !singleton.is_null() { CloseHandle(singleton); }
    }
}


unsafe fn register_window_classes(instance: *mut core::ffi::c_void) {
    let cursor = LoadCursorW(null_mut(), IDC_ARROW);
    let main_name = wide(MAIN_CLASS);
    let main = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
        lpfnWndProc: Some(main_proc),
        hInstance: instance,
        hCursor: cursor,
        lpszClassName: main_name.as_ptr(),
        ..zeroed()
    };
    RegisterClassExW(&main);

    let settings_name = wide(SETTINGS_CLASS);
    let settings = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(settings_proc),
        hInstance: instance,
        hCursor: cursor,
        lpszClassName: settings_name.as_ptr(),
        ..zeroed()
    };
    RegisterClassExW(&settings);
}

unsafe fn create_edit(parent: HWND, instance: *mut core::ffi::c_void) {
    let class = wide("EDIT");
    let text = {
        let state = APP.get().unwrap().lock().unwrap();
        wide(&state.settings.expression)
    };
    let edit = CreateWindowExW(
        0, class.as_ptr(), text.as_ptr(),
        WS_CHILD | WS_VISIBLE | ES_RIGHT as u32 | ES_AUTOHSCROLL as u32,
        16, 60, 200, 52, parent, ID_EDIT as HMENU, instance, null(),
    );
    let old = SetWindowLongPtrW(edit, GWLP_WNDPROC, edit_proc as usize as isize);
    let static_class = wide("STATIC");
    let ellipsis = wide("…");
    let left = CreateWindowExW(0, static_class.as_ptr(), ellipsis.as_ptr(), WS_CHILD,
        20, 60, 24, 52, parent, ID_OVERFLOW_LEFT as HMENU, instance, null());
    let right = CreateWindowExW(0, static_class.as_ptr(), ellipsis.as_ptr(), WS_CHILD,
        20, 60, 24, 52, parent, ID_OVERFLOW_RIGHT as HMENU, instance, null());
    {
        let mut state = APP.get().unwrap().lock().unwrap();
        state.hwnd_edit = edit as isize;
        state.hwnd_overflow_left = left as isize;
        state.hwnd_overflow_right = right as isize;
        state.old_edit_proc = old;
    }
    layout_main(parent);
}

unsafe extern "system" fn main_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => 0,
        WM_SIZE => {
            layout_main(hwnd);
            persist_window_rect(hwnd);
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_MOVE => { persist_window_rect(hwnd); 0 }
        WM_ACTIVATE => {
            if (wparam & 0xffff) == 0 && APP.get().unwrap().lock().unwrap().settings.hide_on_blur {
                hide_main();
            }
            0
        }
        WM_PAINT => { paint_main(hwnd); 0 }
        WM_ERASEBKGND => 1,
        WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC => control_color(wparam),
        WM_COMMAND => {
            let id = wparam & 0xffff;
            let notification = (wparam >> 16) & 0xffff;
            if id == ID_EDIT && notification == 0x0300 { // EN_CHANGE
                edit_changed();
            }
            0
        }
        WM_LBUTTONUP => {
            let x = (lparam as i16) as i32;
            let y = ((lparam >> 16) as i16) as i32;
            main_click(hwnd, x, y);
            0
        }
        WM_GETMINMAXINFO => {
            let info = &mut *(lparam as *mut MINMAXINFO);
            info.ptMinTrackSize.x = 260;
            info.ptMinTrackSize.y = 380;
            0
        }
        WM_CLOSE => { hide_main(); 0 }
        WM_SHOW_ME => { show_main(); 0 }
        WM_TOGGLE => { toggle_main(); 0 }
        WM_TRAY => {
            match lparam as u32 {
                WM_LBUTTONUP => toggle_main(),
                WM_RBUTTONUP => show_tray_menu(hwnd),
                _ => {}
            }
            0
        }
        WM_NCLBUTTONDBLCLK => 0,
        WM_DESTROY => { PostQuitMessage(0); 0 }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn edit_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_KEYDOWN {
        if wparam as u32 == VK_ESCAPE as u32 {
            hide_main();
            return 0;
        }
        if wparam as u32 == b'H' as u32 && GetAsyncKeyState(VK_CONTROL as i32) < 0 {
            toggle_history();
            return 0;
        }
    }
    if msg == WM_CHAR {
        let ch = char::from_u32(wparam as u32).unwrap_or('\0');
        if ch == '\r' || ch == '=' {
            evaluate_current();
            return 0;
        }
        if ch == ',' || ch == '.' {
            before_typing(ch);
            let decimal = if APP.get().unwrap().lock().unwrap().settings.decimal_comma { "," } else { "." };
            replace_selection(decimal);
            return 0;
        }
        if !ch.is_control() && !"0123456789+-*/()%^ ".contains(ch) {
            return 0;
        }
        if !ch.is_control() {
            before_typing(ch);
        }
    }
    let result = call_old_edit(hwnd, msg, wparam, lparam);
    if msg == WM_KEYUP || msg == WM_LBUTTONUP {
        update_overflow_indicators();
    }
    result
}

unsafe fn call_old_edit(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let old = APP.get().unwrap().lock().unwrap().old_edit_proc;
    let proc = transmute::<isize, unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>(old);
    CallWindowProcW(Some(proc), hwnd, msg, wparam, lparam)
}

unsafe fn control_color(wparam: WPARAM) -> LRESULT {
    let hdc = wparam as HDC;
    let state = APP.get().unwrap().lock().unwrap();
    let palette = Palette::from(&state.settings);
    SetTextColor(hdc, palette.foreground);
    windows_sys::Win32::Graphics::Gdi::SetBkColor(hdc, palette.bg);
    SetBkMode(hdc, TRANSPARENT as i32);
    state.bg_brush
}

unsafe fn layout_main(hwnd: HWND) {
    if APP.get().is_none() { return; }
    let edit = hwnd_from_state(|s| s.hwnd_edit);
    if edit.is_null() { return; }
    let mut rect: RECT = zeroed();
    GetClientRect(hwnd, &mut rect);
    MoveWindow(edit, 18, 54, (rect.right - 36).max(40), 54, 1);
    let (left, right) = {
        let state = APP.get().unwrap().lock().unwrap();
        (state.hwnd_overflow_left as HWND, state.hwnd_overflow_right as HWND)
    };
    MoveWindow(left, 20, 54, 26, 54, 1);
    MoveWindow(right, (rect.right - 44).max(20), 54, 26, 54, 1);
    recreate_edit_font();
    update_overflow_indicators();
}

// Native EDIT controls scroll horizontally as the caret moves. The two small
// markers make that movement obvious instead of silently clipping long input.
unsafe fn update_overflow_indicators() {
    if APP.get().is_none() { return; }
    let (edit, left, right, scale) = {
        let state = APP.get().unwrap().lock().unwrap();
        (state.hwnd_edit as HWND, state.hwnd_overflow_left as HWND,
         state.hwnd_overflow_right as HWND, state.settings.font_scale as i32)
    };
    if edit.is_null() { return; }
    let text_length = GetWindowTextLengthW(edit).max(0) as u32;
    let mut rect: RECT = zeroed();
    GetClientRect(edit, &mut rect);
    let average_digit_width = (17 * scale / 100).max(9) as u32;
    let visible_capacity = ((rect.right - rect.left).max(1) as u32 / average_digit_width).max(1);
    if text_length <= visible_capacity {
        ShowWindow(left, SW_HIDE);
        ShowWindow(right, SW_HIDE);
        return;
    }
    let mut selection_start = 0u32;
    let mut selection_end = 0u32;
    SendMessageW(edit, EM_GETSEL, &mut selection_start as *mut u32 as WPARAM, &mut selection_end as *mut u32 as LPARAM);
    ShowWindow(left, if selection_start > 0 { SW_SHOW } else { SW_HIDE });
    ShowWindow(right, if selection_end < text_length { SW_SHOW } else { SW_HIDE });
}

unsafe fn recreate_edit_font() {
    if APP.get().is_none() { return; }
    let (old, edit, height) = {
        let state = APP.get().unwrap().lock().unwrap();
        let scale = state.settings.font_scale as i32;
        (state.edit_font, state.hwnd_edit, -(30 * scale / 100).max(18))
    };
    let font = create_font(height, FW_NORMAL as i32);
    if edit != 0 { SendMessageW(edit as HWND, WM_SETFONT, font as WPARAM, 1); }
    let mut state = APP.get().unwrap().lock().unwrap();
    state.edit_font = font as isize;
    if old != 0 { DeleteObject(old as _); }
}

unsafe fn recreate_theme_resources() {
    let (old, color) = {
        let state = APP.get().unwrap().lock().unwrap();
        (state.bg_brush, Palette::from(&state.settings).bg)
    };
    let brush = CreateSolidBrush(color);
    let mut state = APP.get().unwrap().lock().unwrap();
    state.bg_brush = brush as isize;
    if old != 0 { DeleteObject(old as _); }
}

unsafe fn paint_main(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut client: RECT = zeroed();
    GetClientRect(hwnd, &mut client);
    let (settings, history_line, preview, history_open, entries) = {
        let state = APP.get().unwrap().lock().unwrap();
        (state.settings.clone(), state.settings.history_line.clone(), state.preview.clone(), state.history_open, state.entries.clone())
    };
    let palette = Palette::from(&settings);
    fill(hdc, &client, palette.bg);

    let small = create_font(-(13 * settings.font_scale as i32 / 100).max(10), FW_NORMAL as i32);
    let medium = create_font(-(16 * settings.font_scale as i32 / 100).max(12), FW_NORMAL as i32);
    let button_font = create_font(-(19 * settings.font_scale as i32 / 100).max(13), FW_NORMAL as i32);

    draw_text(hdc, "☰", RECT { left: 18, top: 12, right: 48, bottom: 43 }, palette.dim, medium, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
    draw_text(hdc, &history_line, RECT { left: 52, top: 12, right: client.right - 18, bottom: 43 }, palette.dim, small, DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
    draw_text(hdc, &preview, RECT { left: 18, top: 110, right: client.right - 18, bottom: 142 }, palette.dim, medium, DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);

    if history_open {
        paint_history(hdc, &client, &entries, &palette, small, medium);
    } else {
        for (label, rect, kind) in button_rects(&client, settings.decimal_comma) {
            let (bg, fg) = match kind {
                ButtonKind::Equal => (palette.accent, palette.accent_foreground),
                ButtonKind::Number => (palette.button, palette.foreground),
                ButtonKind::Operation => (palette.operation, palette.foreground),
            };
            round_fill(hdc, &rect, bg, 14);
            draw_text(hdc, label, rect, fg, button_font, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        }
    }

    DeleteObject(small as _);
    DeleteObject(medium as _);
    DeleteObject(button_font as _);
    EndPaint(hwnd, &ps);
}

#[derive(Clone, Copy)]
enum ButtonKind { Number, Operation, Equal }

fn button_rects(client: &RECT, decimal_comma: bool) -> Vec<(&'static str, RECT, ButtonKind)> {
    let rows: [[&str; 4]; 5] = [
        ["C", "⌫", "(", ")"],
        ["7", "8", "9", "÷"],
        ["4", "5", "6", "×"],
        ["1", "2", "3", "−"],
        ["%", "0", ".", "+"],
    ];
    let left = 14;
    let right = client.right - 14;
    let top = 148;
    let bottom = client.bottom - 14;
    let gap = 6;
    let width = (right - left - gap * 3) / 4;
    let height = ((bottom - top - gap * 5) / 6).max(34);
    let mut out = Vec::with_capacity(23);
    for (row, labels) in rows.iter().enumerate() {
        for (col, label) in labels.iter().enumerate() {
            let rect = grid_rect(left, top, gap, width, height, col as i32, row as i32, 1);
            let label = if *label == "." && decimal_comma { "," } else { *label };
            let kind = if label.chars().next().unwrap_or(' ').is_ascii_digit() || label == "." || label == "," {
                ButtonKind::Number
            } else { ButtonKind::Operation };
            out.push((label, rect, kind));
        }
    }
    out.push(("^", grid_rect(left, top, gap, width, height, 0, 5, 1), ButtonKind::Operation));
    out.push(("=", grid_rect(left, top, gap, width, height, 1, 5, 2), ButtonKind::Equal));
    out.push(("⚙", grid_rect(left, top, gap, width, height, 3, 5, 1), ButtonKind::Operation));
    out
}

fn grid_rect(left: i32, top: i32, gap: i32, width: i32, height: i32, col: i32, row: i32, span: i32) -> RECT {
    let x = left + col * (width + gap);
    let y = top + row * (height + gap);
    RECT { left: x, top: y, right: x + width * span + gap * (span - 1), bottom: y + height }
}

unsafe fn paint_history(hdc: HDC, client: &RECT, entries: &[history::Entry], palette: &Palette, small: HFONT, medium: HFONT) {
    let panel = RECT { left: 14, top: 148, right: client.right - 14, bottom: client.bottom - 14 };
    round_fill(hdc, &panel, palette.panel, 16);
    if entries.is_empty() {
        draw_text(hdc, "Журнал пока пуст", panel, palette.dim, medium, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        return;
    }
    let mut y = panel.top + 12;
    for entry in entries.iter().take(((panel.bottom - panel.top - 58) / 58).max(1) as usize) {
        draw_text(hdc, &format!("{} =", entry.expression), RECT { left: panel.left + 14, top: y, right: panel.right - 14, bottom: y + 22 }, palette.dim, small, DT_RIGHT | DT_SINGLELINE | DT_END_ELLIPSIS);
        draw_text(hdc, &entry.result, RECT { left: panel.left + 14, top: y + 22, right: panel.right - 14, bottom: y + 52 }, palette.foreground, medium, DT_RIGHT | DT_SINGLELINE | DT_END_ELLIPSIS);
        y += 58;
        if y + 58 > panel.bottom - 45 { break; }
    }
    let clear = RECT { left: panel.left + 12, top: panel.bottom - 42, right: panel.right - 12, bottom: panel.bottom - 10 };
    round_fill(hdc, &clear, palette.operation, 10);
    draw_text(hdc, "Очистить журнал", clear, palette.dim, small, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
}

unsafe fn main_click(hwnd: HWND, x: i32, y: i32) {
    if y < 48 {
        toggle_history();
        return;
    }
    let mut client: RECT = zeroed();
    GetClientRect(hwnd, &mut client);
    let history_open = APP.get().unwrap().lock().unwrap().history_open;
    if history_open {
        let panel = RECT { left: 14, top: 148, right: client.right - 14, bottom: client.bottom - 14 };
        if y >= panel.bottom - 48 {
            let mut state = APP.get().unwrap().lock().unwrap();
            state.entries.clear();
            history::save(&state.entries);
            InvalidateRect(hwnd, null(), 0);
            return;
        }
        let index = ((y - panel.top - 12) / 58) as usize;
        let result = APP.get().unwrap().lock().unwrap().entries.get(index).map(|e| e.result.clone());
        if let Some(result) = result {
            set_expression(&result, true);
            let mut state = APP.get().unwrap().lock().unwrap();
            state.history_open = false;
            state.last_result = result;
            state.last_tail = None;
            InvalidateRect(hwnd, null(), 0);
        }
        return;
    }
    let decimal_comma = APP.get().unwrap().lock().unwrap().settings.decimal_comma;
    for (label, rect, _) in button_rects(&client, decimal_comma) {
        if point_in(rect, x, y) {
            match label {
                "C" => set_expression("", false),
                "⌫" => {
                    let edit = hwnd_from_state(|s| s.hwnd_edit);
                    SendMessageW(edit, WM_KEYDOWN, VK_BACK as WPARAM, 0);
                }
                "=" => evaluate_current(),
                "⚙" => open_settings(),
                "×" => insert_text("*"),
                "÷" => insert_text("/"),
                "−" => insert_text("-"),
                "." | "," => insert_text(label),
                _ => insert_text(label),
            }
            SetFocus(hwnd_from_state(|s| s.hwnd_edit));
            return;
        }
    }
}

unsafe fn before_typing(ch: char) {
    let evaluated = APP.get().unwrap().lock().unwrap().just_evaluated;
    if !evaluated { return; }
    if ch.is_ascii_digit() || matches!(ch, '.' | ',' | '(') {
        set_expression("", false);
    } else {
        let edit = hwnd_from_state(|s| s.hwnd_edit);
        let len = GetWindowTextLengthW(edit);
        SendMessageW(edit, EM_SETSEL, len as WPARAM, len as LPARAM);
    }
}

unsafe fn insert_text(text: &str) {
    if let Some(ch) = text.chars().next() { before_typing(ch); }
    replace_selection(text);
}

unsafe fn replace_selection(text: &str) {
    let edit = hwnd_from_state(|s| s.hwnd_edit);
    let wide = wide(text);
    SendMessageW(edit, EM_REPLACESEL, 1, wide.as_ptr() as LPARAM);
}

unsafe fn set_expression(text: &str, evaluated: bool) {
    let edit = hwnd_from_state(|s| s.hwnd_edit);
    {
        let mut state = APP.get().unwrap().lock().unwrap();
        state.setting_text = true;
        state.just_evaluated = evaluated;
    }
    let value = wide(text);
    SetWindowTextW(edit, value.as_ptr());
    SendMessageW(edit, EM_SETSEL, text.encode_utf16().count(), text.encode_utf16().count() as LPARAM);
    let mut state = APP.get().unwrap().lock().unwrap();
    state.setting_text = false;
    state.settings.expression = text.to_owned();
    state.settings.save();
    update_overflow_indicators();
}

unsafe fn edit_changed() {
    let setting = APP.get().unwrap().lock().unwrap().setting_text;
    if setting { return; }
    let text = get_edit_text();
    let mut state = APP.get().unwrap().lock().unwrap();
    state.just_evaluated = false;
    state.settings.expression = text.clone();
    state.preview = match calc::evaluate(text.trim()) {
        Ok(value) if !text.trim().is_empty() => format!("= {}", calc::format_number(value, state.settings.decimal_comma)),
        _ => String::new(),
    };
    state.settings.save();
    InvalidateRect(state.hwnd_main as HWND, null(), 0);
    update_overflow_indicators();
}

unsafe fn evaluate_current() {
    let mut expression = get_edit_text().trim().to_owned();
    if expression.is_empty() { return; }
    {
        let state = APP.get().unwrap().lock().unwrap();
        if state.just_evaluated && expression == state.last_result {
            if let Some(tail) = &state.last_tail { expression.push_str(tail); }
        }
    }
    match calc::evaluate(&expression) {
        Ok(value) => {
            let (result, line) = {
                let state = APP.get().unwrap().lock().unwrap();
                (calc::format_number(value, state.settings.decimal_comma), calc::pretty(&expression))
            };
            {
                let mut state = APP.get().unwrap().lock().unwrap();
                state.last_tail = calc::last_operation(&expression);
                state.last_result = result.clone();
                state.settings.history_line = format!("{line} = {result}");
                state.preview.clear();
                state.entries.insert(0, history::Entry { expression: line, result: result.clone() });
                state.entries.truncate(history::MAX_ENTRIES);
                history::save(&state.entries);
                state.settings.save();
            }
            set_expression(&result, true);
            InvalidateRect(hwnd_from_state(|s| s.hwnd_main), null(), 0);
        }
        Err(error) => {
            let mut state = APP.get().unwrap().lock().unwrap();
            state.preview = format!("Ошибка: {error}");
            InvalidateRect(state.hwnd_main as HWND, null(), 0);
        }
    }
}

unsafe fn toggle_history() {
    let mut state = APP.get().unwrap().lock().unwrap();
    state.history_open = !state.history_open;
    InvalidateRect(state.hwnd_main as HWND, null(), 0);
}

unsafe fn show_main() {
    let hwnd = hwnd_from_state(|s| s.hwnd_main);
    let (show_at, width, height) = {
        let state = APP.get().unwrap().lock().unwrap();
        (state.settings.show_at, state.settings.width, state.settings.height)
    };
    if IsWindowVisible(hwnd) == 0 && show_at != 0 {
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let mut point: POINT = zeroed();
        GetCursorPos(&mut point);
        let (x, y) = match show_at {
            1 => (point.x - width / 2, point.y - 24),
            2 => (screen_w - width - 18, screen_h - height - 58),
            3 => ((screen_w - width) / 2, (screen_h - height) / 2),
            _ => (0, 0),
        };
        MoveWindow(hwnd, x.clamp(0, (screen_w - width).max(0)), y.clamp(0, (screen_h - height).max(0)), width, height, 1);
    }
    ShowWindow(hwnd, SW_SHOW);
    SetForegroundWindow(hwnd);
    SetFocus(hwnd_from_state(|s| s.hwnd_edit));
}

unsafe fn persist_window_rect(hwnd: HWND) {
    if APP.get().is_none() || hwnd.is_null() { return; }
    let mut rect: RECT = zeroed();
    if GetWindowRect(hwnd, &mut rect) == 0 { return; }
    let mut state = APP.get().unwrap().lock().unwrap();
    state.settings.x = rect.left;
    state.settings.y = rect.top;
    state.settings.width = (rect.right - rect.left).max(260);
    state.settings.height = (rect.bottom - rect.top).max(380);
    state.settings.save();
}

unsafe fn hide_main() {
    ShowWindow(hwnd_from_state(|s| s.hwnd_main), SW_HIDE);
}

unsafe fn apply_window_settings() {
    let (hwnd, settings) = {
        let state = APP.get().unwrap().lock().unwrap();
        (state.hwnd_main as HWND, state.settings.clone())
    };
    if hwnd.is_null() { return; }
    SetLayeredWindowAttributes(hwnd, 0, ((settings.opacity as u32 * 255) / 100) as u8, LWA_ALPHA);
    SetWindowPos(hwnd, if settings.on_top { HWND_TOPMOST } else { HWND_NOTOPMOST }, 0, 0,
        settings.width, settings.height, SWP_NOMOVE | SWP_FRAMECHANGED);
    let mut ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
    if settings.in_taskbar {
        ex_style = (ex_style | WS_EX_APPWINDOW) & !WS_EX_TOOLWINDOW;
    } else {
        ex_style = (ex_style | WS_EX_TOOLWINDOW) & !WS_EX_APPWINDOW;
    }
    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style as isize);
    recreate_theme_resources();
    recreate_edit_font();
    InvalidateRect(hwnd, null(), 1);
}

unsafe fn open_settings() {
    let existing = hwnd_from_state(|s| s.hwnd_settings);
    if !existing.is_null() {
        ShowWindow(existing, SW_SHOW);
        SetForegroundWindow(existing);
        return;
    }
    let instance = GetModuleHandleW(null());
    let class = wide(SETTINGS_CLASS);
    let title = wide("Настройки TrayCalc");
    let owner = hwnd_from_state(|s| s.hwnd_main);
    {
        let mut state = APP.get().unwrap().lock().unwrap();
        state.draft = Some(state.settings.clone());
        state.capture_hotkey = false;
    }
    let hwnd = CreateWindowExW(
        WS_EX_APPWINDOW,
        class.as_ptr(), title.as_ptr(),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
        CW_USEDEFAULT, CW_USEDEFAULT, 590, 790,
        owner, null_mut(), instance, null(),
    );
    APP.get().unwrap().lock().unwrap().hwnd_settings = hwnd as isize;
    ShowWindow(hwnd, SW_SHOW);
    UpdateWindow(hwnd);
    SetForegroundWindow(hwnd);
}

unsafe extern "system" fn settings_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => { paint_settings(hwnd); 0 }
        WM_ERASEBKGND => 1,
        WM_LBUTTONUP => {
            settings_click(hwnd, (lparam as i16) as i32, ((lparam >> 16) as i16) as i32);
            0
        }
        WM_KEYDOWN if wparam as u32 == VK_ESCAPE as u32 => { close_settings(hwnd, false); 0 }
        WM_CLOSE => { close_settings(hwnd, false); 0 }
        WM_DESTROY => {
            let mut state = APP.get().unwrap().lock().unwrap();
            state.hwnd_settings = 0;
            state.draft = None;
            state.capture_hotkey = false;
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// The settings window is custom-painted; its implementation is below and keeps
// the native window fast while allowing a more playful card-based layout.
unsafe fn paint_settings(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut client: RECT = zeroed();
    GetClientRect(hwnd, &mut client);
    let draft = APP.get().unwrap().lock().unwrap().draft.clone().unwrap_or_default();
    let palette = Palette::from(&draft);
    fill(hdc, &client, palette.bg);
    let title = create_font(-27, 600);
    let medium = create_font(-16, 600);
    let normal = create_font(-14, FW_NORMAL as i32);
    let small = create_font(-12, FW_NORMAL as i32);

    draw_text(hdc, "Настройки", RECT { left: 24, top: 18, right: 400, bottom: 52 }, palette.foreground, title, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
    draw_text(hdc, "Сделайте калькулятор своим", RECT { left: 26, top: 51, right: 500, bottom: 70 }, palette.dim, small, DT_LEFT | DT_SINGLELINE);

    paint_card(hdc, RECT { left: 20, top: 78, right: client.right - 20, bottom: 208 }, "Быстрый вызов", palette, medium, normal);
    setting_row(hdc, "Горячая клавиша", &hotkey_text(draft.hotkey), 112, palette, normal, small, APP.get().unwrap().lock().unwrap().capture_hotkey);
    setting_row(hdc, "Клавиша в программах", if draft.block_key { "Блокировать" } else { "Оставить" }, 158, palette, normal, small, false);

    paint_card(hdc, RECT { left: 20, top: 220, right: client.right - 20, bottom: 440 }, "Внешний вид", palette, medium, normal);
    setting_row(hdc, "Тема", if draft.theme == 0 { "Тёмная" } else { "Светлая" }, 255, palette, normal, small, false);
    setting_row(hdc, "Десятичный знак", if draft.decimal_comma { "Запятая  1,5" } else { "Точка  1.5" }, 301, palette, normal, small, false);
    draw_text(hdc, "Акцент", RECT { left: 40, top: 349, right: 250, bottom: 376 }, palette.foreground, normal, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
    for (index, color) in ACCENTS.iter().enumerate() {
        let x = 330 + index as i32 * 48;
        let rect = RECT { left: x, top: 345, right: x + 30, bottom: 375 };
        round_fill(hdc, &rect, colorref_from_rgb(*color), 15);
        if draft.accent == *color {
            draw_text(hdc, "✓", rect, rgb(255, 255, 255), medium, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        }
    }
    draw_text(hdc, &format!("Прозрачность  {}%", draft.opacity), RECT { left: 40, top: 391, right: 280, bottom: 423 }, palette.foreground, normal, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
    paint_slider(hdc, 330, 405, 200, draft.opacity as i32, 30, 100, palette);

    paint_card(hdc, RECT { left: 20, top: 452, right: client.right - 20, bottom: 587 }, "Поведение", palette, medium, normal);
    paint_toggle(hdc, "Поверх окон", draft.on_top, 40, 490, palette, normal);
    paint_toggle(hdc, "Прятать без фокуса", draft.hide_on_blur, 300, 490, palette, normal);
    paint_toggle(hdc, "Панель задач", draft.in_taskbar, 40, 536, palette, normal);
    paint_toggle(hdc, "Автозагрузка", draft.auto_start, 300, 536, palette, normal);

    paint_card(hdc, RECT { left: 20, top: 599, right: client.right - 20, bottom: 687 }, "Окно", palette, medium, normal);
    setting_row(hdc, "Размер", size_text(&draft), 625, palette, normal, small, false);
    setting_row(hdc, "Открывать", position_text(draft.show_at), 658, palette, normal, small, false);

    let save = RECT { left: client.right - 156, top: 704, right: client.right - 24, bottom: 742 };
    let cancel = RECT { left: client.right - 286, top: 704, right: client.right - 166, bottom: 742 };
    round_fill(hdc, &save, palette.accent, 12);
    round_fill(hdc, &cancel, palette.operation, 12);
    draw_text(hdc, "Сохранить", save, palette.accent_foreground, normal, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    draw_text(hdc, "Отмена", cancel, palette.foreground, normal, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    draw_text(hdc, "Закрыть TrayCalc", RECT { left: 24, top: 704, right: 185, bottom: 742 }, rgb(215, 83, 92), normal, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

    DeleteObject(title as _); DeleteObject(medium as _); DeleteObject(normal as _); DeleteObject(small as _);
    EndPaint(hwnd, &ps);
}

const ACCENTS: [u32; 5] = [0x4c8bf5, 0x8b5cf6, 0x14b8a6, 0xf59e0b, 0xef476f];

unsafe fn paint_card(hdc: HDC, rect: RECT, title: &str, palette: Palette, font: HFONT, _: HFONT) {
    round_fill(hdc, &rect, palette.panel, 18);
    draw_text(hdc, title, RECT { left: rect.left + 20, top: rect.top + 10, right: rect.right - 20, bottom: rect.top + 38 }, palette.dim, font, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
}

unsafe fn setting_row(hdc: HDC, label: &str, value: &str, y: i32, palette: Palette, normal: HFONT, small: HFONT, active: bool) {
    draw_text(hdc, label, RECT { left: 40, top: y, right: 300, bottom: y + 32 }, palette.foreground, normal, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
    let rect = RECT { left: 315, top: y, right: 535, bottom: y + 32 };
    round_fill(hdc, &rect, if active { palette.accent } else { palette.operation }, 11);
    draw_text(hdc, if active { "Нажмите новую клавишу…" } else { value }, rect, if active { palette.accent_foreground } else { palette.foreground }, small, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
}

unsafe fn paint_toggle(hdc: HDC, text: &str, on: bool, x: i32, y: i32, palette: Palette, font: HFONT) {
    let track = RECT { left: x, top: y, right: x + 42, bottom: y + 24 };
    round_fill(hdc, &track, if on { palette.accent } else { palette.operation }, 12);
    let knob_x = if on { x + 21 } else { x + 3 };
    let knob = RECT { left: knob_x, top: y + 3, right: knob_x + 18, bottom: y + 21 };
    round_fill(hdc, &knob, rgb(255, 255, 255), 9);
    draw_text(hdc, text, RECT { left: x + 50, top: y - 2, right: x + 240, bottom: y + 28 }, palette.foreground, font, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
}

unsafe fn paint_slider(hdc: HDC, x: i32, y: i32, width: i32, value: i32, min: i32, max: i32, palette: Palette) {
    let track = RECT { left: x, top: y + 8, right: x + width, bottom: y + 14 };
    round_fill(hdc, &track, palette.operation, 3);
    let filled = width * (value - min) / (max - min);
    round_fill(hdc, &RECT { right: x + filled, ..track }, palette.accent, 3);
    round_fill(hdc, &RECT { left: x + filled - 8, top: y + 1, right: x + filled + 8, bottom: y + 21 }, palette.accent, 8);
}

unsafe fn settings_click(hwnd: HWND, x: i32, y: i32) {
    let mut state = APP.get().unwrap().lock().unwrap();
    let Some(draft) = state.draft.as_mut() else { return };
    match y {
        108..=143 if x >= 300 => state.capture_hotkey = true,
        153..=193 if x >= 300 => draft.block_key = !draft.block_key,
        250..=293 if x >= 300 => draft.theme = 1 - draft.theme,
        296..=341 if x >= 300 => draft.decimal_comma = !draft.decimal_comma,
        343..=382 if x >= 315 => {
            let index = ((x - 330) / 48).clamp(0, 4) as usize;
            draft.accent = ACCENTS[index];
        }
        390..=435 if x >= 315 => {
            draft.opacity = (30 + ((x - 330).clamp(0, 200) * 70 / 200)) as u8;
        }
        476..=520 if x < 290 => draft.on_top = !draft.on_top,
        476..=520 => draft.hide_on_blur = !draft.hide_on_blur,
        521..=570 if x < 290 => draft.in_taskbar = !draft.in_taskbar,
        521..=570 => draft.auto_start = !draft.auto_start,
        618..=651 if x >= 300 => cycle_size(draft),
        652..=690 if x >= 300 => draft.show_at = (draft.show_at + 1) % 4,
        700..=748 if x >= 410 => { drop(state); close_settings(hwnd, true); return; }
        700..=748 if x >= 280 => { drop(state); close_settings(hwnd, false); return; }
        700..=748 if x < 200 => { drop(state); destroy_app(); return; }
        _ => {}
    }
    InvalidateRect(hwnd, null(), 0);
}

unsafe fn close_settings(hwnd: HWND, save: bool) {
    if save {
        let draft = APP.get().unwrap().lock().unwrap().draft.clone();
        if let Some(draft) = draft {
            {
                let mut state = APP.get().unwrap().lock().unwrap();
                state.settings = draft;
                state.settings.save();
            }
            apply_window_settings();
            set_autostart(APP.get().unwrap().lock().unwrap().settings.auto_start);
        }
    }
    windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd);
}

unsafe fn destroy_app() {
    let main = hwnd_from_state(|s| s.hwnd_main);
    windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow(main);
}

fn cycle_size(settings: &mut Settings) {
    let next = match (settings.width, settings.height) {
        (300, 430) => (340, 500),
        (340, 500) => (420, 600),
        _ => (300, 430),
    };
    settings.width = next.0;
    settings.height = next.1;
}

fn size_text(settings: &Settings) -> &'static str {
    match (settings.width, settings.height) {
        (300, 430) => "Компактный",
        (420, 600) => "Большой",
        _ => "Обычный",
    }
}

fn position_text(value: u8) -> &'static str {
    match value { 1 => "Возле курсора", 2 => "Внизу справа", 3 => "По центру", _ => "Где оставили" }
}

fn hotkey_text(value: u32) -> String {
    let mut parts = Vec::new();
    if value & 0x0002_0000 != 0 { parts.push("Ctrl".to_owned()); }
    if value & 0x0004_0000 != 0 { parts.push("Alt".to_owned()); }
    if value & 0x0001_0000 != 0 { parts.push("Shift".to_owned()); }
    let key = (value & 0xffff) as u16;
    parts.push(match key {
        0x13 => "Pause".into(), 0x2d => "Insert".into(), 0x24 => "Home".into(),
        0x23 => "End".into(), 0x21 => "Page Up".into(), 0x22 => "Page Down".into(),
        0x70..=0x87 => format!("F{}", key - 0x6f),
        0x30..=0x5a => char::from_u32(key as u32).unwrap_or('?').to_string(),
        _ => format!("VK {key}"),
    });
    parts.join(" + ")
}

unsafe fn get_edit_text() -> String {
    let edit = hwnd_from_state(|s| s.hwnd_edit);
    let len = GetWindowTextLengthW(edit).max(0) as usize;
    let mut buffer = vec![0u16; len + 1];
    GetWindowTextW(edit, buffer.as_mut_ptr(), buffer.len() as i32);
    String::from_utf16_lossy(&buffer[..len])
}

unsafe fn cleanup_resources() {
    if APP.get().is_none() { return; }
    delete_tray_icon();
    let state = APP.get().unwrap().lock().unwrap();
    if state.hook != 0 { windows_sys::Win32::UI::WindowsAndMessaging::UnhookWindowsHookEx(state.hook as _); }
    if state.edit_font != 0 { DeleteObject(state.edit_font as _); }
    if state.bg_brush != 0 { DeleteObject(state.bg_brush as _); }
    if state.icon != 0 { windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon(state.icon as _); }
}

unsafe fn toggle_main() {
    let hwnd = hwnd_from_state(|s| s.hwnd_main);
    if IsWindowVisible(hwnd) != 0 { hide_main(); } else { show_main(); }
}

unsafe fn add_tray_icon() {
    let (hwnd, icon, hotkey) = {
        let state = APP.get().unwrap().lock().unwrap();
        (state.hwnd_main as HWND, state.icon as _, hotkey_text(state.settings.hotkey))
    };
    let mut data: NOTIFYICONDATAW = zeroed();
    data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = TRAY_ID;
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.uCallbackMessage = WM_TRAY;
    data.hIcon = icon;
    copy_wide_fixed(&mut data.szTip, &format!("TrayCalc — {hotkey}"));
    if Shell_NotifyIconW(NIM_ADD, &data) != 0 {
        APP.get().unwrap().lock().unwrap().tray_added = true;
    }
}

unsafe fn delete_tray_icon() {
    if APP.get().is_none() { return; }
    let (added, hwnd) = {
        let state = APP.get().unwrap().lock().unwrap();
        (state.tray_added, state.hwnd_main as HWND)
    };
    if !added { return; }
    let mut data: NOTIFYICONDATAW = zeroed();
    data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = TRAY_ID;
    Shell_NotifyIconW(NIM_DELETE, &data);
}

unsafe fn show_tray_menu(hwnd: HWND) {
    let menu = CreatePopupMenu();
    let toggle = wide("Показать / скрыть");
    let settings = wide("Настройки…");
    let exit = wide("Выход");
    AppendMenuW(menu, MF_STRING, MENU_TOGGLE, toggle.as_ptr());
    AppendMenuW(menu, MF_STRING, MENU_SETTINGS, settings.as_ptr());
    AppendMenuW(menu, MF_STRING, MENU_EXIT, exit.as_ptr());
    let mut point: POINT = zeroed();
    GetCursorPos(&mut point);
    SetForegroundWindow(hwnd);
    let selected = windows_sys::Win32::UI::WindowsAndMessaging::TrackPopupMenu(
        menu, TPM_RIGHTBUTTON | TPM_RETURNCMD, point.x, point.y, 0, hwnd, null(),
    ) as usize;
    DestroyMenu(menu);
    match selected {
        MENU_TOGGLE => toggle_main(),
        MENU_SETTINGS => open_settings(),
        MENU_EXIT => destroy_app(),
        _ => {}
    }
}

unsafe fn install_keyboard_hook() {
    let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), GetModuleHandleW(null()), 0);
    APP.get().unwrap().lock().unwrap().hook = hook as isize;
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 || APP.get().is_none() {
        return CallNextHookEx(null_mut(), code, wparam, lparam);
    }
    let message = wparam as u32;
    let key = &*(lparam as *const KBDLLHOOKSTRUCT);
    let key_down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
    let key_up = message == WM_KEYUP || message == WM_SYSKEYUP;

    let (capture, hotkey, block_setting, main, settings_hwnd) = {
        let state = APP.get().unwrap().lock().unwrap();
        (state.capture_hotkey, state.settings.hotkey, state.settings.block_key, state.hwnd_main as HWND, state.hwnd_settings as HWND)
    };
    if capture && key_down && !is_modifier(key.vkCode) {
        let encoded = current_modifiers() | key.vkCode;
        let mut state = APP.get().unwrap().lock().unwrap();
        if let Some(draft) = state.draft.as_mut() { draft.hotkey = encoded; }
        state.capture_hotkey = false;
        drop(state);
        InvalidateRect(settings_hwnd, null(), 0);
        return 1;
    }

    let expected_key = hotkey & 0xffff;
    if key.vkCode == expected_key {
        if key_down && current_modifiers() == hotkey & 0x000f_0000 {
            if !HOTKEY_DOWN.swap(true, Ordering::SeqCst) {
                PostMessageW(main, WM_TOGGLE, 0, 0);
            }
            let blocked = block_setting || GetForegroundWindow() == main;
            HOTKEY_BLOCKED.store(blocked, Ordering::SeqCst);
            if blocked { return 1; }
        } else if key_up {
            HOTKEY_DOWN.store(false, Ordering::SeqCst);
            if HOTKEY_BLOCKED.swap(false, Ordering::SeqCst) { return 1; }
        }
    }
    CallNextHookEx(null_mut(), code, wparam, lparam)
}

unsafe fn current_modifiers() -> u32 {
    let mut value = 0;
    if GetAsyncKeyState(VK_CONTROL as i32) < 0 { value |= 0x0002_0000; }
    if GetAsyncKeyState(VK_MENU as i32) < 0 { value |= 0x0004_0000; }
    if GetAsyncKeyState(VK_SHIFT as i32) < 0 { value |= 0x0001_0000; }
    value
}

fn is_modifier(key: u32) -> bool { matches!(key, 0x10 | 0x11 | 0x12 | 0x5b | 0x5c) }

unsafe fn set_autostart(enabled: bool) {
    let path = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let name = wide("TrayCalc");
    let mut key = null_mut();
    if RegCreateKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, null(), REG_OPTION_NON_VOLATILE,
        KEY_SET_VALUE, null(), &mut key, null_mut()) != 0 { return; }
    if enabled {
        if let Ok(exe) = std::env::current_exe() {
            let command = wide(&format!("\"{}\" --hidden", exe.display()));
            RegSetValueExW(key, name.as_ptr(), 0, REG_SZ, command.as_ptr() as *const u8, (command.len() * 2) as u32);
        }
    } else {
        RegDeleteValueW(key, name.as_ptr());
    }
    RegCloseKey(key);
}

fn copy_wide_fixed<const N: usize>(target: &mut [u16; N], value: &str) {
    for (slot, ch) in target.iter_mut().zip(value.encode_utf16().take(N - 1)) { *slot = ch; }
}

unsafe fn create_app_icon() -> windows_sys::Win32::UI::WindowsAndMessaging::HICON {
    let mut and_mask = [0u8; 128];
    let mut xor = [0u8; 32 * 32 * 4];
    for y in 0..32usize {
        for x in 0..32usize {
            let i = (y * 32 + x) * 4;
            let rounded = (x >= 4 && x < 28 && y >= 3 && y < 29)
                && !((x < 7 || x >= 25) && (y < 6 || y >= 26));
            if rounded {
                xor[i] = 245; xor[i + 1] = 139; xor[i + 2] = 76; xor[i + 3] = 255;
                if (8..=23).contains(&x) && (8..=11).contains(&y)
                    || (9..=12).contains(&x) && (17..=20).contains(&y)
                    || (19..=22).contains(&x) && (17..=20).contains(&y)
                {
                    xor[i] = 255; xor[i + 1] = 255; xor[i + 2] = 255;
                }
            } else {
                let bit = y * 32 + x;
                and_mask[bit / 8] |= 1 << (7 - bit % 8);
            }
        }
    }
    CreateIcon(GetModuleHandleW(null()), 32, 32, 1, 32, and_mask.as_ptr(), xor.as_ptr())
}

unsafe fn fill(hdc: HDC, rect: &RECT, color: u32) {
    let brush = CreateSolidBrush(color);
    FillRect(hdc, rect, brush);
    DeleteObject(brush as _);
}

unsafe fn round_fill(hdc: HDC, rect: &RECT, color: u32, radius: i32) {
    let brush = CreateSolidBrush(color);
    let old_brush = SelectObject(hdc, brush as _);
    let old_pen = SelectObject(hdc, GetStockObject(NULL_PEN));
    RoundRect(hdc, rect.left, rect.top, rect.right, rect.bottom, radius * 2, radius * 2);
    SelectObject(hdc, old_pen);
    SelectObject(hdc, old_brush);
    DeleteObject(brush as _);
}

unsafe fn draw_text(hdc: HDC, text: &str, mut rect: RECT, color: u32, font: HFONT, format: u32) {
    let wide = wide(text);
    let old = SelectObject(hdc, font as _);
    SetBkMode(hdc, TRANSPARENT as i32);
    SetTextColor(hdc, color);
    DrawTextW(hdc, wide.as_ptr(), -1, &mut rect, format);
    SelectObject(hdc, old);
}

unsafe fn create_font(height: i32, weight: i32) -> HFONT {
    let face = wide("Segoe UI");
    CreateFontW(height, 0, 0, 0, weight, 0, 0, 0, DEFAULT_CHARSET as u32, OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32, PROOF_QUALITY as u32, (DEFAULT_PITCH | FF_DONTCARE) as u32, face.as_ptr())
}

fn point_in(rect: RECT, x: i32, y: i32) -> bool {
    x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom
}

fn hwnd_from_state(read: impl FnOnce(&AppState) -> isize) -> HWND {
    APP.get().map(|app| read(&app.lock().unwrap()) as HWND).unwrap_or(null_mut())
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

const fn rgb(r: u32, g: u32, b: u32) -> u32 { r | (g << 8) | (b << 16) }
const fn colorref_from_rgb(value: u32) -> u32 {
    ((value >> 16) & 0xff) | (value & 0x00ff00) | ((value & 0xff) << 16)
}


