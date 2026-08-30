//! 连点器 Auto Clicker — 主界面与程序入口。
//!
//! 用法：
//! - F2 或"选择位置"：打开全屏准星取点
//! - F6 或"开始/停止"按钮：开始 / 停止连点
//! - 倒计时到点自动停止，0 表示不限时
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clicker;
mod config;
mod hotkey;
mod input;
mod picker;
mod tray;

use clicker::ClickMode;
use eframe::egui::{
    self, Align, Color32, ColorImage, Context, FontData, FontDefinitions, FontFamily, Layout, Pos2,
    RichText, Stroke, TextEdit, TextureOptions, Vec2, ViewportCommand, WindowLevel,
};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

pub const APP_NAME: &str = "连点器 Auto Clicker";
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 顶刊风格配色（参考 Nature 期刊色系）：
/// 深蓝主色 + 青绿强调 + 砖红警示，浅灰蓝底、白色卡片。
mod theme {
    use eframe::egui::Color32;
    /// 主色：深蓝（标题、主按钮、坐标值）
    pub const PRIMARY: Color32 = Color32::from_rgb(0x3C, 0x54, 0x88);
    /// 强调：青绿（运行中、开始按钮）
    pub const ACCENT: Color32 = Color32::from_rgb(0x00, 0x9F, 0x87);
    /// 警示：砖红（停止按钮、错误）
    pub const DANGER: Color32 = Color32::from_rgb(0xE6, 0x4B, 0x35);
    /// 提醒：琥珀（取点中、警告文字）
    pub const WARN: Color32 = Color32::from_rgb(0xCC, 0x8A, 0x1E);
    /// 次要文字
    pub const SUBTLE: Color32 = Color32::from_rgb(0x5A, 0x6B, 0x7D);
    /// 卡片底色（白）
    pub const CARD: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
    /// 卡片描边
    pub const BORDER: Color32 = Color32::from_rgb(0xD5, 0xDD, 0xE6);
}

/// 白卡片容器：细描边、小圆角、紧凑内边距。
fn card_frame() -> egui::Frame {
    egui::Frame::default()
        .fill(theme::CARD)
        .stroke(Stroke::new(1.0_f32,theme::BORDER))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(10))
}

/// 常驻边框、文字居中的短数字输入框。
/// egui 的 TextEdit 边框只在悬停/聚焦时明显，这里关掉它的自带外观，
/// 用外层 Frame 画常驻描边；文字居中直接用 TextEdit 自带的
/// `horizontal_align(Center)`（提示文字与光标滚动都会正确处理）。
fn number_field(ui: &mut egui::Ui, text: &mut String, width: f32, enabled: bool, hint: &str) {
    egui::Frame::default()
        .fill(if enabled { theme::CARD } else { Color32::from_rgb(0xF0, 0xF3, 0xF8) })
        .stroke(Stroke::new(
            1.0_f32,
            if enabled {
                Color32::from_rgb(0x7D, 0x93, 0xAC)
            } else {
                theme::BORDER
            },
        ))
        .corner_radius(4.0)
        .inner_margin(egui::Margin { left: 4, right: 4, top: 3, bottom: 3 })
        .show(ui, |ui| {
            // 框总宽 = TextEdit 宽 + 内部边距(4+4) + Frame 边距(4+4)
            ui.add_enabled(
                enabled,
                TextEdit::singleline(text)
                    .frame(false)
                    .hint_text(hint)
                    .desired_width(width - 16.0)
                    .horizontal_align(Align::Center),
            );
        });
}

