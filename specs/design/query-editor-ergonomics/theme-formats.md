# Theme file formats: YAML, TOML, CSS variables, or DTCG JSON

Supporting document of `query-editor-ergonomics`, written 2026-11-21 for Orest's question at the
Phase 1 gate: what a theme file would look like in each format, and which one best serves the
longer goal of **one theme reused by every Liquers frontend**, mainly egui, the browser and a
ratatui terminal UI. It answers Phase 1's open question 1.

## 1. What a theme has to hold

Whatever the syntax, every frontend needs the same five things. The format question is about how
well each syntax carries them.

| Part | Example | Notes per frontend |
|---|---|---|
| **Palette**: raw colours | `blue-500: #2d81ff` | All three |
| **Modes**: semantic roles per mode | `dark.accent → blue-500`, `light.accent → blue-600` | All three; a terminal also needs a fallback when only 16 or 256 colours exist |
| **Hues** for namespaces | `teal: dark #2fb3a3, light #1d8a7d, ansi cyan` | Registry metadata names a hue, the theme maps it (`NAMESPACES-HAVE-NO-METADATA`) |
| **Density**: sizes | `comfortable: control 28 px, gap 8 px` | egui and web in pixels; a terminal in cells and blank lines; radius and shadow do not exist there |
| **Overrides** by namespace and command | `pl: icon table, hue teal`; `pl/gt: icon funnel` | AC-14; later, selector rules (`NO-RENDERER-NEUTRAL-STYLESHEET`) |

Typography (font families, sizes) applies to egui and web; a terminal has one font and only bold,
italic, underline and reverse.

## 2. The same theme in four formats

Every example below holds the same content, cut down to a few tokens of each kind.

### 2.1 YAML

```yaml
# liquers-theme.yaml
name: liquers-default
palette:
  ink-50: "#f2f1ee"
  gray-900: "#121316"
  gray-800: "#1b1c21"
  gray-100: "#f4f4f2"
  blue-500: "#2d81ff"
  blue-600: "#1f6fe5"
  red-500: "#e2475a"
modes:
  dark:
    bg: gray-900
    card: gray-800
    ink: ink-50
    accent: blue-500
    danger: red-500
  light:
    bg: gray-100
    card: "#ffffff"
    ink: "#1d1e22"
    accent: blue-600
    danger: red-500
hues:
  teal:   { dark: "#2fb3a3", light: "#1d8a7d", ansi: cyan }
  violet: { dark: "#9b6cff", light: "#7b45d6", ansi: magenta }
densities:
  comfortable: { control: 28px, gap: 8px, radius: 8px, terminal: { gap: 1 } }
  compact:     { control: 23px, gap: 4px, radius: 6px, terminal: { gap: 0 } }
fonts:
  ui: [Inter, system-ui]
  mono: [Geist Mono, DejaVu Sans Mono]
terminal:            # fallbacks when the terminal has no true colour
  accent: blue
  danger: red
namespaces:
  pl:    { icon: table, hue: teal }
  img:   { icon: image, hue: violet }
commands:
  pl/gt: { icon: funnel }
```

### 2.2 TOML

```toml
# liquers-theme.toml
name = "liquers-default"

[palette]
ink-50 = "#f2f1ee"
gray-900 = "#121316"
gray-800 = "#1b1c21"
gray-100 = "#f4f4f2"
blue-500 = "#2d81ff"
blue-600 = "#1f6fe5"
red-500 = "#e2475a"

[modes.dark]
bg = "gray-900"
card = "gray-800"
ink = "ink-50"
accent = "blue-500"
danger = "red-500"

[modes.light]
bg = "gray-100"
card = "#ffffff"
ink = "#1d1e22"
accent = "blue-600"
danger = "red-500"

[hues]
teal = { dark = "#2fb3a3", light = "#1d8a7d", ansi = "cyan" }
violet = { dark = "#9b6cff", light = "#7b45d6", ansi = "magenta" }

[densities.comfortable]
control = "28px"
gap = "8px"
radius = "8px"
terminal = { gap = 1 }

[densities.compact]
control = "23px"
gap = "4px"
radius = "6px"
terminal = { gap = 0 }

[fonts]
ui = ["Inter", "system-ui"]
mono = ["Geist Mono", "DejaVu Sans Mono"]

[terminal]
accent = "blue"
danger = "red"

[namespaces]
pl = { icon = "table", hue = "teal" }
img = { icon = "image", hue = "violet" }

[commands]
"pl/gt" = { icon = "funnel" }     # a key containing "/" must be quoted
```

### 2.3 CSS custom properties

