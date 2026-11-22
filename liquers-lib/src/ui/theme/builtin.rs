//! The built-in theme, built in Rust so it cannot fail to load. `builtin.yaml` is the same theme
//! written out; a test keeps the two equal (`LIQUERS_BLESS=1` rewrites the file).

use std::collections::BTreeMap;

use super::icon::{Icon, IconOverride, IconTheme, PHOSPHOR};
use super::model::{
    Hue, Palette, Rgba, Spacing, StateRoles, SurfaceRoles, SyntaxRoles, TextRoles, Theme, Typography,
};

const fn c(hex: u32) -> Rgba {
    Rgba::rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}
const fn ca(hex: u32, alpha: u8) -> Rgba {
    Rgba::rgba((hex >> 16) as u8, (hex >> 8) as u8, hex as u8, alpha)
}

fn dark() -> Palette {
    Palette {
        surface: SurfaceRoles {
            bg: c(0x121316),
            panel: c(0x16171b),
            card: c(0x1b1c21),
            card_head: c(0x202127),
            control: c(0x26272d),
            control_hover: c(0x2e3037),
            line: ca(0xffffff, 0x12),
            line_strong: ca(0xffffff, 0x1f),
            shadow: ca(0x000000, 0x47),
        },
        text: TextRoles {
            ink: c(0xf2f1ee),
            ink_muted: c(0xa9abb3),
            ink_faint: c(0x6c6f79),
            on_accent: c(0xffffff),
        },
        state: StateRoles {
            accent: c(0x2d81ff),
            accent_wash: ca(0x2d81ff, 0x24),
            focus_ring: ca(0x2d81ff, 0x80),
            danger: c(0xe2475a),
            warning: c(0xe0a23a),
            success: c(0x3fb27f),
            modified: c(0xe0a23a),
        },
        syntax: SyntaxRoles {
            header: c(0x6f9fe8),
            resource: c(0x8ec5ff),
            separator: c(0x5d616b),
            namespace: c(0xc49bff),
            action: c(0xf2f1ee),
            parameter: c(0xf0b37e),
            entity: c(0xc5e243),
            link: c(0x7fd6a1),
            filename: c(0xe8d27a),
            plain: c(0xa9abb3),
            highlight: c(0xffd75e),
            error: c(0xe2475a),
        },
    }
}

fn light() -> Palette {
    Palette {
        surface: SurfaceRoles {
            bg: c(0xf4f4f2),
            panel: c(0xecebe8),
            card: c(0xffffff),
            card_head: c(0xfafaf8),
            control: c(0xf0efec),
            control_hover: c(0xe6e5e1),
            line: ca(0x000000, 0x14),
            line_strong: ca(0x000000, 0x24),
            shadow: ca(0x000000, 0x0d),
        },
        text: TextRoles {
            ink: c(0x1d1e22),
            ink_muted: c(0x55585f),
            ink_faint: c(0x8b8e96),
            on_accent: c(0xffffff),
        },
        state: StateRoles {
            accent: c(0x1f6fe5),
            accent_wash: ca(0x1f6fe5, 0x1a),
            focus_ring: ca(0x1f6fe5, 0x80),
            danger: c(0xc8364a),
            warning: c(0xb97a14),
            success: c(0x2a8a5f),
            modified: c(0xb97a14),
        },
        syntax: SyntaxRoles {
            header: c(0x3b6fb8),
            resource: c(0x1f63c6),
            separator: c(0xa3a6ad),
            namespace: c(0x7b45d6),
            action: c(0x1d1e22),
            parameter: c(0xb4570f),
            entity: c(0x6f8a12),
            link: c(0x1d8a4f),
            filename: c(0x8a6d0b),
            plain: c(0x55585f),
            highlight: c(0xb58900),
            error: c(0xc8364a),
        },
    }
}

/// Named hues: (name, dark, light).
const HUES: [(&str, u32, u32); 10] = [
    ("teal", 0x2fb3a3, 0x16857a),
    ("violet", 0x9b6cff, 0x6f45d6),
    ("blue", 0x4c9dff, 0x1f63c6),
    ("grey", 0x8a8f9c, 0x6a6f7b),
    ("orange", 0xd9893b, 0xb0661c),
    ("sky", 0x5fa8d3, 0x2f7aa8),
    ("pink", 0xd35fa8, 0xa83a7f),
    ("indigo", 0x7f8cff, 0x4f5bd6),
    ("olive", 0xb9a24a, 0x8a7620),
    ("amber", 0xe0a23a, 0xb97a14),
];

/// The bundled namespaces: (namespace, Phosphor icon, Unicode text, hue).
const NAMESPACES: [(&str, &str, &str, &str); 8] = [
    ("pl", "table", "▦", "teal"),
    ("img", "image", "▣", "violet"),
    ("rec", "list-bullets", "≡", "orange"),
    ("md", "markdown-logo", "¶", "sky"),
    ("mem", "brain", "◎", "pink"),
    ("lui", "app-window", "▭", "indigo"),
    ("template", "brackets-curly", "{", "olive"),
    ("", "gear", "⚙", "grey"),
];

/// The built-in theme.
pub(crate) fn builtin_theme() -> Theme {
    let hues = HUES
        .iter()
        .map(|(n, d, l)| (n.to_string(), Hue { dark: c(*d), light: c(*l) }))
        .collect();
    let defaults = NAMESPACES
        .iter()
        .map(|(ns, icon, unicode, hue)| {
            (
                ns.to_string(),
                IconOverride {
                    icon: Some(Icon::set(icon).with_unicode(*unicode)),
                    hue: Some(hue.to_string()),
                },
            )
        })
        .collect();
    Theme {
        name: "default".to_string(),
        dark: dark(),
        light: light(),
        comfortable: Spacing {
            control_height: 28.0,
            row_gap: 8.0,
            card_gap: 10.0,
            card_radius: 9.0,
            card_padding: 12.0,
            label_width: 140.0,
            icon_size: 24.0,
        },
        compact: Spacing {
            control_height: 22.0,
            row_gap: 4.0,
            card_gap: 6.0,
            card_radius: 7.0,
            card_padding: 8.0,
            label_width: 120.0,
            icon_size: 18.0,
        },
        typography: Typography {
            font_ui: "Inter, system-ui, sans-serif".to_string(),
            font_mono: "DejaVu Sans Mono, ui-monospace, monospace".to_string(),
            size_body: 13.0,
            size_small: 11.5,
            size_title: 15.0,
        },
        hues,
        icons: IconTheme {
            icon_set: PHOSPHOR.to_string(),
            library: BTreeMap::new(),
            namespaces: BTreeMap::new(),
            commands: BTreeMap::new(),
            defaults,
        },
        dict: serde_json::Map::new(),
    }
}
