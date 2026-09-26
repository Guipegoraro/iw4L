//! In-game mod menu: F5 opens it and pauses the game, F6 opens it with the
//! game running; either key closes it.
//!
//! A page is data — a title and a list of entries — so a feature adds its own
//! page with [`ModMenuAppExt::add_mod_menu_page`] and never touches layout.
//! Entries run console lines through `session::PendingConsoleLines`, the same
//! queue `--cmds` feeds, so anything typeable is clickable.

use std::collections::BTreeMap;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::time::{Real, Virtual};
use frame::{AppScreen, ClientSet, RuntimeRole};

use crate::UiLayer;
use crate::menu::{MenuEnabled, MenuMapList};
use crate::model::{Modality, Screen, ScreenCmd, UiIntent, Widget};
use crate::nav::Focus;
use crate::retail_menu::RetailMenuStack;
use crate::screens;

/// The root page; every other page id is `mod_menu/<name>`.
pub const MOD_MENU_ROOT: &str = "mod_menu";

const ROWS_PER_COLUMN: usize = 17;
const COLUMN_WIDTH: f32 = 190.0;
const COLUMN_X: f32 = 40.0;
const ROW_Y: f32 = 64.0;
const ROW_H: f32 = 20.0;
const MAX_COLUMNS: usize = 3;
/// Real seconds the game runs after an entry fires under pause, so the action
/// (most go through a sim tick) lands before the freeze resumes.
const APPLY_SECS: f64 = 0.5;

