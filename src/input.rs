//! Win32 输入注入封装：SendInput 通用注入、PostMessage 后台注入、屏幕截取。
//!
//! 两种点击模式：
//! - 通用模式 `click_at_universal`：SendInput 直接注入到指定屏幕坐标，适用于所有软件。
//! - 后台模式 `click_at_background`：给目标窗口发 WM_LBUTTONDOWN/UP 消息，光标完全不动，
//!   适用于浏览器等普通窗口。
use winapi::ctypes::c_void;
use winapi::shared::windef::{HGDIOBJ, POINT, RECT};
use winapi::shared::winerror::ERROR_ALREADY_EXISTS;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::synchapi::CreateMutexW;
use winapi::um::winuser::{
    FindWindowW, GetCursorPos, GetDC, GetSystemMetrics, GetWindowRect, PostMessageW, ReleaseDC,
    ScreenToClient, SendInput, SetForegroundWindow, SetWindowPos, ShowWindow, WindowFromPoint,
    HWND_TOPMOST, MK_LBUTTON, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SWP_NOACTIVATE, SW_SHOW, WM_LBUTTONDOWN, WM_LBUTTONUP,
    INPUT, INPUT_MOUSE,
};
use winapi::um::wingdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject,
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CAPTUREBLT, DIB_RGB_COLORS, SRCCOPY,
};
use std::ptr::null_mut;

/// 当前鼠标在屏幕上的坐标（物理像素）。
pub fn get_cursor_pos() -> (i32, i32) {
    unsafe {
        let mut p = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut p) != 0 {
            (p.x, p.y)
        } else {
            (0, 0)
        }
    }
}

/// 单实例检测：用命名互斥体判断是否已有实例在运行。
/// Windows 允许同一 exe 开多个进程，这里用全局互斥体把程序限制为单实例
/// （否则热键会被第一个实例占用、托盘出现多个图标）。
/// 返回 true 表示已经有一个实例在运行（本进程是第二个）。
pub fn is_second_instance() -> bool {
    unsafe {
        let name: Vec<u16> = "AutoClickerSingleInstance\0".encode_utf16().collect();
        // 创建（或打开已存在的）命名互斥体；已存在时 GetLastError 返回 ERROR_ALREADY_EXISTS
        CreateMutexW(null_mut(), 0, name.as_ptr());
        GetLastError() == ERROR_ALREADY_EXISTS
    }
}

/// 按标题查找并显示主窗口（托盘 / 热键线程直接调用，不依赖 UI 线程存活）。
pub fn show_main_window(title: &str) -> bool {
    unsafe {
        let w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let hwnd = FindWindowW(null_mut(), w.as_ptr());
        if hwnd.is_null() {
            return false;
        }
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd) != 0
    }
}

/// 把指定标题的窗口精确铺满整个虚拟桌面（物理像素，无 DPI 换算）。
/// 用于取点覆盖层：egui/winit 的逻辑坐标换算在多屏混合缩放下不可靠，
/// 这里直接用 SetWindowPos 按物理像素定位，任何缩放组合都精确。
/// 返回是否找到并校正了窗口。
pub fn force_window_to_virtual_screen(title: &str) -> bool {
    unsafe {
        let wtitle: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let hwnd = FindWindowW(null_mut(), wtitle.as_ptr());
        if hwnd.is_null() {
            return false;
        }
        let (vx, vy, vw, vh) = virtual_screen();
        // 已在正确位置就不动（避免每帧系统调用的副作用）
        let mut rc: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut rc) != 0
            && rc.left == vx
            && rc.top == vy
            && rc.right - rc.left == vw
            && rc.bottom - rc.top == vh
        {
            return true;
        }
        SetWindowPos(hwnd, HWND_TOPMOST, vx, vy, vw, vh, SWP_NOACTIVATE) != 0
    }
}

