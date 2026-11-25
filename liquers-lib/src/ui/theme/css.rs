//! The theme as CSS custom properties, for the browser renderer.

use std::fmt::Write;

use super::model::{Density, Palette, Spacing, SyntaxRole, Theme, ThemeMode};

fn palette_vars(out: &mut String, p: &Palette, theme: &Theme, mode: ThemeMode) {
    let s = &p.surface;
    let t = &p.text;
    let st = &p.state;
    let vars = [
        ("bg", s.bg),
        ("panel", s.panel),
        ("card", s.card),
        ("card-head", s.card_head),
        ("control", s.control),
        ("control-hover", s.control_hover),
        ("line", s.line),
        ("line-strong", s.line_strong),
        ("shadow", s.shadow),
        ("ink", t.ink),
        ("ink-muted", t.ink_muted),
        ("ink-faint", t.ink_faint),
        ("on-accent", t.on_accent),
        ("accent", st.accent),
        ("accent-wash", st.accent_wash),
        ("focus-ring", st.focus_ring),
        ("danger", st.danger),
        ("warning", st.warning),
        ("success", st.success),
        ("modified", st.modified),
        ("preview", st.preview),
    ];
    for (name, colour) in vars {
        let _ = write!(out, "--lq-{name}:{colour};");
    }
    for role in SyntaxRole::ALL {
        let _ = write!(out, "--lq-syntax-{}:{};", role.name(), p.syntax.get(role));
    }
    for (name, hue) in &theme.hues {
        let _ = write!(out, "--lq-hue-{name}:{};", hue.get(mode));
    }
}

fn spacing_vars(out: &mut String, s: &Spacing) {
    let vars = [
        ("control-height", s.control_height),
        ("row-gap", s.row_gap),
        ("card-gap", s.card_gap),
        ("card-radius", s.card_radius),
        ("card-padding", s.card_padding),
        ("label-width", s.label_width),
        ("icon-size", s.icon_size),
    ];
    for (name, v) in vars {
        let _ = write!(out, "--lq-{name}:{v}px;");
    }
}

impl Theme {
    /// The theme as CSS: custom properties for the dark palette on `:root` and
    /// `[data-lq-theme="dark"]`, the light one on `[data-lq-theme="light"]` and under
    /// `prefers-color-scheme: light` when no mode is forced, the densities on
    /// `[data-lq-density]`, typography, and one `.lq-tok-<role>` rule per query token role.
    pub fn to_css(&self) -> String {
        let mut out = String::new();
        out.push_str(":root,[data-lq-theme=\"dark\"]{");
        palette_vars(&mut out, &self.dark, self, ThemeMode::Dark);
        out.push_str("}\n[data-lq-theme=\"light\"]{");
        palette_vars(&mut out, &self.light, self, ThemeMode::Light);
        out.push_str("}\n@media (prefers-color-scheme: light){:root:not([data-lq-theme=\"dark\"]){");
        palette_vars(&mut out, &self.light, self, ThemeMode::Light);
        out.push_str("}}\n:root,[data-lq-density=\"comfortable\"]{");
        spacing_vars(&mut out, self.spacing(Density::Comfortable));
        out.push_str("}\n[data-lq-density=\"compact\"]{");
        spacing_vars(&mut out, self.spacing(Density::Compact));
        let ty = &self.typography;
        let _ = write!(
            out,
            "}}\n:root{{--lq-font-ui:{};--lq-font-mono:{};--lq-size-body:{}px;--lq-size-small:{}px;--lq-size-title:{}px;}}\n",
            ty.font_ui, ty.font_mono, ty.size_body, ty.size_small, ty.size_title
        );
        for role in SyntaxRole::ALL {
            let _ = writeln!(out, ".lq-tok-{0}{{color:var(--lq-syntax-{0})}}", role.name());
        }
        out
    }
}