pub fn is_mod_menu(name: &str) -> bool {
    name == MOD_MENU_ROOT
        || name
            .strip_prefix(MOD_MENU_ROOT)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// `mod_menu/<name>`, the id a page is registered and opened under.
pub fn page_id(name: &str) -> String {
    format!("{MOD_MENU_ROOT}/{name}")
}

#[derive(Clone, Debug, PartialEq)]
pub enum ModMenuEntry {
    /// Run a console line; `close` shuts the menu first.
    Command {
        label: String,
        line: String,
        close: bool,
    },
    /// Open another page.
    Page {
        label: String,
        page: String,
    },
    Close,
}

impl ModMenuEntry {
    fn label(&self) -> &str {
        match self {
            Self::Command { label, .. } | Self::Page { label, .. } => label,
            Self::Close => "Close",
        }
    }

    fn on_activate(&self) -> Vec<ScreenCmd> {
        let click = ScreenCmd::PlaySound("mouse_click".into());
        match self {
            Self::Command { line, close, .. } => {
                let mut cmds = vec![click];
                if *close {
                    cmds.push(ScreenCmd::Emit(UiIntent::CloseModMenu));
                }
                cmds.push(ScreenCmd::Emit(UiIntent::ConsoleLine(line.clone())));
                cmds
            }
            Self::Page { page, .. } => vec![click, ScreenCmd::Open(page.clone())],
            Self::Close => vec![click, ScreenCmd::Emit(UiIntent::CloseModMenu)],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModMenuPage {
    pub id: String,
    pub title: String,
    pub entries: Vec<ModMenuEntry>,
    /// The label of this page's link on the root page, if it has one.
    pub root_link: Option<String>,
}

impl ModMenuPage {
    /// A page opened as `mod_menu/<name>`.
    pub fn new(name: &str, title: &str) -> Self {
        Self {
            id: page_id(name),
            title: title.into(),
            entries: Vec::new(),
            root_link: None,
        }
    }

    /// List this page on the root page under `label`.
    pub fn on_root(mut self, label: &str) -> Self {
        self.root_link = Some(label.into());
        self
    }

    /// Run `line` and keep the menu open.
    pub fn command(mut self, label: &str, line: &str) -> Self {
        self.entries.push(ModMenuEntry::Command {
            label: label.into(),
            line: line.into(),
            close: false,
        });
        self
    }

    /// Close the menu, then run `line`.
    pub fn command_and_close(mut self, label: &str, line: &str) -> Self {
        self.entries.push(ModMenuEntry::Command {
            label: label.into(),
            line: line.into(),
            close: true,
        });
        self
    }

    /// Link to the page registered as `mod_menu/<name>`.
    pub fn link(mut self, label: &str, name: &str) -> Self {
        self.entries.push(ModMenuEntry::Page {
            label: label.into(),
            page: page_id(name),
        });
        self
    }
}

/// Every registered page plus the pause state the menu shows.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct ModMenuView {
    pages: BTreeMap<String, ModMenuPage>,
    root_order: Vec<String>,
    /// The menu is open and holding `Time<Virtual>` paused.
    pub paused: bool,
}

impl ModMenuView {
    pub fn insert(&mut self, page: ModMenuPage) {
        if page.root_link.is_some() && !self.root_order.contains(&page.id) {
            self.root_order.push(page.id.clone());
        }
        self.pages.insert(page.id.clone(), page);
    }

    fn entries(&self, id: &str) -> Vec<ModMenuEntry> {
        if id == MOD_MENU_ROOT {
            let mut entries: Vec<ModMenuEntry> = self
                .root_order
                .iter()
                .filter_map(|id| self.pages.get(id))
                .filter_map(|page| {
                    Some(ModMenuEntry::Page {
                        label: page.root_link.clone()?,
                        page: page.id.clone(),
                    })
                })
                .collect();
            entries.push(ModMenuEntry::Close);
            return entries;
        }
        self.pages
            .get(id)
            .map_or_else(Vec::new, |page| page.entries.clone())
    }

    fn title(&self, id: &str) -> &str {
        if id == MOD_MENU_ROOT {
            return "MOD MENU";
        }
        self.pages.get(id).map_or("", |page| page.title.as_str())
    }
}

pub trait ModMenuAppExt {
    /// Register a page; a page with the same id replaces the old one.
    fn add_mod_menu_page(&mut self, page: ModMenuPage) -> &mut Self;
}

impl ModMenuAppExt for App {
    fn add_mod_menu_page(&mut self, page: ModMenuPage) -> &mut Self {
        self.init_resource::<ModMenuView>();
        self.world_mut().resource_mut::<ModMenuView>().insert(page);
        self
    }
}

/// Real-time deadline until which a pausing menu lets the game run.
#[derive(Resource, Default)]
struct ApplyWindow {
    until: f64,
}

/// Open or close the menu from code or the `modmenu` console command.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModMenuRequest {
    Toggle { pause: bool },
    Close,
}

fn entry_widget_id(page: &str, index: usize) -> String {
    format!("{page}/{index}")
}

/// The screen for a mod menu page. Without a view (the stack pushing a name)
/// the page is an empty shell with the right id; the painted stack always has
/// the view.
pub(crate) fn screen(id: &str, view: Option<&ModMenuView>) -> Screen {
    let mut widgets = Vec::new();
    if let Some(view) = view {
        widgets.push(screens::dim(&format!("{id}/dim")));
        let mut title = screens::retail_title(
            &format!("{id}/title"),
            COLUMN_X,
            28.0,
            400.0,
            view.title(id),
        );
        title.style.text_align_mode = 4;
        title.style.text_align_x = 8.0;
        widgets.push(title);
        let state = if view.paused {
            "GAME PAUSED"
        } else {
            "GAME RUNNING"
        };
        let mut status =
            screens::label(&format!("{id}/state"), 440.0, 28.0, 180.0, 28.0, 0.3, state);
        status.style.text_align_mode = 6;
        status.style.fore_color = if view.paused {
            [1.0, 0.75, 0.3, 1.0]
        } else {
            [0.55, 0.9, 0.55, 1.0]
        };
        widgets.push(status);
        let hint = if id == MOD_MENU_ROOT {
            "ESC / F5 / F6 CLOSE"
        } else {
            "ESC BACK   F5 / F6 CLOSE"
        };
        widgets.push(screens::label(
            &format!("{id}/hint"),
            COLUMN_X,
            432.0,
            400.0,
            20.0,
            0.3,
            hint,
        ));
        let entries = view.entries(id);
        let capacity = ROWS_PER_COLUMN * MAX_COLUMNS;
        if entries.len() > capacity {
            diag::warn!(
                Ui,
                "mod menu: page `{id}` has {} entries, showing the first {capacity}",
                entries.len()
            );
        }
        for (index, entry) in entries.iter().take(capacity).enumerate() {
            widgets.push(entry_widget(id, index, entry));
        }
    }
    Screen {
        id: id.into(),
        layer: UiLayer::Shell,
        modality: Modality::Opaque,
        background: None,
        bed: None,
        widgets,
        focus_overrides: vec![],
        on_open: vec![],
        on_back: vec![ScreenCmd::Back],
    }
}

fn entry_widget(page: &str, index: usize, entry: &ModMenuEntry) -> Widget {
    let column = index / ROWS_PER_COLUMN;
    let row = index % ROWS_PER_COLUMN;
    let label = match entry {
        ModMenuEntry::Page { label, .. } => format!("{label}  >"),
        _ => entry.label().to_owned(),
    };
    let mut widget = screens::button(
        &entry_widget_id(page, index),
        COLUMN_X + column as f32 * COLUMN_WIDTH,
        ROW_Y + row as f32 * ROW_H,
        COLUMN_WIDTH - 8.0,
        ROW_H,
        &label,
        entry.on_activate(),
    );
    widget.style.font_enum = 3;
    widget.style.text_scale = 0.33;
    widget.style.text_align_x = 8.0;
    widget.style.background = "menu_button_selection_bar".into();
    widget.on_focus = vec![ScreenCmd::PlaySound("mouse_over".into())];
    widget
}

/// The shell resources the mod menu opens and closes through.
#[derive(SystemParam)]
struct Shell<'w> {
    stack: ResMut<'w, RetailMenuStack>,
    enabled: ResMut<'w, MenuEnabled>,
    focus: ResMut<'w, Focus>,
}

impl Shell<'_> {
    fn open(&mut self) {
        self.stack.names.push(MOD_MENU_ROOT.into());
        self.enabled.0 = true;
        self.focus.widget = Some(entry_widget_id(MOD_MENU_ROOT, 0));
    }

    fn close(&mut self) {
        close(&mut self.stack, &mut self.enabled, &mut self.focus);
    }
}

