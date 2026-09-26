//! Mod guide: how to use this fork's additions, reachable from the pause menu.
//!
//! Topics are data. A feature ships its instructions with
//! `app.add_mod_guide_topic(ModGuideTopic::new(..).line(..))`; the pause menu
//! lists every topic and each opens as a page of text.

use bevy::prelude::*;

use crate::UiLayer;
use crate::model::{Modality, Screen, ScreenCmd};
use crate::screens;

pub const MOD_GUIDE_ROOT: &str = "mod_guide";

const LEFT: f32 = 40.0;
const TOP: f32 = 64.0;
const ROW_H: f32 = 20.0;
const LINE_H: f32 = 15.0;
const TEXT_W: f32 = 580.0;
const MAX_LINES: usize = 23;

pub(crate) fn is_mod_guide(id: &str) -> bool {
    id == MOD_GUIDE_ROOT || id.starts_with("mod_guide/")
}

fn topic_id(name: &str) -> String {
    format!("{MOD_GUIDE_ROOT}/{name}")
}

#[derive(Clone, Debug)]
pub struct ModGuideTopic {
    name: String,
    title: String,
    lines: Vec<String>,
}

impl ModGuideTopic {
    pub fn new(name: &str, title: &str) -> Self {
        Self {
            name: name.into(),
            title: title.into(),
            lines: Vec::new(),
        }
    }

    /// One line of text; an empty string leaves a gap.
    pub fn line(mut self, text: &str) -> Self {
        self.lines.push(text.into());
        self
    }
}

/// Every registered topic, in registration order.
#[derive(Resource, Default, Debug)]
pub struct ModGuide {
    topics: Vec<ModGuideTopic>,
}

impl ModGuide {
    pub fn add(&mut self, topic: ModGuideTopic) {
        self.topics.retain(|t| t.name != topic.name);
        self.topics.push(topic);
    }
}

pub trait ModGuideAppExt {
    fn add_mod_guide_topic(&mut self, topic: ModGuideTopic) -> &mut Self;
}

impl ModGuideAppExt for App {
    fn add_mod_guide_topic(&mut self, topic: ModGuideTopic) -> &mut Self {
        self.world_mut()
            .get_resource_or_insert_with(ModGuide::default)
            .add(topic);
        self
    }
}

