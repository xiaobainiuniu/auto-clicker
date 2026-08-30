//! 全局热键线程：RegisterHotKey 注册 F2（取点）/ F6（开始停止），
//! 程序窗口不在前台时依然生效。
//!
//! F6 的切换直接在本线程执行（操作共享的连点器与配置），
//! 不经过 UI 线程——主窗口隐藏到托盘时 F6 依然有效；
//! F2 需要打开取点覆盖层（UI 资源），先恢复主窗口再把事件发给 UI，
//! 窗口恢复后事件随即被处理，取点覆盖层自动打开。
use crate::clicker::{self, ClickerHandle};
use crate::config::Config;
use crate::input;
use std::ptr::null_mut;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use winapi::shared::minwindef::UINT;
use winapi::um::winuser::{
    DispatchMessageW, GetMessageW, RegisterHotKey, TranslateMessage, MSG, VK_F2, VK_F6, WM_HOTKEY,
};

pub enum HotkeyEvent {
    Pick,
    Registration { pick_ok: bool, toggle_ok: bool },
}

pub fn spawn(
    tx: mpsc::Sender<HotkeyEvent>,
    clicker: Arc<Mutex<ClickerHandle>>,
    cfg: Arc<Mutex<Config>>,
) -> JoinHandle<()> {
    std::thread::spawn(move || unsafe {
        const MOD_NOREPEAT: UINT = 0x4000;
        const ID_PICK: u32 = 1;
        const ID_TOGGLE: u32 = 2;

        let pick_ok = RegisterHotKey(null_mut(), ID_PICK as i32, MOD_NOREPEAT, VK_F2 as u32) != 0;
        let toggle_ok = RegisterHotKey(null_mut(), ID_TOGGLE as i32, MOD_NOREPEAT, VK_F6 as u32) != 0;
        let _ = tx.send(HotkeyEvent::Registration { pick_ok, toggle_ok });

        let mut msg: MSG = std::mem::zeroed();
        loop {
            if GetMessageW(&mut msg, null_mut(), 0, 0) <= 0 {
                break;
            }
            if msg.message == WM_HOTKEY {
                match msg.wParam as u32 {
                    ID_PICK => {
                        // 连点运行中锁定取点（换点对进行中的任务不生效）；
                        // 空闲时先恢复主窗口，再让 UI 打开取点覆盖层
                        let running = clicker.lock().map(|c| c.is_running()).unwrap_or(false);
                        if !running {
                            input::show_main_window(crate::APP_NAME);
                            let _ = tx.send(HotkeyEvent::Pick);
                        }
                    }
                    // 直接切换连点，不依赖 UI 线程存活
                    ID_TOGGLE => {
                        let run = cfg.lock().ok().map(|c| c.clone());
                        if let (Ok(mut c), Some(run)) = (clicker.lock(), run) {
                            clicker::toggle(&mut c, &run);
                        }
                    }
                    _ => {}
                }
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    })
}
