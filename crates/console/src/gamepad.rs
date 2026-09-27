use bevy::prelude::{Entity, Gamepad, GamepadButton, Vec2};
use frame::{ControllerButtonLayout, ControllerStickLayout, GameSettings};
use input_iw4::{ClientInput, cl_key_event};

const FIRST_KEY: usize = 128;
const BINDINGS: [(GamepadButton, u32); 16] = [
    (GamepadButton::South, 25),        // jump
    (GamepadButton::East, 35),         // crouch, toggled below
    (GamepadButton::West, 49),         // use
    (GamepadButton::West, 51),         // reload
    (GamepadButton::North, 66),        // next weapon
    (GamepadButton::RightTrigger2, 1), // fire
    (GamepadButton::LeftTrigger2, 13), // hold aim
    (GamepadButton::RightTrigger, 5),  // frag
    (GamepadButton::LeftTrigger, 7),   // tactical
    (GamepadButton::LeftThumb, 59),    // sprint
    (GamepadButton::RightThumb, 3),    // melee
    (GamepadButton::Select, 61),       // scoreboard
    (GamepadButton::DPadUp, 15),
    (GamepadButton::DPadDown, 17),
    (GamepadButton::DPadLeft, 19),
    (GamepadButton::DPadRight, 21),
];
const LAST_KEY: usize = FIRST_KEY + BINDINGS.len() - 1;

#[derive(Default)]
pub(super) struct ControllerInputState {
    selected: Option<Entity>,
    layout: Option<ControllerButtonLayout>,
    crouched: bool,
    stance_down: bool,
}

fn bindings(layout: ControllerButtonLayout) -> [(GamepadButton, u32); BINDINGS.len()] {
    let mut result = BINDINGS;
    match layout {
        ControllerButtonLayout::Default => {}
        ControllerButtonLayout::Tactical => {
            result[1].1 = 3; // B: melee
            result[10].1 = 35; // R3: crouch
        }
        ControllerButtonLayout::Lefty => {
            result[5].1 = 13; // RT: aim
            result[6].1 = 1; // LT: fire
            result[7].1 = 7; // RB: tactical
            result[8].1 = 5; // LB: frag
            result[9].1 = 3; // L3: melee
            result[10].1 = 59; // R3: sprint
        }
        ControllerButtonLayout::Nomad => {
            result[1].1 = 3; // B: melee
            result[6].1 = 57; // LT: toggle aim
            result[10].1 = 35; // R3: crouch
        }
    }
    result
}

fn stick_deadzone(stick: Vec2, deadzone: f32) -> Vec2 {
    let magnitude = stick.length();
    if magnitude <= deadzone {
        return Vec2::ZERO;
    }
    stick * ((magnitude.min(1.0) - deadzone) / (1.0 - deadzone) / magnitude)
}

fn set_binding(client: &mut ClientInput, key: usize, id: u32, down: bool, now: i32, frame: u32) {
    let mut previous = client.keys[key].down != 0;
    if previous && client.keys[key].binding != id {
        cl_key_event(client, key, false, now, frame);
        previous = false;
    }
    client.keys[key].binding = id;
    if previous == down {
        return;
    }
    cl_key_event(client, key, down, now, frame);
}

fn release_all(client: &mut ClientInput, now: i32, frame: u32) {
    for key in FIRST_KEY..=LAST_KEY {
        if client.keys[key].down != 0 {
            cl_key_event(client, key, false, now, frame);
        }
    }
}