/// 应用顶刊风格浅色主题（覆盖 egui 默认 light 视觉的控件配色）。
fn apply_theme(ctx: &Context) {
    let mut v = egui::Visuals::light();
    v.panel_fill = Color32::from_rgb(0xF5, 0xF7, 0xFA);
    v.window_fill = theme::CARD;
    v.extreme_bg_color = theme::CARD; // 输入框底色
    v.faint_bg_color = Color32::from_rgb(0xEC, 0xF0, 0xF5);
    v.hyperlink_color = theme::PRIMARY;
    v.override_text_color = Some(Color32::from_rgb(0x22, 0x2E, 0x3D));
    v.selection.bg_fill = theme::PRIMARY;
    v.selection.stroke = Stroke::new(1.0_f32,Color32::WHITE);

    // 控件三态：白底细边框 → 悬停浅蓝 → 按下深一档，文字统一深蓝
    // （inactive 边框刻意用可感知的灰蓝：输入框不悬浮也常驻显示边框）
    let inactive_fg = Color32::from_rgb(0x33, 0x45, 0x60);
    let input_border = Color32::from_rgb(0xA8, 0xB6, 0xC6);
    v.widgets.inactive.bg_fill = theme::CARD;
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(0xF0, 0xF3, 0xF8);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32,input_border);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32,inactive_fg);
    v.widgets.hovered.bg_fill = Color32::from_rgb(0xE8, 0xEE, 0xF6);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0xE8, 0xEE, 0xF6);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32,theme::PRIMARY);
    v.widgets.hovered.fg_stroke = Stroke::new(1.2_f32,theme::PRIMARY);
    v.widgets.active.bg_fill = Color32::from_rgb(0xDC, 0xE5, 0xF1);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(0xDC, 0xE5, 0xF1);
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32,theme::PRIMARY);
    v.widgets.active.fg_stroke = Stroke::new(1.2_f32,Color32::from_rgb(0x2C, 0x3F, 0x66));
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32,theme::BORDER);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32,theme::SUBTLE);

    ctx.set_visuals(v);
}

/// 毫秒 → 秒的显示文本（最短表示：100 → "0.1"，50 → "0.05"）。
fn fmt_secs(ms: u64) -> String {
    let s = ms as f64 / 1000.0;
    let t = format!("{s:.3}");
    t.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn main() -> eframe::Result<()> {
    // 单实例限制：已有实例在运行时，把它的主窗口显示出来，本进程直接退出
    // （不提示、不弹窗，用户再点 exe 就相当于"唤出已运行的程序"）
    if input::is_second_instance() {
        input::show_main_window(APP_NAME);
        return Ok(());
    }

    let mut viewport = egui::ViewportBuilder::default()
        .with_title(APP_NAME)
        .with_inner_size([372.0, 480.0])
        .with_min_inner_size([350.0, 450.0]);
    if let Some(icon) = load_window_icon() {
        viewport = viewport.with_icon(Arc::new(icon));
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(APP_NAME, options, Box::new(|cc| Ok(Box::new(AutoClickerApp::new(cc)))))
}

/// 从系统字体加载中文字体（egui 默认字体不含 CJK 字形）。
/// 优先微软雅黑，按常见字体依次回退。
fn load_cjk_fonts(ctx: &Context) {
    let candidates = [
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\msyhbd.ttc",
        "C:\\Windows\\Fonts\\simhei.ttf",
        "C:\\Windows\\Fonts\\simsun.ttc",
        "C:\\Windows\\Fonts\\deng.ttf",
    ];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = FontDefinitions::default();
            fonts.font_data.insert("cjk".to_owned(), FontData::from_owned(bytes).into());
            for family in [FontFamily::Proportional, FontFamily::Monospace] {
                fonts.families.entry(family).or_default().insert(0, "cjk".to_owned());
            }
            ctx.set_fonts(fonts);
            return;
        }
    }
}

/// 从内置 PNG 生成窗口任务栏图标。
fn load_window_icon() -> Option<egui::IconData> {
    let png = include_bytes!("../assets/icon.png");
    let img = image::load_from_memory(png).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width: w,
        height: h,
    })
}

struct AutoClickerApp {
    /// 共享配置：托盘/热键线程直接读它启动连点、托盘菜单直接改它；
    /// UI 每帧把本地字段写回并做变化检测落盘。
    cfg: Arc<Mutex<config::Config>>,
    point: Option<(i32, i32)>,
    interval_ms: u64,
    /// 点击间隔输入框内容（单位：秒，支持 0.001）
    interval_text: String,
    duration_sec: u64,
    /// 倒计时输入框内容（单位：秒）
    duration_text: String,
    mode: ClickMode,
    always_on_top: bool,
    /// 共享连点器：F6 热键与托盘菜单在自己的线程里直接切换它，
    /// 不依赖 UI 线程（主窗口隐藏到托盘后 UI 不再重绘）。
    clicker: Arc<Mutex<clicker::ClickerHandle>>,
    hotkey_rx: mpsc::Receiver<hotkey::HotkeyEvent>,
    picker_tx: mpsc::Sender<Option<(i32, i32)>>,
    picker_rx: mpsc::Receiver<Option<(i32, i32)>>,
    picker_state: Arc<Mutex<picker::PickerState>>,
    picking: bool,
    tray_rx: mpsc::Receiver<tray::TrayEvent>,
    tray_state: Arc<Mutex<tray::TrayState>>,
    /// 真正退出标志：区分"托盘菜单退出"与"点 X 缩到托盘"
    quitting: bool,
    was_running: bool,
    /// 上次落盘的配置值（变化检测：改了立即保存，退出兜底强杀也不丢）
    last_saved: (Option<(i32, i32)>, u64, u64, ClickMode),
    status: String,
    status_color: Color32,
    hotkey_warn: String,
}