fn menu_open(stack: &RetailMenuStack) -> bool {
    stack.names.iter().any(|name| is_mod_menu(name))
}

pub(crate) fn close(stack: &mut RetailMenuStack, enabled: &mut MenuEnabled, focus: &mut Focus) {
    stack.names.retain(|name| !is_mod_menu(name));
    if stack.names.is_empty() {
        enabled.0 = false;
        focus.widget = None;
    }
}

fn toggle_mod_menu(
    keys: Res<ButtonInput<KeyCode>>,
    mut requests: MessageReader<ModMenuRequest>,
    screen: Res<AppScreen>,
    role: Res<RuntimeRole>,
    mut shell: Shell,
    mut view: ResMut<ModMenuView>,
) {
    let mut request = requests.read().last().copied();
    if keys.just_pressed(KeyCode::F5) {
        request = Some(ModMenuRequest::Toggle { pause: true });
    } else if keys.just_pressed(KeyCode::F6) {
        request = Some(ModMenuRequest::Toggle { pause: false });
    }
    let Some(request) = request else {
        return;
    };
    if menu_open(&shell.stack) {
        shell.close();
        return;
    }
    let ModMenuRequest::Toggle { pause } = request else {
        return;
    };
    // Another menu (Esc options, class select) owns the shell.
    if *screen != AppScreen::InGame || shell.enabled.0 {
        return;
    }
    // A client cannot stop the server it is joined to.
    let pause = pause && *role != RuntimeRole::Client;
    if request == (ModMenuRequest::Toggle { pause: true }) && !pause {
        diag::info!(
            Ui,
            "mod menu: joined to a remote server; opening without pause"
        );
    }
    view.set_if_neq(ModMenuView {
        paused: pause,
        ..view.clone()
    });
    shell.open();
}

