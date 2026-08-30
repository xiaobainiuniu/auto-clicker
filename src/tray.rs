//! 系统托盘：程序常驻通知区域图标。
//! 左键单击显示主窗口；右键菜单包含全部配置功能：
//! 开始/停止、取点、点击模式、间隔、时长、窗口置顶、退出。
//!
//! 关键设计：主窗口隐藏到托盘后，egui 的 UI 线程不再收到重绘事件、
//! channel 里的消息没人处理。因此开始/停止与各项配置修改
//! 都在托盘线程里直接执行（操作共享的连点器与配置对象），
//! 不经过 UI 线程；只有"取点"（需要 UI 覆盖层）与"退出"（优雅保存）
//! 发给 UI 线程，退出另有 400ms 超时兜底强杀，保证一定能退。
use crate::clicker::{self, ClickerHandle};
use crate::config::Config;
use crate::input;
use std::mem::size_of;
use std::ptr::null_mut;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;
use winapi::shared::minwindef::{DWORD, LPARAM, LRESULT, UINT, WPARAM};
use winapi::shared::windef::{HICON, HMENU, HWND, POINT};
use winapi::um::libloaderapi::GetModuleHandleW;
use winapi::um::shellapi::{
    Shell_NotifyIconW, NOTIFYICONDATAW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
};
use winapi::um::winuser::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetCursorPos, GetMessageW, GetSystemMetrics, LoadIconW, LoadImageW, MAKEINTRESOURCEW,
    MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, PostMessageW, RegisterClassW,
    SetForegroundWindow, TrackPopupMenu, TranslateMessage, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    WM_APP, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WNDCLASSW, IMAGE_ICON, LR_DEFAULTCOLOR, MSG,
    SM_CXSMICON, SM_CYSMICON,
};

/// 必须由 UI 线程处理的托盘动作（其余都在托盘线程直接执行）。
pub enum TrayEvent {
    /// 打开取点覆盖层（托盘线程已先恢复主窗口）
    Pick,
    /// 切换窗口置顶（ViewportCommand 属于 UI 资源）
    ToggleOnTop,
    /// 优雅退出：UI 保存配置并关闭；超时由托盘线程兜底强杀
    Quit,
}

/// UI 线程同步给托盘的最新状态（决定菜单文字与勾选）。
#[derive(Clone, Copy)]
pub struct TrayState {
    pub running: bool,
    pub has_point: bool,
    pub interval_ms: u64,
    pub duration_sec: u64,
    pub background_mode: bool,
    pub always_on_top: bool,
}

impl Default for TrayState {
    fn default() -> Self {
        Self {
            running: false,
            has_point: false,
            interval_ms: 100,
            duration_sec: 30,
            background_mode: false,
            always_on_top: false,
        }
    }
}

// 菜单项 ID
const ID_SHOW: u32 = 1;
const ID_TOGGLE: u32 = 2;
const ID_PICK: u32 = 3;
const ID_MODE_UNIVERSAL: u32 = 10;
const ID_MODE_BACKGROUND: u32 = 11;
const ID_INT_50: u32 = 20;
const ID_INT_100: u32 = 21;
const ID_INT_200: u32 = 22;
const ID_INT_500: u32 = 23;
const ID_DUR_10: u32 = 30;
const ID_DUR_30: u32 = 31;
const ID_DUR_60: u32 = 32;
const ID_DUR_300: u32 = 33;
const ID_DUR_INF: u32 = 34;
const ID_TOPMOST: u32 = 40;
const ID_QUIT: u32 = 99;

/// 托盘图标编号
const TRAY_ID: u32 = 1;
/// 托盘回调消息（自定义消息号）
const CALLBACK_MSG: UINT = WM_APP + 1;

static TRAY_TX: OnceLock<mpsc::Sender<TrayEvent>> = OnceLock::new();
static TRAY_STATE: OnceLock<Arc<Mutex<TrayState>>> = OnceLock::new();
static TRAY_CLICKER: OnceLock<Arc<Mutex<ClickerHandle>>> = OnceLock::new();
static TRAY_CONFIG: OnceLock<Arc<Mutex<Config>>> = OnceLock::new();

