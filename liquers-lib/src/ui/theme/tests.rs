//! Tests of the theme model (`specs/design/query-editor-ergonomics/phase3-examples.md`).

use liquers_core::command_metadata::CommandKey;
use liquers_core::query::StyledQueryToken;
use serde_json::json;

use super::*;

const BUILTIN_YAML: &str = include_str!("builtin.yaml");

// Test 16 (AC-11): the built-in theme has both modes and both densities, and builtin.yaml is the
// same theme written out.
#[test]
fn builtin_dark_light_and_densities() {
    let theme = Theme::builtin();
    assert_ne!(theme.palette(ThemeMode::Dark).surface.bg, theme.palette(ThemeMode::Light).surface.bg);
    assert!(theme.spacing(Density::Compact).control_height < theme.spacing(Density::Comfortable).control_height);
    assert!(theme.unknown_keys().is_empty());

    let written = serde_yaml::to_string(theme.as_ref()).expect("yaml");
    if std::env::var("LIQUERS_BLESS").is_ok() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/ui/theme/builtin.yaml");
        std::fs::write(path, format!("# The built-in theme, generated from builtin.rs (LIQUERS_BLESS=1).\n{written}"))
            .expect("write");
        return;
    }
    let fixture: serde_json::Value = serde_yaml::from_str(BUILTIN_YAML).expect("fixture parses");
    assert_eq!(fixture, serde_json::to_value(theme.as_ref()).expect("json"), "run with LIQUERS_BLESS=1");
    let composed = Theme::compose(&[ThemeLayer::from_yaml(BUILTIN_YAML).expect("layer")]).expect("compose");
    assert_eq!(serde_json::to_value(&composed).ok(), serde_json::to_value(theme.as_ref()).ok());
}

// Test 20 (AC-15): every highlighter token has a role and a colour.
#[test]
fn syntax_role_of_every_token() {
    let cases = [
        (StyledQueryToken::Header("-R".into()), SyntaxRole::Header),
        (StyledQueryToken::ResourceName("data".into()), SyntaxRole::Resource),
        (StyledQueryToken::Separator("/".into()), SyntaxRole::Separator),
        (StyledQueryToken::ActionName("head".into()), SyntaxRole::Action),
        (StyledQueryToken::StringParameter("20".into()), SyntaxRole::Parameter),
        (StyledQueryToken::Entity("~".into()), SyntaxRole::Entity),
        (StyledQueryToken::Highlight("x".into()), SyntaxRole::Highlight),
    ];
    let theme = Theme::builtin();
    for (token, role) in cases {
        assert_eq!(SyntaxRole::from(&token), role);
        assert_eq!(theme.syntax(ThemeMode::Dark, role), theme.dark.syntax.get(role));
    }
    let css = theme.to_css();
    for role in SyntaxRole::ALL {
        assert!(css.contains(&format!(".lq-tok-{}{{", role.name())), "{role:?}");
    }
}

// Test 22 (AC-16): layers override and add, unknown keys are kept, merging associates.
#[test]
fn layers_override_add_and_associate() {
    let a = ThemeLayer::from_yaml("dark: { surface: { card: \"#101010\" } }\nhues: { mint: { dark: \"#00ff99\", light: \"#008855\" } }").expect("a");
    let b = ThemeLayer::from_yaml("dark: { surface: { card: \"#202020\", panel: \"#303030\" } }\nextra: { x: 1 }").expect("b");
    let c = ThemeLayer::from_yaml("extra: { y: 2 }\nicons: { commands: { pl/gt: { icon: ph:funnel, hue: mint } } }").expect("c");

    let all = Theme::compose(&[a.clone(), b.clone(), c.clone()]).expect("compose");
    assert_eq!(all.dark.surface.card, Rgba::rgb(0x20, 0x20, 0x20));
    assert_eq!(all.dark.surface.panel, Rgba::rgb(0x30, 0x30, 0x30));
    assert_eq!(all.dark.surface.bg, Theme::builtin().dark.surface.bg);
    assert_eq!(all.hue("mint", ThemeMode::Light), Some(Rgba::rgb(0, 0x88, 0x55)));
    assert_eq!(all.hue("teal", ThemeMode::Dark), Theme::builtin().hue("teal", ThemeMode::Dark));
    assert_eq!(all.lookup("extra.x"), Some(&json!(1)));
    assert_eq!(all.lookup("extra.y"), Some(&json!(2)));
    assert_eq!(all.unknown_keys(), vec!["extra".to_string()]);

    let stepwise = Theme::compose(&[a]).and_then(|t| t.with_layer(&b)).and_then(|t| t.with_layer(&c)).expect("stepwise");
    assert_eq!(stepwise, all);
    let mut ab = b.clone();
    ab.merge(&c);
    assert_eq!(Theme::compose(&[ThemeLayer::default()]).and_then(|t| t.with_layer(&ab)).ok().map(|t| t.lookup("extra.y").cloned()), Some(Some(json!(2))));

    // A bad colour or a known key of the wrong type is refused.
    for bad in ["dark: { surface: { card: red } }", "comfortable: { row-gap: wide }", "icons: [1]"] {
        let layer = ThemeLayer::from_yaml(bad).expect("parses as YAML");
        let err = Theme::compose(&[layer]).expect_err(bad);
        assert_eq!(err.error_type, liquers_core::error::ErrorType::ConfigurationError, "{bad}");
    }
    assert!(ThemeLayer::from_yaml("- 1\n- 2").is_err());
}