fn route_mod_menu_intents(
    mut intents: MessageReader<UiIntent>,
    mut lines: ResMut<session::PendingConsoleLines>,
    mut shell: Shell,
    mut apply: ResMut<ApplyWindow>,
    real: Res<Time<Real>>,
) {
    for intent in intents.read() {
        match intent {
            UiIntent::ConsoleLine(line) => {
                diag::info!(Ui, "mod menu: {line}");
                lines.0.push(line.clone());
                apply.until = real.elapsed_secs_f64() + APPLY_SECS;
            }
            UiIntent::CloseModMenu => shell.close(),
            _ => {}
        }
    }
}

/// Focus the first entry of a page on open, and the link that led to a child
/// page when coming back from it.
fn sync_mod_menu_focus(
    stack: Res<RetailMenuStack>,
    view: Res<ModMenuView>,
    mut focus: ResMut<Focus>,
    mut previous_top: Local<Option<String>>,
) {
    let top = stack.names.last().cloned();
    if *previous_top == top {
        return;
    }
    let previous = std::mem::replace(&mut *previous_top, top.clone());
    let Some(top) = top.filter(|top| is_mod_menu(top)) else {
        return;
    };
    let back_link = previous.as_deref().and_then(|child| {
        view.entries(&top)
            .iter()
            .position(|entry| matches!(entry, ModMenuEntry::Page { page, .. } if page == child))
    });
    focus.widget = Some(entry_widget_id(&top, back_link.unwrap_or(0)));
}

/// Hold `Time<Virtual>` — and with it `FixedUpdate`, where the authority and
/// prediction step — paused while a pausing menu is open, except for the
/// short window after an entry fires. Leaving the match any other way drops
/// the menu and the pause with it.
fn sync_pause(
    screen: Res<AppScreen>,
    mut stack: ResMut<RetailMenuStack>,
    mut view: ResMut<ModMenuView>,
    mut time: ResMut<Time<Virtual>>,
    apply: Res<ApplyWindow>,
    real: Res<Time<Real>>,
    mut holding: Local<bool>,
) {
    if *screen != AppScreen::InGame && menu_open(&stack) {
        stack.names.retain(|name| !is_mod_menu(name));
    }
    let open = menu_open(&stack);
    if !open && view.paused {
        view.paused = false;
    }
    let want = open && view.paused && real.elapsed_secs_f64() >= apply.until;
    if want && !*holding {
        time.pause();
        *holding = true;
    } else if !want && *holding {
        time.unpause();
        *holding = false;
    }
}

fn refresh_map_page(maps: Res<MenuMapList>, mut view: ResMut<ModMenuView>) {
    if !maps.is_changed() {
        return;
    }
    let mut page = ModMenuPage::new("maps", "CHANGE MAP");
    for zone in &maps.0 {
        page = page.command_and_close(zone, &format!("map {zone}"));
    }
    view.insert(page);
}

fn refresh_weapon_page(
    weapons: Option<Res<assets::PreparedWeapons>>,
    mut view: ResMut<ModMenuView>,
) {
    let Some(weapons) = weapons.filter(|weapons| weapons.is_changed()) else {
        return;
    };
    let mut page = ModMenuPage::new("weapons", "GIVE WEAPON").on_root("Weapons");
    for name in weapons.giveable_names() {
        page = page.command(&name, &format!("give {name}"));
    }
    view.insert(page);
}