/// 虚拟屏幕（所有显示器的合并矩形）：返回 (x, y, w, h)。
/// 多显示器时 x/y 可能为负（副屏在主屏左侧/上方时）。
pub fn virtual_screen() -> (i32, i32, i32, i32) {
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

/// 通用模式：SendInput 在指定屏幕坐标注入一次左键点击（支持多显示器）。
pub fn click_at_universal(x: i32, y: i32) -> bool {
    unsafe {
        let (vx, vy, vw, vh) = virtual_screen();
        if vw <= 1 || vh <= 1 {
            return false;
        }
        // SendInput 绝对坐标是相对虚拟桌面的 0..65535 归一化值
        let dx = ((x - vx) as i64 * 65535) / (vw as i64 - 1);
        let dy = ((y - vy) as i64 * 65535) / (vh as i64 - 1);

        let mut input = INPUT {
            type_: INPUT_MOUSE,
            u: std::mem::zeroed(),
        };
        {
            let mi = input.u.mi_mut();
            mi.dx = dx as i32;
            mi.dy = dy as i32;
            mi.mouseData = 0;
            mi.dwFlags = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK | MOUSEEVENTF_LEFTDOWN;
            mi.time = 0;
            mi.dwExtraInfo = 0;
        }
        let sent_down = SendInput(1, &mut input, std::mem::size_of::<INPUT>() as i32);
        input.u.mi_mut().dwFlags = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK | MOUSEEVENTF_LEFTUP;
        let sent_up = SendInput(1, &mut input, std::mem::size_of::<INPUT>() as i32);
        sent_down == 1 && sent_up == 1
    }
}

/// 后台模式：直接给点击位置下的窗口发送鼠标按下/抬起消息，光标完全不动。
pub fn click_at_background(x: i32, y: i32) -> bool {
    unsafe {
        let mut pt = POINT { x, y };
        let hwnd = WindowFromPoint(pt);
        if hwnd.is_null() {
            return false;
        }
        if ScreenToClient(hwnd, &mut pt) == 0 {
            return false;
        }
        let lparam = (((pt.y as u32) & 0xFFFF) << 16 | ((pt.x as u32) & 0xFFFF)) as isize;
        PostMessageW(hwnd, WM_LBUTTONDOWN, MK_LBUTTON as usize, lparam);
        PostMessageW(hwnd, WM_LBUTTONUP, 0, lparam);
        true
    }
}

/// 截取屏幕上 (x, y) 起 w×h 的区域，返回 RGBA 像素。
/// 超出主屏幕范围时返回 None（多显示器只支持主屏）。
pub fn capture_region(x: i32, y: i32, w: u32, h: u32) -> Option<Vec<u8>> {
    unsafe {
        let hdc = GetDC(null_mut());
        if hdc.is_null() {
            return None;
        }
        let mem = CreateCompatibleDC(hdc);
        if mem.is_null() {
            ReleaseDC(null_mut(), hdc);
            return None;
        }

        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = w as i32;
        bmi.bmiHeader.biHeight = -(h as i32); // 负值 = 自上而下
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;

        let mut bits: *mut c_void = null_mut();
        let bmp = CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        if bmp.is_null() || bits.is_null() {
            DeleteDC(mem);
            ReleaseDC(null_mut(), hdc);
            return None;
        }
        let old = SelectObject(mem, bmp as HGDIOBJ);
        BitBlt(mem, 0, 0, w as i32, h as i32, hdc, x, y, SRCCOPY | CAPTUREBLT);

        let n = (w * h * 4) as usize;
        let src = std::slice::from_raw_parts(bits as *const u8, n);
        let mut out = Vec::with_capacity(n);
        for p in src.chunks_exact(4) {
            out.extend_from_slice(&[p[2], p[1], p[0], 255]); // BGR -> RGBA
        }

        SelectObject(mem, old);
        DeleteObject(bmp as HGDIOBJ);
        DeleteDC(mem);
        ReleaseDC(null_mut(), hdc);
        Some(out)
    }
}
