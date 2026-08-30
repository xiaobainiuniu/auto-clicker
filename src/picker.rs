//! 全屏取点覆盖层：无边框置顶窗口铺满整个虚拟桌面（支持多显示器），
//! 背景是取点瞬间的屏幕快照（代替不可靠的透明窗口），叠加实时放大镜 + 十字准星，
//! 左键确认取点坐标，Esc 或关闭窗口取消。
//!
//! egui 0.31 通过 `Context::show_viewport_immediate` 创建子视口，
//! 渲染逻辑是 UI 回调函数（而非独立 App），见 `picker_ui`。
use crate::input;
use eframe::egui::{
    self, Align2, Color32, ColorImage, Context, CursorIcon, FontId, Key, Pos2, Rect, Stroke,
    TextureHandle, TextureOptions, Vec2, ViewportCommand,
};
use std::sync::{mpsc, Arc, Mutex};

/// 放大倍数
const ZOOM: f32 = 3.0_f32;
/// 放大镜显示尺寸（逻辑像素）
const VIEW_SIZE: f32 = 210.0_f32;
/// 放大镜截取源区域边长
const SRC_SIZE: u32 = (VIEW_SIZE / ZOOM) as u32;

/// 取点视口的跨帧状态。
pub struct PickerState {
    /// 放大镜纹理（每帧更新）
    pub tex: Option<TextureHandle>,
    /// 屏幕快照背景纹理（打开取点时截一次）
    pub bg: Option<TextureHandle>,
}

impl Default for PickerState {
    fn default() -> Self {
        Self { tex: None, bg: None }
    }
}

