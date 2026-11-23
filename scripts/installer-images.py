"""Recrée les deux images de l'assistant d'installation Windows à partir du logo.

    python scripts/installer-images.py

Prérequis : Python 3 et Pillow (pip install pillow). Lance d'abord `npm run logo -- <logo.svg>`
pour que src-tauri/icons/icon.png soit à jour.

Produit (formats exigés par NSIS : BMP 24 bits) :
  src-tauri/installer/sidebar.bmp   164 x 314, pages d'accueil et de fin
  src-tauri/installer/header.bmp    150 x 57, en-tête des autres pages
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
ICON = ROOT / "src-tauri" / "icons" / "icon.png"
OUT = ROOT / "src-tauri" / "installer"

FONTS_BOLD = [
    "C:/Windows/Fonts/segoeuib.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
]
FONTS_REGULAR = [
    "C:/Windows/Fonts/segoeui.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
]


def font(candidates, size):
    for path in candidates:
        if Path(path).exists():
            return ImageFont.truetype(path, size)
    return ImageFont.load_default()


def logo(size):
    return Image.open(ICON).convert("RGBA").resize((size, size), Image.LANCZOS)


def sidebar():
    width, height = 164, 314
    top, bottom = (29, 63, 158), (12, 30, 84)  # bleu du logo vers un bleu plus profond
    image = Image.new("RGB", (width, height))
    draw = ImageDraw.Draw(image)
    for y in range(height):
        t = y / (height - 1)
        draw.line([(0, y), (width, y)], fill=tuple(round(a + (b - a) * t) for a, b in zip(top, bottom)))

    mark = logo(116)
    image.paste(mark, ((width - 116) // 2, 34), mark)

    title_font, text_font = font(FONTS_BOLD, 24), font(FONTS_REGULAR, 12)
    draw.text((width / 2, 176), "Multitool", font=title_font, fill="white", anchor="mm")
    for i, line in enumerate(["Plusieurs utilitaires,", "un seul logiciel."]):
        draw.text((width / 2, 208 + i * 17), line, font=text_font, fill=(214, 226, 255), anchor="mm")
    return image


def header():
    width, height = 150, 57
    image = Image.new("RGB", (width, height), "white")
    mark = logo(44)
    image.paste(mark, (width - 44 - 8, (height - 44) // 2), mark)
    return image


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    sidebar().save(OUT / "sidebar.bmp", format="BMP")
    header().save(OUT / "header.bmp", format="BMP")
    print(f"Images écrites dans {OUT}")