// Test 25 (AC-17): string forms, the structured form, lib: checks and the Unicode fallback.
#[test]
fn icon_variants_and_unicode_fallback() {
    assert_eq!(Icon::parse("funnel").expect("bare"), Icon::set("funnel"));
    assert_eq!(Icon::parse("ph:funnel").expect("ph"), Icon::set("funnel"));
    assert_eq!(Icon::parse("u:⏷").expect("u"), Icon::unicode("⏷"));
    assert_eq!(Icon::parse("lib:brush").expect("lib").variants, vec![IconVariant::Library("brush".into())]);
    assert!(matches!(Icon::parse("-R/icons/brush.svg").expect("svg").variants[0], IconVariant::Svg(IconSource::Key(_))));
    assert!(matches!(Icon::parse("icons/brush.PNG").expect("png").variants[0], IconVariant::Raster(IconSource::Key(_))));
    assert!(Icon::parse("bad name!").is_err());
    assert!(Icon::parse("u:").is_err());

    let icon: Icon = serde_yaml::from_str("{ unicode: \"🖌\", variants: [ ph:paint-brush, { svg: { inline: \"<svg/>\" } }, { raster: { inline: \"iVBORw0KGgo=\" } } ] }").expect("full");
    assert_eq!(icon.unicode.as_deref(), Some("🖌"));
    assert_eq!(icon.variants.len(), 3);
    assert_eq!(icon.variants[1], Icon::embedded_svg(b"<svg/>"));
    let back: Icon = serde_json::from_value(serde_json::to_value(&icon).expect("ser")).expect("de");
    assert_eq!(back, icon);
    let list: Icon = serde_yaml::from_str("[ph:funnel, \"u:⏷\"]").expect("list");
    assert_eq!(list, Icon::set("funnel").with_unicode("⏷"));
    assert_eq!(Icon::from_hint(&json!("rows")).expect("hint"), Icon::set("rows"));

    let ok = ThemeLayer::from_yaml("icons: { library: { brush: { unicode: \"🖌\", variants: [ph:paint-brush] } }, commands: { img/draw: { icon: lib:brush } } }").expect("layer");
    let theme = Theme::compose(&[ok]).expect("compose");
    let draw = CommandKey::new("", "img", "draw");
    let r = theme.icon_for(&draw, "Draw", None, None);
    assert_eq!(r.variants[0], &IconVariant::Set { set: PHOSPHOR.into(), name: "paint-brush".into() });
    assert_eq!(r.unicode, "🖌");
    // The namespace default follows: `img` → image.
    assert!(r.variants.iter().any(|v| **v == IconVariant::Set { set: PHOSPHOR.into(), name: "image".into() }));

    let missing = ThemeLayer::from_yaml("icons: { commands: { img/draw: { icon: lib:nope } } }").expect("layer");
    assert!(Theme::compose(&[missing]).is_err());
    let chain = ThemeLayer::from_yaml("icons: { library: { a: lib:b, b: ph:x } }").expect("layer");
    assert!(Theme::compose(&[chain]).is_err());

    // No candidate anywhere: the monogram.
    let user = CommandKey::new("", "mine", "frobnicate");
    let builtin = Theme::builtin();
    let r = builtin.icon_for(&user, "Frobnicate", None, None);
    assert!(r.variants.is_empty());
    assert_eq!(r.unicode, "F");
}

