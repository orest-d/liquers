# Mockup sources

`gen.py` writes one HTML page per mockup into `html/`; `shot.js` renders them to PNG with
Playwright (`node shot.js 2` for 2x). The icons come from two npm packages unpacked beside this
folder's parent as `icons/`: `npm pack @phosphor-icons/core@2.1.1` (extracted to `icons/ph/`) and
`npm pack lucide-static@0.460.0` (extracted to `icons/`). The committed PNGs were rendered at 2x
and reduced to 256 colours.