impl AutoClickerApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        load_cjk_fonts(&cc.egui_ctx);
        apply_theme(&cc.egui_ctx);

        let cfg_loaded = config::Config::load();
        // 从上次配置恢复
        let point = cfg_loaded.point;
        let mode = ClickMode::from(cfg_loaded.mode.as_str());
        let interval_ms = cfg_loaded.interval_ms.clamp(1, 60_000);
        let duration_sec = cfg_loaded.duration_sec.min(36_000);
        let interval_text = fmt_secs(interval_ms);
        let duration_text = duration_sec.to_string();
        let initial_status = match point {
            Some((x, y)) => format!("点击位置：({x}, {y})，按 F6 开始连点"),
            None => "就绪：按 F2 或点下方按钮选择点击位置".to_string(),
        };
        let cfg = Arc::new(Mutex::new(cfg_loaded));
        let clicker = Arc::new(Mutex::new(clicker::ClickerHandle::new()));
        let (hotkey_tx, hotkey_rx) = mpsc::channel();
        let (picker_tx, picker_rx) = mpsc::channel();
        let (tray_tx, tray_rx) = mpsc::channel();
        let tray_state = Arc::new(Mutex::new(tray::TrayState::default()));
        let _hotkey_thread = hotkey::spawn(hotkey_tx, clicker.clone(), cfg.clone());
        let _tray_thread = tray::spawn(tray_tx, tray_state.clone(), clicker.clone(), cfg.clone());

        Self {
            cfg,
            point,
            interval_ms,
            interval_text,
            duration_sec,
            duration_text,
            mode,
            always_on_top: false,
            clicker,
            hotkey_rx,
            picker_tx,
            picker_rx,
            picker_state: Arc::new(Mutex::new(picker::PickerState::default())),
            picking: false,
            tray_rx,
            tray_state,
            quitting: false,
            was_running: false,
            last_saved: (point, interval_ms, duration_sec, mode),
            status: initial_status,
            status_color: theme::SUBTLE,
            hotkey_warn: String::new(),
        }
    }

    fn toggle_run(&mut self) {
        // 状态提示由 update 里的运行状态机统一刷新（覆盖按钮/F6/托盘所有路径）
        let cfg = {
            let mut c = self.cfg.lock().unwrap();
            c.point = self.point;
            c.interval_ms = self.interval_ms;
            c.duration_sec = self.duration_sec;
            c.mode = self.mode.name().to_string();
            c.clone()
        };
        if self.point.is_none() && !self.clicker.lock().unwrap().is_running() {
            self.status = "请先选择点击位置（F2）".to_string();
            self.status_color = theme::WARN;
            return;
        }
        let mut c = self.clicker.lock().unwrap();
        clicker::toggle(&mut c, &cfg);
    }

    fn open_picker(&mut self, ctx: &Context) {
        if self.picking {
            return;
        }
        // 截取整个虚拟桌面作为取点背景快照（支持多显示器，坐标可为负）
        let (vx, vy, vw, vh) = input::virtual_screen();
        if vw > 0 && vh > 0 {
            if let Some(pixels) = input::capture_region(vx, vy, vw as u32, vh as u32) {
                let img = ColorImage::from_rgba_unmultiplied([vw as usize, vh as usize], &pixels);
                let tex = ctx.load_texture("picker_bg", img, TextureOptions::LINEAR);
                self.picker_state.lock().unwrap().bg = Some(tex);
            }
        }
        self.picking = true;
        self.status = "取点模式：移动鼠标，左键确认，Esc 取消".to_string();
        self.status_color = theme::WARN;
    }

    /// 每帧调用：只要 `picking` 为真就渲染取点子视口（egui 0.31 的
    /// `show_viewport_immediate` 需要在视口存在的每一帧都调用）。
    fn show_picker(&self, ctx: &Context) {
        let state = self.picker_state.clone();
        let tx = self.picker_tx.clone();
        // 覆盖层铺满整个虚拟桌面（多显示器全包含）
        let (vx, vy, vw, vh) = input::virtual_screen();
        let ppi = ctx.pixels_per_point();
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("auto-clicker-picker"),
            egui::ViewportBuilder::default()
                .with_title("选择点击位置")
                .with_decorations(false)
                .with_always_on_top()
                .with_resizable(false)
                .with_clamp_size_to_monitor_size(false)
                .with_position(Pos2::new(vx as f32 / ppi, vy as f32 / ppi))
                .with_inner_size(Vec2::new(vw as f32 / ppi, vh as f32 / ppi)),
            move |ctx, _class| picker::picker_ui(ctx, &state, &tx),
        );
    }

    fn handle_events(&mut self, ctx: &Context) {
        let events: Vec<hotkey::HotkeyEvent> = self.hotkey_rx.try_iter().collect();
        for ev in events {
            match ev {
                hotkey::HotkeyEvent::Pick => self.open_picker(ctx),
                hotkey::HotkeyEvent::Registration { pick_ok, toggle_ok } => {
                    let mut warns = Vec::new();
                    if !pick_ok {
                        warns.push("F2（取点）注册失败，可能被其他程序占用");
                    }
                    if !toggle_ok {
                        warns.push("F6（开始/停止）注册失败，可能被其他程序占用");
                    }
                    self.hotkey_warn = warns.join("\n");
                }
            }
        }
        while let Ok(res) = self.picker_rx.try_recv() {
            self.picking = false;
            // 释放屏幕快照纹理
            self.picker_state.lock().unwrap().bg = None;
            match res {
                Some((x, y)) => {
                    self.point = Some((x, y));
                    self.status = format!("点击位置：({x}, {y})，按 F6 开始连点");
                    self.status_color = theme::PRIMARY;
                }
                None => {
                    self.status = "已取消取点".to_string();
                    self.status_color = theme::SUBTLE;
                }
            }
        }
        // 托盘事件：只剩必须由 UI 处理的三种（开始停止/配置修改等
        // 都在托盘线程直接执行了，不依赖 UI 线程存活）
        while let Ok(ev) = self.tray_rx.try_recv() {
            match ev {
                tray::TrayEvent::Pick => {
                    // 托盘线程已用 Win32 恢复主窗口；这里同步 egui 可见状态再开取点
                    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                    self.open_picker(ctx);
                }
                tray::TrayEvent::ToggleOnTop => {
                    self.always_on_top = !self.always_on_top;
                }
                tray::TrayEvent::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            }
        }
    }

    fn ui_header(&self, ui: &mut egui::Ui) {
        let running = self.clicker.lock().unwrap().is_running();
        let (dot_color, state_text) = if self.picking {
            (theme::WARN, "取点中")
        } else if running {
            (theme::ACCENT, "运行中")
        } else {
            (theme::SUBTLE, "空闲")
        };
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(RichText::new("●").size(13.0).color(dot_color));
            ui.label(
                RichText::new("连点器")
                    .size(16.0)
                    .strong()
                    .color(theme::PRIMARY),
            );
            ui.label(RichText::new("Auto Clicker").size(12.0).color(theme::SUBTLE));
            ui.label(RichText::new(format!("v{VERSION}")).size(11.0).color(theme::SUBTLE));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(state_text).size(13.0).color(dot_color));
                ui.add_space(10.0);
            });
        });
    }

    fn ui_status(&self, ui: &mut egui::Ui) {
        ui.label(RichText::new(&self.status).size(13.0).color(self.status_color));
        if !self.hotkey_warn.is_empty() {
            ui.label(RichText::new(&self.hotkey_warn).size(11.0).color(theme::DANGER));
        }
    }

    fn ui_point_card(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let ctx = ui.ctx().clone();
            ui.horizontal(|ui| {
                // 主操作按钮放最左边；连点运行中锁定（换点对进行中的任务不生效）
                let running = self.clicker.lock().unwrap().is_running();
                let btn = egui::Button::new(
                    RichText::new("选择位置（F2）").size(13.0).color(Color32::WHITE),
                )
                .fill(theme::PRIMARY);
                if ui.add_enabled(!running, btn).clicked() {
                    self.open_picker(&ctx);
                }
            });
            match self.point {
                Some((x, y)) => {
                    // 坐标横排紧凑显示：冒号和数字贴近
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("X:").size(14.0).color(theme::SUBTLE));
                        ui.add_space(-3.0);
                        ui.label(
                            RichText::new(format!("{x}"))
                                .size(15.0)
                                .monospace()
                                .color(theme::PRIMARY),
                        );
                        ui.add_space(12.0);
                        ui.label(RichText::new("Y:").size(14.0).color(theme::SUBTLE));
                        ui.add_space(-3.0);
                        ui.label(
                            RichText::new(format!("{y}"))
                                .size(15.0)
                                .monospace()
                                .color(theme::PRIMARY),
                        );
                    });
                }
                None => {
                    ui.label(
                        RichText::new("未选择，点击左侧按钮或按 F2")
                            .size(12.0)
                            .color(theme::SUBTLE),
                    );
                }
            }
        });
    }

    fn ui_params_card(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            // 连点运行中锁定全部参数（间隔/时长/模式对进行中的任务不生效，改了只会误导）
            let running = self.clicker.lock().unwrap().is_running();
            ui.label(RichText::new("参数").size(13.0).strong().color(theme::PRIMARY));
            ui.add_space(4.0);

            egui::Grid::new("params")
                .num_columns(3)
                .spacing(Vec2::new(8.0, 6.0))
                .show(ui, |ui| {
                    ui.label(RichText::new("间隔（秒）").size(13.0));
                    number_field(ui, &mut self.interval_text, 70.0, !running, "0.1");
                    // 实时解析并校验（只接受数字，不做拖动）
                    match self.interval_text.trim().parse::<f64>() {
                        Ok(v) if (0.001..=60.0).contains(&v) => {
                            self.interval_ms = (v * 1000.0).round() as u64;
                            ui.label(RichText::new("").size(10.0));
                        }
                        _ => {
                            ui.label(
                                RichText::new("0.001~60")
                                    .size(10.0)
                                    .color(theme::DANGER),
                            );
                        }
                    }
                    ui.end_row();

                    ui.label(RichText::new("倒计时（秒）").size(13.0));
                    number_field(ui, &mut self.duration_text, 70.0, !running, "30");
                    match self.duration_text.trim().parse::<u64>() {
                        Ok(v) if v <= 36_000 => {
                            self.duration_sec = v;
                            let note = if v == 0 { "不限时" } else { "" };
                            ui.label(RichText::new(note).size(10.0).color(theme::SUBTLE));
                        }
                        _ => {
                            ui.label(
                                RichText::new("0~36000")
                                    .size(10.0)
                                    .color(theme::DANGER),
                            );
                        }
                    }
                    ui.end_row();
                });
            let note = if running {
                "连点运行中，参数已锁定（按 F6 停止后可修改）"
            } else {
                "间隔最小 0.001 秒 · 倒计时 0 = 不限时"
            };
            ui.label(RichText::new(note).size(11.0).color(theme::SUBTLE));
            ui.add_space(6.0);
            ui.label(RichText::new("点击模式").size(13.0).strong().color(theme::PRIMARY));
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !running,
                        egui::RadioButton::new(matches!(self.mode, ClickMode::Universal), "通用模式"),
                    )
                    .clicked()
                {
                    self.mode = ClickMode::Universal;
                }
                if ui
                    .add_enabled(
                        !running,
                        egui::RadioButton::new(matches!(self.mode, ClickMode::Background), "后台模式"),
                    )
                    .clicked()
                {
                    self.mode = ClickMode::Background;
                }
            });
            // 跟随所选模式切换的一行说明
            let mode_desc = match self.mode {
                ClickMode::Universal => "通用模式：点击瞬间鼠标会移到目标位置，期间请勿动鼠标",
                ClickMode::Background => "后台模式：点击时光标不动，您可以专注其他事情",
            };
            ui.label(RichText::new(mode_desc).size(11.0).color(theme::SUBTLE));
            ui.add_space(4.0);
            ui.checkbox(&mut self.always_on_top, "窗口置顶");
        });
    }

    fn ui_control_card(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let running = self.clicker.lock().unwrap().is_running();
            let (_, count, remaining) = self.clicker.lock().unwrap().snapshot();

            let label = if running { "■  停止连点（F6）" } else { "▶  开始连点（F6）" };
            let fill = if running { theme::DANGER } else { theme::ACCENT };
            let clicked = ui
                .add_sized(
                    [ui.available_width(), 36.0],
                    egui::Button::new(RichText::new(label).size(15.0).color(Color32::WHITE))
                        .fill(fill),
                )
                .on_hover_text("全局快捷键 F6")
                .clicked();
            if clicked {
                self.toggle_run();
            }

            let stat_text = if running {
                let secs = remaining.map(|d| d.as_secs().to_string()).unwrap_or_else(|| "∞".to_string());
                format!("已点击 {count} 次 · 剩余 {secs} 秒")
            } else {
                format!("已点击 {count} 次")
            };
            ui.label(
                RichText::new(stat_text)
                    .size(12.0)
                    .color(if running { theme::ACCENT } else { theme::SUBTLE }),
            );
        });
    }
}