```css
/* liquers-theme.css */
:root {
  --lq-palette-ink-50: #f2f1ee;
  --lq-palette-gray-900: #121316;
  --lq-palette-gray-800: #1b1c21;
  --lq-palette-gray-100: #f4f4f2;
  --lq-palette-blue-500: #2d81ff;
  --lq-palette-blue-600: #1f6fe5;
  --lq-palette-red-500: #e2475a;
  --lq-font-ui: Inter, system-ui;
  --lq-font-mono: "Geist Mono", "DejaVu Sans Mono";
}
:root, [data-lq-mode="dark"] {
  --lq-bg: var(--lq-palette-gray-900);
  --lq-card: var(--lq-palette-gray-800);
  --lq-ink: var(--lq-palette-ink-50);
  --lq-accent: var(--lq-palette-blue-500);
  --lq-danger: var(--lq-palette-red-500);
  --lq-hue-teal: #2fb3a3;
  --lq-hue-violet: #9b6cff;
}
[data-lq-mode="light"] {
  --lq-bg: var(--lq-palette-gray-100);
  --lq-card: #ffffff;
  --lq-ink: #1d1e22;
  --lq-accent: var(--lq-palette-blue-600);
  --lq-hue-teal: #1d8a7d;
  --lq-hue-violet: #7b45d6;
}
[data-lq-density="comfortable"] { --lq-control: 28px; --lq-gap: 8px; --lq-radius: 8px; --lq-term-gap: 1; }
[data-lq-density="compact"]     { --lq-control: 23px; --lq-gap: 4px; --lq-radius: 6px; --lq-term-gap: 0; }

/* terminal fallbacks: meaningless to a browser, read only by the TUI */
:root { --lq-term-accent: blue; --lq-term-danger: red; --lq-term-hue-teal: cyan; --lq-term-hue-violet: magenta; }

/* overrides by namespace and command */
[data-lq-ns="pl"]  { --lq-icon: "table"; --lq-hue: var(--lq-hue-teal); }
[data-lq-ns="img"] { --lq-icon: "image"; --lq-hue: var(--lq-hue-violet); }
[data-lq-command="pl/gt"] { --lq-icon: "funnel"; }
```

### 2.4 DTCG JSON (W3C Design Tokens Format Module 2025.10)

The design-tokens format reached its first stable version on 2025-10-28: JSON files of tokens with
`$value` and `$type`, groups, and aliases in braces (`"{color.blue-500}"`). Its **Resolver
Module** selects between files by *modifiers* such as `theme: light | dark`. Token files are
`.tokens.json`, resolver files `.resolver.json`.

```json
// tokens/base.tokens.json  (JSON has no comments; shown here for orientation only)
{
  "color": {
    "$type": "color",
    "blue-500": { "$value": { "colorSpace": "srgb", "components": [0.176, 0.506, 1], "hex": "#2d81ff" } },
    "gray-900": { "$value": { "colorSpace": "srgb", "components": [0.071, 0.075, 0.086], "hex": "#121316" } }
  },
  "font": {
    "ui":   { "$type": "fontFamily", "$value": ["Inter", "system-ui"] },
    "mono": { "$type": "fontFamily", "$value": ["Geist Mono", "DejaVu Sans Mono"] }
  }
}
// tokens/dark.tokens.json
{ "role": { "$type": "color",
    "bg":     { "$value": "{color.gray-900}" },
    "accent": { "$value": "{color.blue-500}" } },
  "hue": { "$type": "color",
    "teal": { "$value": { "colorSpace": "srgb", "components": [0.184, 0.702, 0.639], "hex": "#2fb3a3" },
              "$extensions": { "dev.liquers": { "ansi": "cyan" } } } } }
// tokens/compact.tokens.json
{ "size": { "$type": "dimension",
    "control": { "$value": { "value": 23, "unit": "px" } },
    "gap":     { "$value": { "value": 4,  "unit": "px" },
                 "$extensions": { "dev.liquers": { "cells": 0 } } } } }
// liquers.resolver.json
{
  "version": "2025-10-01",
  "sets": { "base": { "sources": [{ "$ref": "tokens/base.tokens.json" }] } },
  "modifiers": {
    "theme":   { "contexts": { "dark": [{ "$ref": "tokens/dark.tokens.json" }],
                               "light": [{ "$ref": "tokens/light.tokens.json" }] }, "default": "dark" },
    "density": { "contexts": { "comfortable": [{ "$ref": "tokens/comfortable.tokens.json" }],
                               "compact": [{ "$ref": "tokens/compact.tokens.json" }] }, "default": "comfortable" }
  },
  "resolutionOrder": [{ "$ref": "#/sets/base" }, { "$ref": "#/modifiers/theme" }, { "$ref": "#/modifiers/density" }]
}
```

Namespace and command overrides have no place in the standard: they would be
`$extensions` or a separate Liquers file.

## 3. How each frontend consumes a theme