/// 取点视口的渲染回调：每帧被 `show_viewport_immediate` 调用。
/// 通过 channel 把取点结果发回主窗口（坐标为屏幕绝对物理坐标，可为负）。
pub fn picker_ui(
    ctx: &Context,
    state: &Arc<Mutex<PickerState>>,
    tx: &mpsc::Sender<Option<(i32, i32)>>,
) {
    // 强制覆盖层精确铺满虚拟桌面（物理像素，修正多屏混合缩放下的 DPI 换算偏差）
    input::force_window_to_virtual_screen("选择点击位置");

    ctx.set_cursor_icon(CursorIcon::Crosshair);

    let (mx, my) = input::get_cursor_pos();
    let (vx, vy, vw, vh) = input::virtual_screen();
    let ppi = ctx.pixels_per_point();

    let (pressed, esc, close_requested, logical) = ctx.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.key_pressed(Key::Escape),
            i.viewport().close_requested(),
            i.pointer.latest_pos(),
        )
    });

    // 结束取点：确认 / 取消 / 窗口被外部关闭（Alt+F4）
    if esc || close_requested {
        let _ = tx.send(None);
        ctx.send_viewport_cmd(ViewportCommand::Close);
        return;
    }
    if pressed {
        let _ = tx.send(Some((mx, my)));
        ctx.send_viewport_cmd(ViewportCommand::Close);
        return;
    }

    // 光标逻辑坐标：优先用 egui 换算的指针位置（多屏混合缩放也准确）
    let cursor =
        logical.unwrap_or_else(|| Pos2::new((mx - vx) as f32 / ppi, (my - vy) as f32 / ppi));

    let bg = state.lock().unwrap().bg.clone();

    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(Color32::from_rgb(16, 18, 24)))
        .show(ctx, |ui| {
            let rect = ui.max_rect();
            let painter = ui.painter();

            // ---- 背景：取点瞬间的屏幕快照（天然支持多显示器） ----
            if let Some(bg) = &bg {
                painter.image(
                    bg.id(),
                    rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0_f32, 1.0_f32)),
                    Color32::WHITE,
                );
            }

            // ---- 放大镜：截取光标周围区域并放大显示 ----
            let half = SRC_SIZE as i32 / 2;
            let in_screen = mx >= vx && my >= vy && mx < vx + vw && my < vy + vh;
            if in_screen && vw >= SRC_SIZE as i32 && vh >= SRC_SIZE as i32 {
                let sx = (mx - half).clamp(vx, vx + vw - SRC_SIZE as i32);
                let sy = (my - half).clamp(vy, vy + vh - SRC_SIZE as i32);
                if let Some(pixels) = input::capture_region(sx, sy, SRC_SIZE, SRC_SIZE) {
                    let img = ColorImage::from_rgba_unmultiplied(
                        [SRC_SIZE as usize, SRC_SIZE as usize],
                        &pixels,
                    );
                    let tex = {
                        let mut s = state.lock().unwrap();
                        if let Some(t) = s.tex.as_mut() {
                            t.set(img, TextureOptions::NEAREST);
                            t.clone()
                        } else {
                            let t = ctx.load_texture("magnifier", img, TextureOptions::NEAREST);
                            s.tex = Some(t.clone());
                            t
                        }
                    };

                    let mut mpos = cursor + Vec2::new(18.0_f32, 18.0_f32);
                    mpos.x = mpos.x.clamp(0.0_f32, (rect.right() - VIEW_SIZE).max(0.0_f32));
                    mpos.y = mpos.y.clamp(0.0_f32, (rect.bottom() - VIEW_SIZE).max(0.0_f32));
                    let mrect = Rect::from_min_size(mpos, Vec2::splat(VIEW_SIZE));
                    painter.rect_filled(mrect, 4.0_f32, Color32::from_rgba_unmultiplied(20, 22, 28, 235));
                    painter.rect_stroke(
                        mrect,
                        4.0_f32,
                        Stroke::new(2.0_f32, Color32::from_rgb(255, 120, 80)),
                        egui::StrokeKind::Outside,
                    );
                    painter.image(
                        tex.id(),
                        mrect.shrink(6.0_f32),
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0_f32, 1.0_f32)),
                        Color32::WHITE,
                    );
                    painter.text(
                        mpos + Vec2::new(8.0_f32, VIEW_SIZE - 24.0_f32),
                        Align2::LEFT_TOP,
                        format!("({mx}, {my})"),
                        FontId::proportional(13.0_f32),
                        Color32::WHITE,
                    );
                }
            }

            // ---- 十字准星 ----
            painter.line_segment(
                [Pos2::new(rect.left(), cursor.y), Pos2::new(rect.right(), cursor.y)],
                Stroke::new(1.5_f32, Color32::from_rgba_unmultiplied(255, 90, 90, 200)),
            );
            painter.line_segment(
                [Pos2::new(cursor.x, rect.top()), Pos2::new(cursor.x, rect.bottom())],
                Stroke::new(1.5_f32, Color32::from_rgba_unmultiplied(255, 90, 90, 200)),
            );
            painter.circle_stroke(cursor, 10.0_f32, Stroke::new(2.0_f32, Color32::WHITE));
            painter.circle_filled(cursor, 3.0_f32, Color32::WHITE);

            // ---- 操作提示：固定在屏幕底部中央，永不消失 ----
            let hint = "左键 确认取点  ·  Esc 取消";
            let galley = painter.layout_no_wrap(hint.to_string(), FontId::proportional(15.0_f32), Color32::WHITE);
            let gsize = galley.size();
            let hpos = Pos2::new((rect.width() - gsize.x) / 2.0_f32, rect.bottom() - gsize.y - 26.0_f32);
            painter.rect_filled(
                Rect::from_min_size(hpos - Vec2::new(8.0_f32, 5.0_f32), gsize + Vec2::new(16.0_f32, 10.0_f32)),
                5.0_f32,
                Color32::from_rgba_unmultiplied(0, 0, 0, 170),
            );
            painter.galley(hpos, galley, Color32::WHITE);
        });

    // 持续重绘以跟踪鼠标移动
    ctx.request_repaint();
}
