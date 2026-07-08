import os
from PIL import Image, ImageDraw

out_dir = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "icons")
os.makedirs(out_dir, exist_ok=True)

BG = (30, 30, 35, 255)
FG = (90, 170, 255, 255)


def draw(size):
    img = Image.new("RGBA", (size, size), BG)
    d = ImageDraw.Draw(img)
    r = size * 0.32
    cx = cy = size / 2
    d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=FG)
    return img


base = draw(512)

sizes = {
    "32x32.png": 32,
    "128x128.png": 128,
    "128x128@2x.png": 256,
    "icon.png": 512,
}
for name, size in sizes.items():
    base.resize((size, size), Image.LANCZOS).save(os.path.join(out_dir, name))

base.save(
    os.path.join(out_dir, "icon.ico"),
    sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
)

# icns not critical on Windows-only target, but generate a basic one for completeness.
try:
    base.save(os.path.join(out_dir, "icon.icns"))
except Exception as e:
    print("icns skipped:", e)

print("done")
