# App Icons

Replace these placeholder files with proper icons before release.

## Required Sizes

| File | Size | Use |
|------|------|-----|
| `icon.svg` | Vector | Source |
| `32x32.png` | 32×32 | Windows taskbar |
| `128x128.png` | 128×128 | macOS dock, Linux |
| `128x128@2x.png` | 256×256 | macOS Retina |
| `icon.icns` | Multi | macOS app bundle |
| `icon.ico` | Multi | Windows installer |

## Generation

```bash
# From SVG (requires ImageMagick)
magick icon.svg -resize 32x32 32x32.png
magick icon.svg -resize 128x128 128x128.png
magick icon.svg -resize 256x256 128x128@2x.png

# Create .icns (macOS)
mkdir -p icon.iconset
magick icon.svg -resize 16x16 icon.iconset/icon_16x16.png
magick icon.svg -resize 32x32 icon.iconset/icon_16x16@2x.png
magick icon.svg -resize 32x32 icon.iconset/icon_32x32.png
magick icon.svg -resize 64x64 icon.iconset/icon_32x32@2x.png
magick icon.svg -resize 128x128 icon.iconset/icon_128x128.png
magick icon.svg -resize 256x256 icon.iconset/icon_128x128@2x.png
magick icon.svg -resize 256x256 icon.iconset/icon_256x256.png
magick icon.svg -resize 512x512 icon.iconset/icon_256x256@2x.png
magick icon.svg -resize 512x512 icon.iconset/icon_512x512.png
magick icon.svg -resize 1024x1024 icon.iconset/icon_512x512@2x.png
iconutil -c icns icon.iconset

# Create .ico (Windows)
magick icon.svg -define icon:auto-resize=16,32,48,64,128,256 icon.ico
```

## Design Guidelines

- **Style**: Minimal, monochrome, readable at 16px
- **Colors**: Dark theme `#0E1116` bg + `#2DD4BF` teal accent
- **No**: Gradients, shadows, 3D effects, stock clipart
- **Concept**: Arrow up (recovery) + circle (data) + checkmark (success)

## Current Placeholder

The `icon.svg` is a minimal vector placeholder. Replace with professional icon before v1.0.0 release.