pub(super) fn apply_controller(
    selected: Option<(Entity, &Gamepad)>,
    settings: &GameSettings,
    client: &mut ClientInput,
    state: &mut ControllerInputState,
    now: i32,
    frame: u32,
) -> ([f32; 2], [f32; 2]) {
    let selected_entity = selected.map(|(entity, _)| entity);
    if selected_entity != state.selected || state.layout != Some(settings.controller_button_layout)
    {
        release_all(client, now, frame);
        state.selected = selected_entity;
        state.layout = Some(settings.controller_button_layout);
        state.crouched = false;
        state.stance_down = false;
        input_iw4::cl_set_ads(client, false);
    }
    let Some((_, pad)) = selected else {
        return ([0.0; 2], [0.0; 2]);
    };

    let bindings = bindings(settings.controller_button_layout);
    let stance_button = bindings.iter().find(|(_, id)| *id == 35).unwrap().0;
    let stance = pad.pressed(stance_button);
    if stance && !state.stance_down {
        state.crouched = !state.crouched;
    }
    state.stance_down = stance;
    if pad.just_pressed(GamepadButton::South) {
        state.crouched = false;
    }

    for (offset, (button, id)) in bindings.into_iter().enumerate() {
        let key = FIRST_KEY + offset;
        let down = match button {
            _ if id == 35 => state.crouched,
            GamepadButton::LeftTrigger2 | GamepadButton::RightTrigger2 => {
                let value = pad.get(button).unwrap_or(0.0);
                value
                    >= if client.keys[key].down != 0 {
                        0.15
                    } else {
                        0.25
                    }
                    || pad.pressed(button)
            }
            _ => pad.pressed(button),
        };
        set_binding(client, key, id, down, now, frame);
    }
    let (move_stick, look_stick) = match settings.controller_stick_layout {
        ControllerStickLayout::Default | ControllerStickLayout::Legacy => {
            (pad.left_stick(), pad.right_stick())
        }
        ControllerStickLayout::Southpaw | ControllerStickLayout::LegacySouthpaw => {
            (pad.right_stick(), pad.left_stick())
        }
    };
    let move_stick = stick_deadzone(move_stick, settings.controller_move_deadzone);
    let look_stick = stick_deadzone(look_stick, settings.controller_look_deadzone);
    let (movement, look) = match settings.controller_stick_layout {
        ControllerStickLayout::Default | ControllerStickLayout::Southpaw => {
            (move_stick, look_stick)
        }
        ControllerStickLayout::Legacy | ControllerStickLayout::LegacySouthpaw => (
            Vec2::new(look_stick.x, move_stick.y),
            Vec2::new(move_stick.x, look_stick.y),
        ),
    };
    (movement.to_array(), look.to_array())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::gamepad::GamepadAxis;

    #[test]
    fn radial_deadzone_keeps_direction_and_reaches_full_scale() {
        assert_eq!(stick_deadzone(Vec2::new(0.1, 0.1), 0.18), Vec2::ZERO);
        let diagonal = stick_deadzone(Vec2::new(0.6, 0.8), 0.18);
        assert!((diagonal.x - 0.6).abs() < 0.0001);
        assert!((diagonal.y - 0.8).abs() < 0.0001);
    }

    #[test]
    fn buttons_toggle_crouch_and_release_on_disconnect() {
        let mut pad = Gamepad::default();
        let mut client = ClientInput::default();
        let mut state = ControllerInputState::default();
        let id = Entity::PLACEHOLDER;
        let settings = GameSettings::default();

        pad.digital_mut().press(GamepadButton::East);
        apply_controller(
            Some((id, &pad)),
            &settings,
            &mut client,
            &mut state,
            100,
            16,
        );
        assert!(client.kb.movedown.active);
        pad.digital_mut().release(GamepadButton::East);
        apply_controller(
            Some((id, &pad)),
            &settings,
            &mut client,
            &mut state,
            116,
            16,
        );
        assert!(client.kb.movedown.active);
        pad.digital_mut().press(GamepadButton::East);
        apply_controller(
            Some((id, &pad)),
            &settings,
            &mut client,
            &mut state,
            132,
            16,
        );
        assert!(!client.kb.movedown.active);

        pad.digital_mut().press(GamepadButton::LeftTrigger2);
        pad.digital_mut().press(GamepadButton::RightTrigger2);
        apply_controller(
            Some((id, &pad)),
            &settings,
            &mut client,
            &mut state,
            148,
            16,
        );
        assert!(client.kb.speed.active);
        assert!(client.kb.attack.active);
        apply_controller(None, &settings, &mut client, &mut state, 164, 16);
        assert!(!client.kb.speed.active);
        assert!(!client.kb.attack.active);
    }

    #[test]
    fn tactical_and_lefty_presets_change_live_bindings() {
        let mut pad = Gamepad::default();
        let mut client = ClientInput::default();
        let mut state = ControllerInputState::default();
        let mut settings = GameSettings::default();
        let id = Entity::PLACEHOLDER;

        pad.digital_mut().press(GamepadButton::East);
        apply_controller(
            Some((id, &pad)),
            &settings,
            &mut client,
            &mut state,
            100,
            16,
        );
        assert!(client.kb.movedown.active);

        settings.controller_button_layout = ControllerButtonLayout::Tactical;
        apply_controller(
            Some((id, &pad)),
            &settings,
            &mut client,
            &mut state,
            116,
            16,
        );
        assert!(!client.kb.movedown.active);
        assert!(client.kb.melee.active);
        pad.digital_mut().press(GamepadButton::RightThumb);
        apply_controller(
            Some((id, &pad)),
            &settings,
            &mut client,
            &mut state,
            132,
            16,
        );
        assert!(client.kb.movedown.active);

        settings.controller_button_layout = ControllerButtonLayout::Lefty;
        pad.digital_mut().press(GamepadButton::RightTrigger2);
        apply_controller(
            Some((id, &pad)),
            &settings,
            &mut client,
            &mut state,
            148,
            16,
        );
        assert!(client.kb.speed.active);
        assert!(!client.kb.attack.active);
    }

    #[test]
    fn southpaw_and_legacy_resolve_both_sticks() {
        let mut pad = Gamepad::default();
        pad.analog_mut().set(GamepadAxis::LeftStickX, 1.0);
        pad.analog_mut().set(GamepadAxis::RightStickY, 1.0);
        let mut client = ClientInput::default();
        let mut state = ControllerInputState::default();
        let mut settings = GameSettings::default();
        let id = Entity::PLACEHOLDER;

        settings.controller_stick_layout = ControllerStickLayout::Southpaw;
        let (movement, look) = apply_controller(
            Some((id, &pad)),
            &settings,
            &mut client,
            &mut state,
            100,
            16,
        );
        assert_eq!(movement, [0.0, 1.0]);
        assert_eq!(look, [1.0, 0.0]);

        settings.controller_stick_layout = ControllerStickLayout::Legacy;
        let (movement, look) = apply_controller(
            Some((id, &pad)),
            &settings,
            &mut client,
            &mut state,
            116,
            16,
        );
        assert_eq!(movement, [0.0, 0.0]);
        assert_eq!(look, [1.0, 1.0]);
    }

    #[test]
    fn nomad_trigger_toggles_aim_once_per_press() {
        let mut pad = Gamepad::default();
        let mut client = ClientInput::default();
        let mut state = ControllerInputState::default();
        let settings = GameSettings {
            controller_button_layout: ControllerButtonLayout::Nomad,
            ..Default::default()
        };
        let id = Entity::PLACEHOLDER;

        pad.digital_mut().press(GamepadButton::LeftTrigger2);
        apply_controller(
            Some((id, &pad)),
            &settings,
            &mut client,
            &mut state,
            100,
            16,
        );
        assert!(client.using_ads);
        apply_controller(
            Some((id, &pad)),
            &settings,
            &mut client,
            &mut state,
            116,
            16,
        );
        assert!(client.using_ads);
        pad.digital_mut().release(GamepadButton::LeftTrigger2);
        apply_controller(
            Some((id, &pad)),
            &settings,
            &mut client,
            &mut state,
            132,
            16,
        );
        assert!(client.using_ads);
        pad.digital_mut().press(GamepadButton::LeftTrigger2);
        apply_controller(
            Some((id, &pad)),
            &settings,
            &mut client,
            &mut state,
            148,
            16,
        );
        assert!(!client.using_ads);
    }
}
