"""由使用者提供的兩張品牌圖產生 App 圖示、官網 logo 與手機連結圖示。

- 圖一：App 圖示。去掉外圍白底與中央黑底，只留霓虹圖形（透明底）。
- 圖二：官網 logo。去掉外圍白底，保留深色圓角方塊。
- 圖五：手機連結圖示。去掉淺灰底，放進圓形淺藍底。
"""
from __future__ import annotations

import sys
from collections import deque
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parent.parent


def flood_white(image: Image.Image, tolerance: int = 14) -> Image.Image:
    """從四邊把接近白色的相連區域變透明；內部白色字樣不受影響。"""
    rgba = image.convert("RGBA")
    width, height = rgba.size
    pixels = rgba.load()
    seen = bytearray(width * height)
    queue: deque[tuple[int, int]] = deque()

    def near_white(x: int, y: int) -> bool:
        r, g, b, _ = pixels[x, y]
        return min(r, g, b) >= 244 - tolerance and max(r, g, b) >= 250 - tolerance

    for x in range(width):
        for y in (0, height - 1):
            if near_white(x, y):
                queue.append((x, y))
                seen[y * width + x] = 1
    for y in range(height):
        for x in (0, width - 1):
            if near_white(x, y) and not seen[y * width + x]:
                queue.append((x, y))
                seen[y * width + x] = 1
    while queue:
        x, y = queue.popleft()
        pixels[x, y] = (255, 255, 255, 0)
        for nx, ny in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
            if 0 <= nx < width and 0 <= ny < height and not seen[ny * width + nx] and near_white(nx, ny):
                seen[ny * width + nx] = 1
                queue.append((nx, ny))
    return rgba


def luminance_alpha(image: Image.Image) -> Image.Image:
    """黑底轉透明：保留霓虹光暈，純黑與近黑直接去掉。"""
    rgba = image.copy()
    pixels = rgba.load()
    for y in range(rgba.height):
        for x in range(rgba.width):
            r, g, b, a = pixels[x, y]
            if a == 0:
                continue
            level = max(r, g, b)
            if level <= 12:
                alpha = 0
            elif level < 170:
                alpha = int(a * min(1.0, (level - 12) / 158.0) * 0.9)
            else:
                alpha = a
            pixels[x, y] = (r, g, b, alpha)
    return rgba


def clean_frame_remnants(image: Image.Image, ring: float = 0.24) -> Image.Image:
    """清掉裁切後殘留的外框弧線與極淡灰邊，只動外圈的低飽和暗色像素。"""
    rgba = image.copy()
    pixels = rgba.load()
    width, height = rgba.size
    margin = int(min(width, height) * ring)
    for y in range(height):
        for x in range(width):
            r, g, b, a = pixels[x, y]
            if a == 0:
                continue
            edge = x < margin or y < margin or x >= width - margin or y >= height - margin
            if a < 8 or (edge and max(r, g, b) < 70 and max(r, g, b) - min(r, g, b) < 40):
                pixels[x, y] = (r, g, b, 0)
    return rgba


def cut_frame_corners(image: Image.Image, ratio: float = 0.26) -> Image.Image:
    """把四個角落裁成圓角安全區，確保來源圖的圓角外框弧線完全消失。"""
    rgba = image.copy()
    pixels = rgba.load()
    width, height = rgba.size
    radius = int(min(width, height) * ratio)
    inner = int(radius * 0.72)
    for y in range(radius):
        for x in range(radius):
            if (x - radius) ** 2 + (y - radius) ** 2 <= inner ** 2:
                continue
            for px, py in ((x, y), (width - 1 - x, y), (x, height - 1 - y), (width - 1 - x, height - 1 - y)):
                r, g, b, _ = pixels[px, py]
                pixels[px, py] = (r, g, b, 0)
    return rgba


def square_pad(image: Image.Image, ratio: float = 0.06) -> Image.Image:
    box = image.getbbox()
    if box is None:
        return image
    image = image.crop(box)
    side = max(image.width, image.height)
    side = int(side * (1.0 + ratio * 2))
    canvas = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    canvas.paste(image, ((side - image.width) // 2, (side - image.height) // 2), image)
    return canvas


def make_app_icon(source: Path) -> None:
    image = Image.open(source).convert("RGBA")
    # 來源圖最外圈是白底＋深藍圓角外框；先裁掉約 7%，只留框內的霓虹圖形。
    inset = int(min(image.size) * 0.07)
    image = image.crop((inset, inset, image.width - inset, image.height - inset))
    image = luminance_alpha(flood_white(image))
    image = square_pad(image, 0.08).resize((512, 512), Image.LANCZOS)
    image = clean_frame_remnants(image)
    image = cut_frame_corners(image)
    image.save(ROOT / "assets" / "kuidao-logo.png")
    image.save(
        ROOT / "assets" / "kuidao_logo.ico",
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )


def make_site_logo(source: Path) -> None:
    image = square_pad(flood_white(Image.open(source)), 0.03).resize((512, 512), Image.LANCZOS)
    (ROOT / "docs" / "images" / "brand").mkdir(parents=True, exist_ok=True)
    image.save(ROOT / "docs" / "images" / "brand" / "logo.webp", quality=86, method=6)
    image.resize((64, 64), Image.LANCZOS).save(ROOT / "docs" / "images" / "brand" / "favicon.png")
    image.resize((180, 180), Image.LANCZOS).save(ROOT / "docs" / "images" / "brand" / "apple-touch-icon.png")


def make_phone_link(source: Path) -> None:
    image = flood_white(Image.open(source), tolerance=24)
    # 截圖底部的灰色地板與淺灰底同色，一併去掉；裝置本身的藍色飽和度較高會保留。
    pixels = image.load()
    for y in range(image.height):
        for x in range(image.width):
            r, g, b, a = pixels[x, y]
            if a and max(r, g, b) > 170 and max(r, g, b) - min(r, g, b) < 14:
                pixels[x, y] = (r, g, b, 0)
    box = image.getbbox()
    if box:
        image = image.crop(box)
    image.thumbnail((190, 190), Image.LANCZOS)

    side = 256
    canvas = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    draw = ImageDraw.Draw(canvas)
    draw.ellipse((0, 0, side - 1, side - 1), fill=(233, 242, 252, 255))
    draw.ellipse((0, 0, side - 1, side - 1), outline=(196, 216, 238, 255), width=3)
    canvas.paste(image, ((side - image.width) // 2, (side - image.height) // 2), image)
    mask = Image.new("L", (side, side), 0)
    ImageDraw.Draw(mask).ellipse((0, 0, side - 1, side - 1), fill=255)
    canvas.putalpha(mask)
    canvas = canvas.filter(ImageFilter.UnsharpMask(radius=1.2, percent=40, threshold=2))
    (ROOT / "assets" / "social").mkdir(parents=True, exist_ok=True)
    canvas.save(ROOT / "assets" / "social" / "phonelink.png")


if __name__ == "__main__":
    app_source, site_source, phone_source = map(Path, sys.argv[1:4])
    make_app_icon(app_source)
    make_site_logo(site_source)
    make_phone_link(phone_source)