fn default_pages() -> Vec<ModMenuPage> {
    vec![
        ModMenuPage::new("match", "MATCH")
            .on_root("Match")
            .command("Start match now", "force_match_start")
            .command_and_close("Restart map", "map_restart")
            .link("Change map", "maps")
            .command("Slow motion (0.25x)", "timescale 0.25")
            .command("Half speed (0.5x)", "timescale 0.5")
            .command("Normal speed", "timescale 1")
            .command("Double speed (2x)", "timescale 2")
            .command_and_close("Leave game", "disconnect"),
        ModMenuPage::new("player", "PLAYER")
            .on_root("Player")
            .command("Spawn", "spawn")
            .command("Respawn at a random spawn", "force_spawn")
            .command("Kill yourself", "kill")
            .command("Heal", "heal")
            .command("God mode on / off", "god")
            .command("Save position", "savepos")
            .command_and_close("Load position", "loadpos")
            .command_and_close("Noclip on / off", "noclip")
            .command("Take 40 damage", "damage 40")
            .command("Show position", "showpos on")
            .command("Hide position", "showpos off"),
        ModMenuPage::new("skate", "SKATE")
            .on_root("Skate")
            .command_and_close("Skate on", "skate on")
            .command_and_close("Skate off (walk)", "skate off")
            .command("Third person on / off", "thirdperson")
            .command("Chase cam / shoulder cam", "skatecam"),
        ModMenuPage::new("world", "WORLD")
            .on_root("World")
            .command("Moon gravity (100)", "gravity 100")
            .command("Low gravity (400)", "gravity 400")
            .command("Normal gravity (800)", "gravity 800")
            .command("Heavy gravity (1600)", "gravity 1600")
            .command("Fast (speed 380)", "speed 380")
            .command("Normal speed (190)", "speed 190")
            .command("Slow motion (0.25x)", "timescale 0.25")
            .command("Normal time", "timescale 1"),
        // Filled once a match prepares its weapons.
        ModMenuPage::new("weapons", "GIVE WEAPON").on_root("Weapons"),
        ModMenuPage::new("bots", "BOTS")
            .on_root("Bots")
            .command("Add 1 bot", "bot add 1")
            .command("Add 5 bots", "bot add 5")
            .command("Add 1 dummy (stands still)", "bot dummy 1")
            .command("Bots hold still", "bot hold on")
            .command("Bots move again", "bot hold off")
            .command("All bots fire", "bot fire all"),
        ModMenuPage::new("visuals", "VISUALS")
            .on_root("Visuals")
            .command("Third person on / off", "thirdperson")
            .command("Fullbright on", "r_fullbright 1")
            .command("Fullbright off", "r_fullbright 0")
            .command("Shadows on", "sm_enable 1")
            .command("Shadows off", "sm_enable 0")
            .command("Reset vision", "visionReset"),
        ModMenuPage::new("capture", "CAPTURE")
            .on_root("Capture")
            .command_and_close("Screenshot", "wait 0.25; screenshot")
            .command_and_close("Save last 45 s as a clip", "clip")
            .command_and_close("Start recording a demo", "record")
            .command_and_close("Stop recording", "stoprecord")
            .command("Dump game state", "dump"),
        ModMenuPage::new("maps", "CHANGE MAP"),
    ]
}

pub(crate) struct ModMenuPlugin;

impl Plugin for ModMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ModMenuView>()
            .init_resource::<ApplyWindow>()
            .add_message::<ModMenuRequest>();
        for page in default_pages() {
            app.add_mod_menu_page(page);
        }
        app.add_systems(
            Update,
            (
                (refresh_map_page, refresh_weapon_page, toggle_mod_menu)
                    .before(crate::retail_menu::handle_menu_back),
                (route_mod_menu_intents, sync_mod_menu_focus, sync_pause)
                    .chain()
                    .after(crate::retail_menu::handle_retail_clicks),
            )
                .in_set(ClientSet::Ui),
        );
    }
}
