"""生成应用图标：assets/icon.ico（多尺寸）+ assets/icon.png（运行时窗口图标）。

用法: python tools/gen_icon.py
依赖: Pillow
"""
from PIL import Image, ImageDraw

S = 256
img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(img)

# 圆角深色背景
d.rounded_rectangle([8, 8, S - 8, S - 8], radius=60, fill=(30, 36, 52, 255))

cx = cy = S // 2
# 十字线
w = 14
half = 100
d.line([cx - half, cy, cx + half, cy], fill=(240, 244, 255, 255), width=w)
d.line([cx, cy - half, cx, cy + half], fill=(240, 244, 255, 255), width=w)
# 圆环
r = 64
d.ellipse([cx - r, cy - r, cx + r, cy + r], outline=(240, 244, 255, 255), width=14)
# 中心红点
r2 = 18
d.ellipse([cx - r2, cy - r2, cx + r2, cy + r2], fill=(255, 90, 90, 255))

img.save("assets/icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
img.save("assets/icon.png")
print("图标已生成: assets/icon.ico, assets/icon.png")