/// 隐藏窗口的窗口过程：处理托盘图标的鼠标事件。
unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: UINT, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == CALLBACK_MSG && wparam as u32 == TRAY_ID {
        match lparam as u32 {
            // 左键：直接用 Win32 恢复主窗口（不依赖 UI 线程）
            WM_LBUTTONUP => {
                input::show_main_window(crate::APP_NAME);
            }
            WM_RBUTTONUP => show_menu(hwnd),
            _ => {}
        }
        return 0;
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe fn append(menu: HMENU, flags: UINT, id: u32, text: &str) {
    let w: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    AppendMenuW(menu, flags, id as usize, w.as_ptr());
}

/// 追加子菜单项。
unsafe fn append_popup(menu: HMENU, sub: HMENU, text: &str) {
    let w: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    AppendMenuW(menu, MF_POPUP, sub as usize, w.as_ptr());
}

/// 修改共享配置并立即落盘（托盘线程直接执行，UI 下帧自动同步显示）。
fn update_config(f: impl FnOnce(&mut Config)) {
    let Some(cfg) = TRAY_CONFIG.get() else { return };
    if let Ok(mut c) = cfg.lock() {
        f(&mut c);
        c.save();
    }
}

/// 右键弹出菜单：全部配置功能（大部分在本线程直接执行）。
unsafe fn show_menu(hwnd: HWND) {
    let st = TRAY_STATE
        .get()
        .and_then(|s| s.lock().ok().map(|g| *g))
        .unwrap_or_default();
    // 连点运行中锁定配置项（运行中改动不生效，置灰防误导）
    let lock: UINT = if st.running { MF_GRAYED } else { 0 };

    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }

    append(menu, MF_STRING, ID_SHOW, "显示主窗口");
    append(menu, MF_SEPARATOR, 0, "");
    let toggle_text = if st.running { "■ 停止连点（F6）" } else { "▶ 开始连点（F6）" };
    // 还没选点时置灰，提示先取点
    let toggle_flags = MF_STRING | (if st.has_point { 0 } else { MF_GRAYED });
    append(menu, toggle_flags, ID_TOGGLE, toggle_text);
    append(menu, MF_STRING | lock, ID_PICK, "选择点击位置（F2）");
    append(menu, MF_SEPARATOR, 0, "");

    // 点击模式
    let m_mode = CreatePopupMenu();
    append(
        m_mode,
        MF_STRING | (if st.background_mode { 0 } else { MF_CHECKED }) | lock,
        ID_MODE_UNIVERSAL,
        "通用模式（适用所有软件）",
    );
    append(
        m_mode,
        MF_STRING | (if st.background_mode { MF_CHECKED } else { 0 }) | lock,
        ID_MODE_BACKGROUND,
        "后台模式（光标完全不动）",
    );
    append_popup(menu, m_mode, "点击模式");

    // 点击间隔（单位显示为秒）
    let m_int = CreatePopupMenu();
    let intervals: &[(u32, u64, &str)] = &[
        (ID_INT_50, 50, "0.05 秒（最快）"),
        (ID_INT_100, 100, "0.1 秒"),
        (ID_INT_200, 200, "0.2 秒"),
        (ID_INT_500, 500, "0.5 秒"),
    ];
    for (id, ms, label) in intervals {
        let mark = if st.interval_ms == *ms { MF_CHECKED } else { 0 };
        append(m_int, MF_STRING | mark | lock, *id, label);
    }
    append_popup(menu, m_int, "点击间隔");

    // 连点时长
    let m_dur = CreatePopupMenu();
    let durations: &[(u32, u64, &str)] = &[
        (ID_DUR_10, 10, "10 秒"),
        (ID_DUR_30, 30, "30 秒"),
        (ID_DUR_60, 60, "1 分钟"),
        (ID_DUR_300, 300, "5 分钟"),
        (ID_DUR_INF, 0, "不限时（按 F6 停止）"),
    ];
    for (id, sec, label) in durations {
        let mark = if st.duration_sec == *sec { MF_CHECKED } else { 0 };
        append(m_dur, MF_STRING | mark | lock, *id, label);
    }
    append_popup(menu, m_dur, "连点时长");

    append(menu, MF_SEPARATOR, 0, "");
    append(
        menu,
        MF_STRING | (if st.always_on_top { MF_CHECKED } else { 0 }),
        ID_TOPMOST,
        "窗口置顶",
    );
    append(menu, MF_SEPARATOR, 0, "");
    append(menu, MF_STRING, ID_QUIT, "退出");

    // 标准托盘菜单技巧：先置前台再弹菜单，最后发 WM_NULL 让菜单正常消失
    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_RIGHTBUTTON,
        pt.x,
        pt.y,
        0,
        hwnd,
        null_mut(),
    );
    PostMessageW(hwnd, WM_NULL, 0, 0);

    match cmd as u32 {
        // 直接执行：显示主窗口
        ID_SHOW => {
            input::show_main_window(crate::APP_NAME);
        }

        // 直接执行：切换连点（不依赖 UI 线程，窗口隐藏时同样有效）
        ID_TOGGLE => {
            if let (Some(clicker), Some(cfg)) = (TRAY_CLICKER.get(), TRAY_CONFIG.get()) {
                let run = cfg.lock().ok().map(|c| c.clone());
                if let (Ok(mut c), Some(run)) = (clicker.lock(), run) {
                    clicker::toggle(&mut c, &run);
                }
            }
        }

        // 取点需要 UI 覆盖层：先恢复主窗口，再发事件
        ID_PICK => {
            input::show_main_window(crate::APP_NAME);
            if let Some(tx) = TRAY_TX.get() {
                let _ = tx.send(TrayEvent::Pick);
            }
        }

        // 配置修改：直接写共享配置并落盘
        ID_MODE_UNIVERSAL => update_config(|c| c.mode = "universal".to_string()),
        ID_MODE_BACKGROUND => update_config(|c| c.mode = "background".to_string()),
        ID_INT_50 => update_config(|c| c.interval_ms = 50),
        ID_INT_100 => update_config(|c| c.interval_ms = 100),
        ID_INT_200 => update_config(|c| c.interval_ms = 200),
        ID_INT_500 => update_config(|c| c.interval_ms = 500),
        ID_DUR_10 => update_config(|c| c.duration_sec = 10),
        ID_DUR_30 => update_config(|c| c.duration_sec = 30),
        ID_DUR_60 => update_config(|c| c.duration_sec = 60),
        ID_DUR_300 => update_config(|c| c.duration_sec = 300),
        ID_DUR_INF => update_config(|c| c.duration_sec = 0),

        // 置顶是 UI 的 ViewportCommand，发给 UI 处理
        ID_TOPMOST => {
            if let Some(tx) = TRAY_TX.get() {
                let _ = tx.send(TrayEvent::ToggleOnTop);
            }
        }

        // 退出：先让 UI 优雅退出（保存配置），400ms 内没退就兜底强杀
        ID_QUIT => {
            if let Some(tx) = TRAY_TX.get() {
                let _ = tx.send(TrayEvent::Quit);
            }
            std::thread::sleep(Duration::from_millis(400));
            remove_tray_icon(hwnd);
            std::process::exit(0);
        }
        _ => {}
    }
}