| Frontend | Native styling | Reading a YAML/TOML/DTCG theme | Reading a CSS-variables theme |
|---|---|---|---|
| **egui** | `Style`/`Visuals` set in code, per `Ui` scope | serde into a token struct; `apply(ctx)` | Needs a parser (`cssparser`) for declarations, and our own reading of `[data-…]` blocks as modes |
| **Browser** (`ui/web`) | CSS | Generate CSS custom properties (`to_css()`) | Used as is |
| **ratatui** | `Style` (fg, bg, modifiers) in code; truecolor or indexed colours | serde; map colours with an `ansi` fallback, sizes to cells | Parser as for egui; `--lq-term-*` variables are dead weight in the browser |
| iced, Slint, Dioxus native, Xilem | Theme or palette structs in code (Slint has `.slint` globals) | serde, then code | Parser |
| GTK, Qt | GTK CSS; Qt QSS (CSS-like) | Generate GTK CSS or QSS | GTK could load it nearly as is; QSS has no custom properties |
| Python Textual (if a Python TUI appears) | Textual CSS with `$variables` | Generate TCSS | Close, but its own dialect |

Precedents among apps that theme both GUI and terminal: Helix (terminal editor) and Alacritty use
**TOML**; Zed and VS Code use **JSON**; base16/tinted-theming schemes use **YAML** and generate
dozens of targets from one file; design systems use **DTCG JSON** with a generator (Style
Dictionary) emitting CSS, iOS, Android and more. In Rust, young crates already parse DTCG for egui
and ratatui (for example `egui-pop-tokens`, `creo-ui`, `theme-kit`, all 2026), which shows the
direction but none is mature enough to depend on.

## 4. Comparison

| | YAML | TOML | CSS variables | DTCG JSON |
|---|---|---|---|---|
| Liquers fit | **Best**: recipes, store and environment configs are YAML; `serde_yaml` already a dependency | Good: `toml` already used | Foreign to the rest of Liquers configuration | New format; plain `serde_json` |
| Hand editing | Good; comments; nested maps read well | Good for flat tables; deep nesting becomes many `[a.b.c]` headers; keys with `/` must be quoted | Good for web developers; verbose (`--lq-` prefixes, `var()`) | Verbose; no comments; colour objects are long |
| Modes (dark/light) and density | Plain keys | Plain keys | Selector blocks (`[data-lq-mode]`), natural in a browser | **Standard** resolver modifiers |
| References between tokens | Our convention (`accent: blue-500`) | Same | `var()` (standard) | `{color.blue-500}` (standard) |
| Terminal fallbacks, cells | A `terminal:` section | Same | Variables the browser ignores | `$extensions` |
| Overrides by namespace/command | Plain maps | Plain maps (quoted keys) | Attribute selectors, natural; matches the future selector stylesheet | `$extensions` or a side file |
| Validation | serde types; a JSON Schema can check it in editors | Same | Only by our parser; browsers ignore unknown values silently | Standard schema and tools |
| Browser | Generated CSS | Generated CSS | **Native** | Generated CSS |
| egui, ratatui | **serde, direct** | **serde, direct** | Needs a CSS parser and a subset reader | serde, plus alias resolution and the resolver logic |
| Interoperability with design tools (Figma, Tokens Studio, Style Dictionary) | None directly | None | Partial (web only) | **Best** |
| Risk | YAML gotchas (unquoted `#` starts a comment, so colours need quotes; `no` is a boolean in YAML 1.1) | Few | Users expect real CSS (selectors, cascade) to work in egui and TUI | Spec is new (2025.10); Rust support immature |

## 5. Recommendation

1. **One token model in Rust is the source of truth**, a serde struct with the five parts of §1,
   including terminal fallbacks. Every frontend reads that struct, never a file syntax. This is
   what makes the theme reusable: egui, ratatui and any later native frontend map tokens in code,
   and the browser gets generated CSS.
2. **Author it in YAML** (TOML reads the same through serde, so accepting both costs one match on
   the file extension). YAML matches the rest of Liquers, nests well for modes and overrides, and
   allows comments. Quote colours.
3. **Generate CSS variables** from it (`to_css()`, as in §2.3) for the browser, rather than
   making CSS the source: CSS would force egui and ratatui to parse a language they cannot honour.
4. **Keep DTCG as the interchange format** for a later theme design: an importer and exporter
   (`.tokens.json` and a resolver file) would let designers bring themes from Figma or Style
   Dictionary. The token model above maps onto DTCG one to one (palette and roles as colour
   tokens, densities as dimension tokens, modes and densities as resolver modifiers, terminal
   fallbacks and overrides as `$extensions`), so nothing has to change in the model to add it.
5. The selector rules of `NO-RENDERER-NEUTRAL-STYLESHEET` are a later layer over the same tokens,
   and may well use CSS *selector* syntax for the rules while values stay tokens.

For this design only items 1 to 3 are in scope, with the keys of §2.1.

## Sources

- W3C Design Tokens Community Group,
  [Design Tokens Format Module 2025.10](https://www.w3.org/community/reports/design-tokens/CG-FINAL-format-20251028/)
  and [Resolver Module 2025.10](https://www.w3.org/community/reports/design-tokens/CG-FINAL-resolver-20251028/);
  [announcement of the first stable version](https://w3.org/community/design-tokens/2025/10/28/design-tokens-specification-reaches-first-stable-version/)
- crates.io search for "design tokens dtcg", 2026-11-21
- `reference-design.md` §4.1 and §4.2 for egui and CSS
