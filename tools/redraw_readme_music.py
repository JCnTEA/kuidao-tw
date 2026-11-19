"""重繪 README 的音樂示意圖：繁體中文、LINE／IG 圖示、與 logo 同款深藍青底。"""
from PIL import Image, ImageDraw, ImageFont
import math

W, H = 2400, 560
BG = (11, 22, 32, 255)          # logo 卡片的深藍
ISLAND = (18, 26, 36, 255)
CTRL = (30, 39, 51, 255)
TEXT = (235, 242, 247, 255)
MUTED = (150, 165, 180, 255)
CYAN = (95, 220, 230, 255)

FONT = r"C:\Windows\Fonts\msjh.ttc"
def f(size, bold=False):
    return ImageFont.truetype(FONT, size, index=0 if not bold else 0)

img = Image.new("RGBA", (W, H), BG)
d = ImageDraw.Draw(img)

# 背景：深藍漸層（與 logo 卡片同調）
for yy in range(H):
    t = yy / H
    d.line([(0, yy), (W, yy)], fill=(int(9 + 6 * t), int(18 + 10 * t), int(27 + 12 * t), 255))

def island(x, y, w, h, side=False):
    # 外光
    d.rounded_rectangle([x - 3, y - 3, x + w + 3, y + h + 3], radius=40, fill=(95, 220, 230, 26))
    d.rounded_rectangle([x, y, x + w, y + h], radius=38, fill=ISLAND, outline=(52, 74, 92, 255), width=2)

def tab_icon(x, y, size, kind):
    d.rounded_rectangle([x, y, x + size, y + size], radius=10, fill=CTRL)
    if kind == "line":
        d.rounded_rectangle([x, y, x + size, y + size], radius=10, fill=(6, 199, 85, 255))
        d.rounded_rectangle([x + 8, y + 9, x + size - 8, y + size - 13], radius=7, fill=(255, 255, 255, 255))
        d.polygon([(x + size // 2 - 5, y + size - 13), (x + size // 2 + 5, y + size - 13), (x + size // 2, y + size - 6)], fill=(255, 255, 255, 255))
        d.line([(x + 13, y + 16), (x + size - 13, y + 16)], fill=(6, 199, 85, 255), width=2)
        d.line([(x + 13, y + 21), (x + size - 13, y + 21)], fill=(6, 199, 85, 255), width=2)
    elif kind == "ig":
        for i in range(size):
            t = i / size
            col = (int(245 - 100 * t), int(133 - 90 * t), int(41 + 130 * t), 255)
            d.line([(x + i, y), (x + i, y + size)], fill=col)
        m = Image.new("L", (size, size), 0)
        md = ImageDraw.Draw(m)
        md.rounded_rectangle([6, 6, size - 6, size - 6], radius=8, outline=255, width=3)
        md.ellipse([size // 2 - 7, size // 2 - 7, size // 2 + 7, size // 2 + 7], outline=255, width=3)
        md.ellipse([size - 14, 9, size - 9, 14], fill=255)
        white = Image.new("RGBA", (size, size), (255, 255, 255, 0))
        white.putalpha(m)
        img.paste(white, (x, y), white)

def note_icon(x, y, size=84):
    d.rounded_rectangle([x, y, x + size, y + size], radius=20, fill=CTRL)
    d.line([(x + 32, y + 58), (x + 32, y + 24), (x + 56, y + 19)], fill=MUTED, width=3)
    d.ellipse([x + 24, y + 54, x + 40, y + 68], fill=MUTED)

def tri(x, y, s, direction="right"):
    if direction == "right":
        d.polygon([(x, y - s), (x, y + s), (x + s * 1.6, y)], fill=TEXT)
    elif direction == "left":
        d.polygon([(x + s * 1.6, y - s), (x + s * 1.6, y + s), (x, y)], fill=TEXT)
    elif direction == "l2":
        d.polygon([(x, y - s), (x, y + s), (x + s * 1.3, y)], fill=MUTED)
        d.polygon([(x + s * 1.4, y - s), (x + s * 1.4, y + s), (x + s * 2.7, y)], fill=MUTED)
    elif direction == "r2":
        d.polygon([(x, y - s), (x, y + s), (x + s * 1.3, y)], fill=MUTED)
        d.polygon([(x + s * 1.4, y - s), (x + s * 1.4, y + s), (x + s * 2.7, y)], fill=MUTED)

def draw_island(x, y, w=760, h=190, with_tabs=True):
    island(x, y, w, h)
    note_icon(x + 26, y + 42)
    d.text((x + 126, y + 40), "暫無音樂", font=f(30), fill=TEXT)
    stamp = "11月5日 週四 09:41"
    d.text((x + 320, y + 44), stamp, font=f(22), fill=MUTED)
    # 設定齒輪
    cx, cy = x + w - 34, y + 44
    d.ellipse([cx - 11, cy - 11, cx + 11, cy + 11], outline=MUTED, width=2)
    d.ellipse([cx - 4, cy - 4, cx + 4, cy + 4], outline=MUTED, width=2)
    # 進度
    p0, p1 = x + 30, x + w - 30
    py = y + 108
    d.line([(p0, py), (p1, py)], fill=(70, 86, 102, 255), width=3)
    d.ellipse([p0 - 4, py - 4, p0 + 4, py + 4], fill=TEXT)
    d.text((p0, py + 12), "00:00", font=f(20), fill=MUTED)
    tw = d.textlength("-00:00", font=f(20))
    d.text((p1 - tw, py + 12), "-00:00", font=f(20), fill=MUTED)
    # 控制列
    cy2 = y + h - 30
    d.text((p0, cy2 - 12), "詞", font=f(22), fill=TEXT)
    tri(x + w // 2 - 96, cy2, 9, "l2")
    tri(x + w // 2 - 44, cy2, 12, "left")
    d.rounded_rectangle([x + w // 2 - 20, cy2 - 14, x + w // 2 + 20, cy2 + 14], radius=14, fill=CTRL)
    tri(x + w // 2 - 4, cy2, 9, "right")
    tri(x + w // 2 + 40, cy2, 9, "r2")
    # 音量
    vx = p1 - 60
    d.rounded_rectangle([vx, cy2 - 6, vx + 12, cy2 + 6], radius=3, fill=MUTED)
    d.polygon([(vx + 12, cy2 - 6), (vx + 12, cy2 + 6), (vx + 24, cy2 - 13), (vx + 24, cy2 + 13)], fill=MUTED)
    d.arc([vx + 14, cy2 - 14, vx + 34, cy2 + 14], -60, 60, fill=MUTED, width=2)
    if with_tabs:
        tab_icon(x + w - 104, y + 92, 40, "line")
        tab_icon(x + w - 54, y + 92, 40, "ig")

draw_island(60, 150)
draw_island(840, 150)
draw_island(1620, 150)

img.convert("RGBA").save(r"C:\Users\JCmTEA\Downloads\鬼島_倉庫_可直接推送\kui-island\docs\images\readme\music.webp", "WEBP", quality=92, method=6)
print("ok")