/// 删除托盘图标（退出兜底路径：进程被强杀时 Windows 不会自动清理）。
unsafe fn remove_tray_icon(hwnd: HWND) {
    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
    nid.cbSize = size_of::<NOTIFYICONDATAW>() as DWORD;
    nid.hWnd = hwnd;
    nid.uID = TRAY_ID;
    Shell_NotifyIconW(NIM_DELETE, &mut nid);
}

/// 启动托盘线程：注册隐藏窗口 + 添加托盘图标 + 消息循环。
/// `state` 由 UI 线程每帧更新；`clicker` 与 `cfg` 是共享的连点器与配置，
/// 托盘菜单直接操作它们（窗口隐藏到托盘时功能依然完整）。
pub fn spawn(
    tx: mpsc::Sender<TrayEvent>,
    state: Arc<Mutex<TrayState>>,
    clicker: Arc<Mutex<ClickerHandle>>,
    cfg: Arc<Mutex<Config>>,
) -> JoinHandle<()> {
    std::thread::spawn(move || unsafe {
        let _ = TRAY_TX.set(tx);
        let _ = TRAY_STATE.set(state);
        let _ = TRAY_CLICKER.set(clicker);
        let _ = TRAY_CONFIG.set(cfg);

        // 注册隐藏消息窗口
        let class_name: Vec<u16> = "AutoClickerTrayWnd\0".encode_utf16().collect();
        let hinst = GetModuleHandleW(null_mut());
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null_mut(),
            lpszClassName: class_name.as_ptr(),
        };
        if RegisterClassW(&wc) == 0 {
            return;
        }
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            null_mut(),
            0,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            hinst,
            null_mut(),
        );
        if hwnd.is_null() {
            return;
        }

        // 托盘图标复用 exe 资源里数字 ID=1 的图标（icon.rc：`1 ICON "icon.ico"`）。
        // 优先按系统小图标尺寸加载（托盘区域更清晰），失败再退回 LoadIconW。
        let icon = {
            let small = LoadImageW(
                hinst,
                MAKEINTRESOURCEW(1),
                IMAGE_ICON,
                GetSystemMetrics(SM_CXSMICON),
                GetSystemMetrics(SM_CYSMICON),
                LR_DEFAULTCOLOR,
            ) as HICON;
            if !small.is_null() {
                small
            } else {
                LoadIconW(hinst, MAKEINTRESOURCEW(1))
            }
        };
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = size_of::<NOTIFYICONDATAW>() as DWORD;
        nid.hWnd = hwnd;
        nid.uID = TRAY_ID;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = CALLBACK_MSG;
        nid.hIcon = icon;
        let tip = "连点器 Auto Clicker";
        for (i, u) in tip.encode_utf16().chain(std::iter::once(0)).take(128).enumerate() {
            nid.szTip[i] = u;
        }
        Shell_NotifyIconW(NIM_ADD, &mut nid);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // 正常退出清理
        Shell_NotifyIconW(NIM_DELETE, &mut nid);
        DestroyWindow(hwnd);
    })
}
