use anyhow::{anyhow, Result};
use snowy_core::{i18n, EffectChoice, EffectKind};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

/// Menu state shown in the tray.
#[derive(Debug, Clone, PartialEq)]
pub struct TrayState {
    pub effect: EffectChoice,
    pub paused: bool,
    /// Resolved language code (see `i18n::LANGS`).
    pub lang: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TrayCommand {
    SetEffect(EffectChoice),
    TogglePause,
    /// Remove all piled snow, petals, leaves and snowballs.
    ClearSnow,
    BuildSnowman,
    AddTree,
    AddLights,
    SendPlough,
    CallYeti,
    OpenSettings,
    Quit,
}

const ID_SEASON: &str = "snowy.season";
const ID_RANDOM: &str = "snowy.random";
const ID_OFF: &str = "snowy.off";
const ID_PAUSE: &str = "snowy.pause";
const ID_CLEAR: &str = "snowy.clear";
const ID_SNOWMAN: &str = "snowy.snowman";
const ID_TREE: &str = "snowy.tree";
const ID_LIGHTS: &str = "snowy.lights";
const ID_PLOUGH: &str = "snowy.plough";
const ID_YETI: &str = "snowy.yeti";
const ID_SETTINGS: &str = "snowy.settings";
const ID_QUIT: &str = "snowy.quit";
const ID_FX_PREFIX: &str = "snowy.fx.";
const ID_WEATHER: &str = "snowy.weather";

fn choice_id(c: EffectChoice) -> String {
    match c {
        EffectChoice::Off => ID_OFF.into(),
        EffectChoice::Season => ID_SEASON.into(),
        EffectChoice::Random => ID_RANDOM.into(),
        EffectChoice::Weather => ID_WEATHER.into(),
        EffectChoice::Fixed(k) => format!("{ID_FX_PREFIX}{}", k.id()),
    }
}

fn parse_id(id: &str) -> Option<TrayCommand> {
    Some(match id {
        ID_SEASON => TrayCommand::SetEffect(EffectChoice::Season),
        ID_RANDOM => TrayCommand::SetEffect(EffectChoice::Random),
        ID_WEATHER => TrayCommand::SetEffect(EffectChoice::Weather),
        ID_OFF => TrayCommand::SetEffect(EffectChoice::Off),
        ID_PAUSE => TrayCommand::TogglePause,
        ID_CLEAR => TrayCommand::ClearSnow,
        ID_SNOWMAN => TrayCommand::BuildSnowman,
        ID_TREE => TrayCommand::AddTree,
        ID_LIGHTS => TrayCommand::AddLights,
        ID_PLOUGH => TrayCommand::SendPlough,
        ID_YETI => TrayCommand::CallYeti,
        ID_SETTINGS => TrayCommand::OpenSettings,
        ID_QUIT => TrayCommand::Quit,
        _ => {
            let k = id.strip_prefix(ID_FX_PREFIX)?;
            let kind = EffectKind::ALL.iter().copied().find(|e| e.id() == k)?;
            TrayCommand::SetEffect(EffectChoice::Fixed(kind))
        }
    })
}

struct Items {
    menu: Menu,
    /// Every effect-choice check item with its choice.
    choices: Vec<(EffectChoice, CheckMenuItem)>,
    pause: MenuItem,
    settings: MenuItem,
    quit: MenuItem,
}

fn pause_label(state: &TrayState) -> &'static str {
    i18n::t(&state.lang, if state.paused { "menu.resume" } else { "menu.pause" })
}

