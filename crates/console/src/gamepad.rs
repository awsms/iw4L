use bevy::prelude::{Entity, Gamepad, GamepadButton, Vec2};
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
    crouched: bool,
    east_down: bool,
}

fn stick_deadzone(stick: Vec2, deadzone: f32) -> Vec2 {
    let magnitude = stick.length();
    if magnitude <= deadzone {
        return Vec2::ZERO;
    }
    stick * ((magnitude.min(1.0) - deadzone) / (1.0 - deadzone) / magnitude)
}

fn set_binding(client: &mut ClientInput, key: usize, id: u32, down: bool, now: i32, frame: u32) {
    let previous = client.keys[key].down != 0;
    if previous == down {
        return;
    }
    client.keys[key].binding = id;
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
    client: &mut ClientInput,
    state: &mut ControllerInputState,
    now: i32,
    frame: u32,
) -> ([f32; 2], [f32; 2]) {
    let selected_entity = selected.map(|(entity, _)| entity);
    if selected_entity != state.selected {
        release_all(client, now, frame);
        state.selected = selected_entity;
        state.crouched = false;
        state.east_down = false;
    }
    let Some((_, pad)) = selected else {
        return ([0.0; 2], [0.0; 2]);
    };

    let east = pad.pressed(GamepadButton::East);
    if east && !state.east_down {
        state.crouched = !state.crouched;
    }
    state.east_down = east;
    if pad.just_pressed(GamepadButton::South) {
        state.crouched = false;
    }

    for (offset, (button, id)) in BINDINGS.into_iter().enumerate() {
        let key = FIRST_KEY + offset;
        let down = match button {
            GamepadButton::East => state.crouched,
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
    let movement = stick_deadzone(pad.left_stick(), 0.18).to_array();
    let look = stick_deadzone(pad.right_stick(), 0.18).to_array();
    (movement, look)
}

#[cfg(test)]
mod tests {
    use super::*;

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

        pad.digital_mut().press(GamepadButton::East);
        apply_controller(Some((id, &pad)), &mut client, &mut state, 100, 16);
        assert!(client.kb.movedown.active);
        pad.digital_mut().release(GamepadButton::East);
        apply_controller(Some((id, &pad)), &mut client, &mut state, 116, 16);
        assert!(client.kb.movedown.active);
        pad.digital_mut().press(GamepadButton::East);
        apply_controller(Some((id, &pad)), &mut client, &mut state, 132, 16);
        assert!(!client.kb.movedown.active);

        pad.digital_mut().press(GamepadButton::LeftTrigger2);
        pad.digital_mut().press(GamepadButton::RightTrigger2);
        apply_controller(Some((id, &pad)), &mut client, &mut state, 148, 16);
        assert!(client.kb.speed.active);
        assert!(client.kb.attack.active);
        apply_controller(None, &mut client, &mut state, 164, 16);
        assert!(!client.kb.speed.active);
        assert!(!client.kb.attack.active);
    }
}