pub(crate) fn screen(id: &str, guide: Option<&ModGuide>) -> Screen {
    let mut widgets = vec![screens::dim(&format!("{id}/dim"))];
    // A darker panel under the text, readable over a bright map.
    let mut panel = screens::dim(&format!("{id}/panel"));
    panel.rect.x = LEFT - 8.0;
    panel.rect.y = 20.0;
    panel.rect.w = TEXT_W + 24.0;
    panel.rect.h = 440.0;
    panel.rect.horz_align = 1;
    panel.rect.vert_align = 1;
    panel.style.fore_color = [0.0, 0.0, 0.0, 0.72];
    widgets.push(panel);
    let topic = guide.and_then(|g| g.topics.iter().find(|t| topic_id(&t.name) == id));
    let title = topic.map_or("MOD GUIDE", |t| t.title.as_str());
    let mut heading = screens::retail_title(&format!("{id}/title"), LEFT, 28.0, 400.0, title);
    heading.style.text_align_mode = 4;
    heading.style.text_align_x = 8.0;
    widgets.push(heading);
    widgets.push(screens::label(
        &format!("{id}/hint"),
        LEFT,
        440.0,
        400.0,
        20.0,
        0.3,
        "ESC / B BACK",
    ));

    match (topic, guide) {
        (Some(topic), _) => {
            for (index, text) in topic.lines.iter().take(MAX_LINES).enumerate() {
                let mut line = screens::label(
                    &format!("{id}/line_{index}"),
                    LEFT + 8.0,
                    TOP + index as f32 * LINE_H,
                    TEXT_W,
                    LINE_H,
                    0.27,
                    text,
                );
                line.style.font_enum = 3;
                line.style.text_align_mode = 4;
                widgets.push(line);
            }
        }
        (None, Some(guide)) => {
            for (index, topic) in guide.topics.iter().enumerate() {
                let mut row = screens::button(
                    &format!("{id}/topic_{index}"),
                    LEFT,
                    TOP + index as f32 * ROW_H,
                    240.0,
                    ROW_H,
                    &format!("{}  >", topic.title),
                    vec![
                        ScreenCmd::PlaySound("mouse_click".into()),
                        ScreenCmd::Open(topic_id(&topic.name)),
                    ],
                );
                row.style.font_enum = 3;
                row.style.text_scale = 0.33;
                row.style.text_align_x = 8.0;
                row.style.background = "menu_button_selection_bar".into();
                row.on_focus = vec![ScreenCmd::PlaySound("mouse_over".into())];
                widgets.push(row);
            }
        }
        (None, None) => {}
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

fn default_topics() -> Vec<ModGuideTopic> {
    vec![
        ModGuideTopic::new("mod_menu", "MOD MENU")
            .line("A menu of ready-made actions, so no command has to be typed.")
            .line("")
            .line("Open and pause:  F5, or pad START")
            .line("Open, game keeps running:  F6")
            .line("Move:  arrows or D-pad.   Pick:  Enter or A")
            .line("Back:  Esc or B.   Close:  F5 or F6")
            .line("F2 works as Esc in a match.")
            .line("")
            .line("Pages: match, player, skate, weapons, bots, visuals, capture,")
            .line("maps. An action picked while paused runs the game for half a")
            .line("second so it lands. Console: modmenu [pause|live|close].")
            .line("Cheat actions (heal, give, skate...) work in local matches."),
        ModGuideTopic::new("skate", "SKATEBOARD")
            .line("Turn it on: mod menu > Skate > Skate on, or type  skate .")
            .line("Type  skate off  (or the menu) to walk again.")
            .line("")
            .line("Push:  W  /  left stick up")
            .line("Brake:  S  /  left stick down")
            .line("Carve:  A, D  /  left stick sideways")
            .line("Ollie:  hold Space, let go  /  hold A, let go")
            .line("Spin in the air:  A, D  /  left stick sideways")
            .line("Look:  mouse  /  right stick")
            .line("")
            .line("The longer you hold jump (up to 0.4 s), the higher the ollie.")
            .line("Carving is tighter when slow and wider when fast; the stick")
            .line("carves deeper the further you tilt it. Hills speed you up.")
            .line("You bail (1.5 s without control) when you land across the")
            .line("board, land too hard or roll into a wall.")
            .line("")
            .line("showpos on   shows speed and the board's state on screen."),
        ModGuideTopic::new("gamepad", "GAMEPAD")
            .line("XInput pads work (an 8BitDo in XInput mode shows up as an")
            .line("Xbox 360 controller).")
            .line("")
            .line("Left stick:  move.   Right stick:  look")
            .line("A:  jump, ollie.   B:  crouch")
            .line("X:  use, reload.   Y:  switch weapon")
            .line("RT:  fire.   LT:  aim")
            .line("RB:  lethal.   LB:  tactical")
            .line("L3:  sprint.   R3:  melee")
            .line("D-pad:  action slots.   START:  mod menu.   BACK:  pause")
            .line("")
            .line("In menus: D-pad moves, A picks, B goes back.")
            .line("Rebind like any key, e.g.  bind BUTTON_A +gostand ,")
            .line("or in Options > Controls by pressing the pad button."),
        ModGuideTopic::new("commands", "COMMANDS")
            .line("Open the console with the ` key. Added by this fork:")
            .line("")
            .line("skate [on|off]  -  toggle the skateboard")
            .line("movemode [normal|skate|noclip]  -  show or set the movement mode")
            .line("heal  -  restore your health")
            .line("god [on|off]  -  take no damage")
            .line("timescale [0.05..4]  -  slow motion / fast-forward (1 = normal)")
            .line("gravity [n] / speed [n]  -  world gravity (800) and move speed (190)")
            .line("savepos [slot] / loadpos [slot]  -  save a spot, teleport back to it")
            .line("noclip [on|off]  -  fly through walls: jump up, crouch down, sprint fast")
            .line("modmenu [pause|live|close]  -  open or close the mod menu")
            .line("")
            .line("Launch settings (.env): IW4L_TIME_LIMIT=0 plays with no time")
            .line("limit; IW4L_SCORE_LIMIT sets the score to win.")
            .line("showpos [on|off]  -  position, speed, board state")
            .line("")
            .line("Useful ones that were already there: give <weapon>, spawn,")
            .line("kill, damage <n>, bot add <n>, map <name>, screenshot."),
    ]
}

pub(crate) struct ModGuidePlugin;

impl Plugin for ModGuidePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ModGuide>();
        for topic in default_topics() {
            app.add_mod_guide_topic(topic);
        }
    }
}