fn build_items(state: &TrayState) -> Result<Items> {
    let lang = state.lang.as_str();
    let menu = Menu::new();
    let mut choices = Vec::new();
    let mk = |choice: EffectChoice, key: &str| {
        CheckMenuItem::with_id(MenuId::new(choice_id(choice)), i18n::t(lang, key), true, false, None)
    };
    for kind in EffectKind::ALL {
        let it = mk(EffectChoice::Fixed(kind), &i18n::effect_key(kind));
        menu.append(&it)?;
        choices.push((EffectChoice::Fixed(kind), it));
    }
    menu.append(&PredefinedMenuItem::separator())?;
    for (c, key) in [
        (EffectChoice::Weather, "effect.weather"),
        (EffectChoice::Season, "effect.season"),
        (EffectChoice::Random, "effect.random"),
        (EffectChoice::Off, "effect.off"),
    ] {
        let it = mk(c, key);
        menu.append(&it)?;
        choices.push((c, it));
    }
    menu.append(&PredefinedMenuItem::separator())?;
    let pause = MenuItem::with_id(MenuId::new(ID_PAUSE), pause_label(state), true, None);
    menu.append(&pause)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&MenuItem::with_id(MenuId::new(ID_SNOWMAN), i18n::t(lang, "menu.build_snowman"), true, None))?;
    menu.append(&MenuItem::with_id(MenuId::new(ID_TREE), i18n::t(lang, "menu.add_tree"), true, None))?;
    menu.append(&MenuItem::with_id(MenuId::new(ID_LIGHTS), i18n::t(lang, "menu.add_lights"), true, None))?;
    menu.append(&MenuItem::with_id(MenuId::new(ID_PLOUGH), i18n::t(lang, "menu.send_plough"), true, None))?;
    menu.append(&MenuItem::with_id(MenuId::new(ID_YETI), i18n::t(lang, "menu.call_yeti"), true, None))?;
    menu.append(&MenuItem::with_id(MenuId::new(ID_CLEAR), i18n::t(lang, "menu.clear_snow"), true, None))?;
    menu.append(&PredefinedMenuItem::separator())?;
    let settings =
        MenuItem::with_id(MenuId::new(ID_SETTINGS), i18n::t(lang, "menu.settings"), true, None);
    menu.append(&settings)?;
    menu.append(&PredefinedMenuItem::separator())?;
    let quit = MenuItem::with_id(MenuId::new(ID_QUIT), i18n::t(lang, "menu.quit"), true, None);
    menu.append(&quit)?;
    Ok(Items { menu, choices, pause, settings, quit })
}

fn sync_checks(items: &Items, effect: EffectChoice) {
    for (c, it) in &items.choices {
        it.set_checked(*c == effect);
    }
}

fn make_icon() -> Result<Icon> {
    Icon::from_rgba(crate::icon::snowflake_rgba(), crate::icon::SIZE, crate::icon::SIZE)
        .map_err(|e| anyhow!("icon: {e}"))
}

/// System tray icon (snowflake) with menu.
pub struct Tray {
    icon: TrayIcon,
    items: Items,
    state: TrayState,
}

impl Tray {
    pub fn new(state: &TrayState) -> Result<Self> {
        let items = build_items(state)?;
        sync_checks(&items, state.effect);
        let icon = TrayIconBuilder::new()
            .with_icon(make_icon()?)
            .with_tooltip(i18n::t(&state.lang, "tray.tooltip"))
            .with_menu(Box::new(items.menu.clone()))
            .with_menu_on_left_click(false)
            .build()
            .map_err(|e| anyhow!("tray: {e}"))?;
        Ok(Self { icon, items, state: state.clone() })
    }

    /// Rebuild/refresh menu check marks and labels.
    pub fn update(&mut self, state: &TrayState) {
        if state.lang != self.state.lang {
            match build_items(state) {
                Ok(items) => {
                    self.icon.set_menu(Some(Box::new(items.menu.clone())));
                    let _ = self.icon.set_tooltip(Some(i18n::t(&state.lang, "tray.tooltip")));
                    self.items = items;
                }
                Err(e) => log::warn!("tray menu rebuild failed: {e}"),
            }
        } else {
            self.items.pause.set_text(pause_label(state));
            self.items.settings.set_text(i18n::t(&state.lang, "menu.settings"));
            self.items.quit.set_text(i18n::t(&state.lang, "menu.quit"));
        }
        sync_checks(&self.items, state.effect);
        self.state = state.clone();
    }

    /// Next pending command, if any (non-blocking). Left-click on the icon = OpenSettings.
    pub fn poll(&mut self) -> Option<TrayCommand> {
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if let Some(cmd) = parse_id(&ev.id.0) {
                if let TrayCommand::SetEffect(c) = &cmd {
                    // CheckMenuItems toggle themselves; force exactly the chosen one.
                    self.state.effect = *c;
                }
                sync_checks(&self.items, self.state.effect);
                return Some(cmd);
            }
        }
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = ev
            {
                return Some(TrayCommand::OpenSettings);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_roundtrip() {
        let mut all = vec![EffectChoice::Off, EffectChoice::Season, EffectChoice::Random, EffectChoice::Weather];
        all.extend(EffectKind::ALL.iter().map(|k| EffectChoice::Fixed(*k)));
        for c in all {
            assert_eq!(parse_id(&choice_id(c)), Some(TrayCommand::SetEffect(c)));
        }
    }
}