impl eframe::App for AutoClickerApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // 托盘线程可能直接改了共享配置（窗口隐藏时托盘菜单依然可用）：
        // 先同步到本地显示字段（必须在本帧事件处理之前，否则会覆盖
        // 取点等事件刚更新的本地值）
        {
            let shared = self.cfg.lock().unwrap().clone();
            if shared.point != self.point {
                self.point = shared.point;
            }
            if shared.interval_ms != self.interval_ms {
                self.interval_ms = shared.interval_ms;
                self.interval_text = fmt_secs(shared.interval_ms);
            }
            if shared.duration_sec != self.duration_sec {
                self.duration_sec = shared.duration_sec;
                self.duration_text = shared.duration_sec.to_string();
            }
            let shared_mode = ClickMode::from(shared.mode.as_str());
            if shared_mode != self.mode {
                self.mode = shared_mode;
            }
        }

        self.handle_events(ctx);

        if self.picking {
            self.show_picker(ctx);
        }

        egui::TopBottomPanel::top("header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 10)))
            .show(ctx, |ui| self.ui_header(ui));

        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(Color32::from_rgb(0xF5, 0xF7, 0xFA))
                    .inner_margin(egui::Margin::same(12)),
            )
            .show(ctx, |ui| {
                // 紧凑布局：内容一屏放下，不再需要滚动
                ui.set_width(ui.available_width());
                ui.add_space(2.0);
                self.ui_status(ui);
                ui.add_space(8.0);
                self.ui_point_card(ui);
                ui.add_space(8.0);
                self.ui_params_card(ui);
                ui.add_space(8.0);
                self.ui_control_card(ui);
            });

        // 点窗口 X：缩到托盘继续运行（正在连点的不中断）；托盘菜单"退出"才真正退出
        if !self.quitting && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
            self.status = "已最小化到托盘，右键托盘图标可退出".to_string();
        }

        // 运行状态机：启动/停止统一刷新提示（覆盖按钮 / F6 / 托盘所有路径，
        // 包括 UI 线程之外直接切换的情况）
        let running = self.clicker.lock().unwrap().is_running();
        if running && !self.was_running {
            self.status = match self.point {
                Some((x, y)) => format!("运行中：点击 ({x}, {y})"),
                None => "运行中".to_string(),
            };
            self.status_color = theme::ACCENT;
        }
        if !running && self.was_running {
            let (_, count, _) = self.clicker.lock().unwrap().snapshot();
            self.status = format!("已停止，共点击 {count} 次");
            self.status_color = theme::SUBTLE;
        }
        self.was_running = running;

        // 把最新状态同步给托盘（右键菜单的文字与勾选依据）
        {
            let mut ts = self.tray_state.lock().unwrap();
            ts.running = running;
            ts.has_point = self.point.is_some();
            ts.interval_ms = self.interval_ms;
            ts.duration_sec = self.duration_sec;
            ts.background_mode = matches!(self.mode, ClickMode::Background);
            ts.always_on_top = self.always_on_top;
        }

        // 配置变化检测：改了就立即写回共享对象并落盘
        // （托盘退出兜底强杀时不走 on_exit，靠这里保证不丢配置）
        let cur = (self.point, self.interval_ms, self.duration_sec, self.mode);
        if cur != self.last_saved {
            {
                let mut c = self.cfg.lock().unwrap();
                c.point = self.point;
                c.interval_ms = self.interval_ms;
                c.duration_sec = self.duration_sec;
                c.mode = self.mode.name().to_string();
            }
            self.cfg.lock().unwrap().save();
            self.last_saved = cur;
        }

        // 置顶开关生效
        let level = if self.always_on_top { WindowLevel::AlwaysOnTop } else { WindowLevel::Normal };
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(level));

        // 定期重绘，保证倒计时与热键事件及时刷新
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.clicker.lock().unwrap().stop();
        self.cfg.lock().unwrap().save();
    }
}