// Test 11 (AC-8, AC-14): precedence theme command > theme namespace > hint > namespace default.
#[test]
fn icon_precedence() {
    let key = CommandKey::new("", "pl", "gt");
    let hint = Icon::set("funnel");
    let first = |t: &Theme, hint: Option<&Icon>| t.icon_for(&key, "Greater than", hint, None).variants.first().map(|v| (*v).clone());
    let ph = |n: &str| Some(IconVariant::Set { set: PHOSPHOR.into(), name: n.into() });

    let builtin = Theme::builtin();
    assert_eq!(first(&builtin, None), ph("table"));          // namespace default
    assert_eq!(first(&builtin, Some(&hint)), ph("funnel"));  // the hint beats it
    assert_eq!(builtin.icon_for(&key, "Greater than", Some(&hint), None).hue, Some("teal"));

    let ns = Theme::compose(&[ThemeLayer::from_yaml("icons: { namespaces: { pl: { icon: ph:rows, hue: blue } } }").expect("l")]).expect("c");
    assert_eq!(first(&ns, Some(&hint)), ph("rows"));         // theme namespace beats the hint
    assert_eq!(ns.icon_for(&key, "x", Some(&hint), None).hue, Some("blue"));

    let cmd = ns.with_layer(&ThemeLayer::from_yaml("icons: { commands: { pl/gt: { icon: ph:funnel-simple } } }").expect("l")).expect("c");
    assert_eq!(first(&cmd, Some(&hint)), ph("funnel-simple"));

    let realm = cmd.with_layer(&ThemeLayer::from_yaml("icons: { commands: { \"r1:pl/gt\": { icon: ph:hash } } }").expect("l")).expect("c");
    assert_eq!(realm.icon_for(&CommandKey::new("r1", "pl", "gt"), "x", None, None).variants[0], &ph("hash").expect("v"));
    assert_eq!(first(&realm, None), ph("funnel-simple"));    // the realm entry applies only in its realm

    // An unknown Phosphor name is kept as a candidate; renderers skip it (is_known_set_icon).
    let unknown = IconVariant::Set { set: PHOSPHOR.into(), name: "no-such-icon".into() };
    assert!(!is_known_set_icon(&unknown));
    assert!(is_known_set_icon(&ph("funnel").expect("v")));
}

// Test 27 (AC-14, AC-17): the embedded name list is egui-phosphor's.
#[cfg(feature = "egui")]
#[test]
fn phosphor_name_list_matches_egui_phosphor() {
    let mut from_crate: Vec<String> = egui_phosphor::regular::ICONS
        .iter()
        .map(|(name, _)| name.to_lowercase().replace('_', "-"))
        .collect();
    from_crate.sort();
    let listed: Vec<String> = include_str!("icons.txt").lines().filter(|l| !l.is_empty()).map(str::to_string).collect();
    assert_eq!(listed, from_crate);
    for name in ["rows", "funnel", "funnel-simple", "sort-ascending", "columns", "table", "image"] {
        assert!(is_icon_name(name), "{name}");
    }
}

// Test 24 (AC-16): the `ui` section of a configuration document.
#[test]
fn ui_section_parses_and_refuses_typos() {
    use liquers_core::environment_config::ConfigDocument;
    let mut doc = ConfigDocument::from_yaml(
        "ui:\n  theme: -R/config/theme.yaml\n  mode: light\n  density: compact\n",
    )
    .expect("doc");
    let ui: UiConfig = doc.take_section("ui").expect("section").expect("present");
    assert_eq!(ui.theme, Some(ThemeSource::Query("-R/config/theme.yaml".into())));
    assert_eq!(ui.initial_mode(true), ThemeMode::Light);
    assert_eq!(ui.cell(true).get().density, Density::Compact);

    let builtin: UiConfig = serde_yaml::from_str("theme: default").expect("builtin");
    assert_eq!(builtin.theme, Some(ThemeSource::Builtin(BuiltinTheme::Default)));
    assert!(builtin.follows_system());
    assert_eq!(builtin.initial_mode(false), ThemeMode::Light);

    let mut typo = ConfigDocument::from_yaml("ui:\n  densty: compact\n").expect("doc");
    let err = typo.take_section::<UiConfig>("ui").expect_err("typo");
    assert_eq!(err.error_type, liquers_core::error::ErrorType::ConfigurationError);
    assert!(serde_yaml::from_str::<UiConfig>("theme: fancy").is_err());

    let mut none = ConfigDocument::from_yaml("{}").expect("doc");
    assert_eq!(none.take_section::<UiConfig>("ui").expect("ok"), None);
}

#[test]
fn theme_cell_bumps_generation_on_change_only() {
    let cell = ThemeCell::default();
    let g0 = cell.get().generation;
    cell.set_mode(ThemeMode::Dark);
    assert_eq!(cell.get().generation, g0);
    cell.set_mode(ThemeMode::Light);
    cell.set_density(Density::Compact);
    cell.set_theme(Theme::builtin());
    let s = cell.get();
    assert_eq!((s.mode, s.density, s.generation), (ThemeMode::Light, Density::Compact, g0 + 3));
    let shared = cell.clone();
    shared.set_mode(ThemeMode::Dark);
    assert_eq!(cell.get().mode, ThemeMode::Dark);

    let icons = IconCache::default();
    assert_eq!(icons.get_or_mark("-R/i.svg").1, true);
    assert_eq!(icons.get_or_mark("-R/i.svg").1, false);
    icons.set("-R/i.svg", IconLoad::Ready(std::sync::Arc::from(&b"<svg/>"[..])));
    assert!(icons.get_or_mark("-R/i.svg").0.is_some());
}
