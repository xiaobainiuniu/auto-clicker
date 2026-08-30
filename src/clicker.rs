//! 连点计时线程：按设定间隔循环注入点击，支持倒计时自动停止。
use crate::config::Config;
use crate::input;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClickMode {
    /// SendInput 注入，所有软件可用，光标会跳到目标点
    Universal,
    /// PostMessage 消息点击，光标完全不动，适用于普通窗口
    Background,
}

impl ClickMode {
    pub fn name(&self) -> &'static str {
        match self {
            ClickMode::Universal => "universal",
            ClickMode::Background => "background",
        }
    }
}

impl From<&str> for ClickMode {
    fn from(s: &str) -> Self {
        match s {
            "background" => ClickMode::Background,
            _ => ClickMode::Universal,
        }
    }
}

/// 连点器运行状态快照（供 UI 读取）。
#[derive(Clone, Copy)]
pub struct ClickStats {
    pub count: u64,
    pub running: bool,
    pub deadline: Option<Instant>,
}

/// 连点器句柄：持有控制标志与后台线程。
pub struct ClickerHandle {
    running: Arc<AtomicBool>,
    stats: Arc<Mutex<ClickStats>>,
    thread: Option<JoinHandle<()>>,
}

impl ClickerHandle {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            stats: Arc::new(Mutex::new(ClickStats {
                count: 0,
                running: false,
                deadline: None,
            })),
            thread: None,
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// 返回 (是否运行中, 已点击次数, 剩余时间)。
    pub fn snapshot(&self) -> (bool, u64, Option<Duration>) {
        let s = self.stats.lock().unwrap();
        let remaining = s.deadline.map(|d| d.saturating_duration_since(Instant::now()));
        (s.running, s.count, remaining)
    }

    /// 启动连点。duration_sec 为 0 表示不限时，直到手动停止。
    pub fn start(&mut self, point: (i32, i32), interval_ms: u64, duration_sec: u64, mode: ClickMode) {
        self.stop();

        let interval_ms = interval_ms.clamp(10, 60_000);
        let running = Arc::new(AtomicBool::new(true));
        let stats = Arc::new(Mutex::new(ClickStats {
            count: 0,
            running: true,
            deadline: if duration_sec > 0 {
                Some(Instant::now() + Duration::from_secs(duration_sec))
            } else {
                None
            },
        }));
        let deadline = stats.lock().unwrap().deadline;

        self.running = running.clone();
        self.stats = stats.clone();

        let handle = std::thread::spawn(move || {
            let interval = Duration::from_millis(interval_ms);
            let mut count: u64 = 0;
            while running.load(Ordering::Relaxed) {
                let t0 = Instant::now();

                let ok = match mode {
                    ClickMode::Universal => input::click_at_universal(point.0, point.1),
                    ClickMode::Background => input::click_at_background(point.0, point.1),
                };
                if ok {
                    count += 1;
                    stats.lock().unwrap().count = count;
                }

                // 倒计时到点自动停止
                if let Some(d) = deadline {
                    if Instant::now() >= d {
                        break;
                    }
                }
                // 分小段睡眠，保证停止响应及时
                let mut waited = t0.elapsed();
                while waited < interval {
                    if !running.load(Ordering::Relaxed) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                    waited = t0.elapsed();
                }
            }
            stats.lock().unwrap().running = false;
        });
        self.thread = Some(handle);
    }

    /// 停止连点并等待线程退出。
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
        let mut s = self.stats.lock().unwrap();
        s.running = false;
        s.deadline = None;
    }
}

impl Default for ClickerHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ClickerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// 切换连点状态（UI 按钮 / F6 热键 / 托盘菜单共用一条路径）。
/// 在热键或托盘线程里直接调用，不依赖 UI 线程存活。
/// 返回切换后是否处于运行中（未选点时无法启动，返回 false）。
pub fn toggle(clicker: &mut ClickerHandle, cfg: &Config) -> bool {
    if clicker.is_running() {
        clicker.stop();
        false
    } else if let Some(point) = cfg.point {
        clicker.start(
            point,
            cfg.interval_ms,
            cfg.duration_sec,
            ClickMode::from(cfg.mode.as_str()),
        );
        true
    } else {
        false
    }
}
