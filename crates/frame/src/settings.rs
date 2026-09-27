use bevy::prelude::{Message, Resource};

#[derive(Message)]
pub struct TestControllerRumble;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControllerButtonLayout {
    #[default]
    Default,
    Tactical,
    Lefty,
    Nomad,
}

impl ControllerButtonLayout {
    pub const ALL: [Self; 4] = [Self::Default, Self::Tactical, Self::Lefty, Self::Nomad];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Tactical => "Tactical",
            Self::Lefty => "Lefty",
            Self::Nomad => "Nomad",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|layout| layout.label().eq_ignore_ascii_case(value))
    }

    pub fn step(self, direction: i32) -> Self {
        Self::ALL[(self as i32 + direction).rem_euclid(Self::ALL.len() as i32) as usize]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControllerStickLayout {
    #[default]
    Default,
    Southpaw,
    Legacy,
    LegacySouthpaw,
}

impl ControllerStickLayout {
    pub const ALL: [Self; 4] = [
        Self::Default,
        Self::Southpaw,
        Self::Legacy,
        Self::LegacySouthpaw,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Southpaw => "Southpaw",
            Self::Legacy => "Legacy",
            Self::LegacySouthpaw => "Legacy Southpaw",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|layout| layout.label().eq_ignore_ascii_case(value))
    }

    pub fn step(self, direction: i32) -> Self {
        Self::ALL[(self as i32 + direction).rem_euclid(Self::ALL.len() as i32) as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplayResolution {
    pub width: u32,
    pub height: u32,
}

impl DisplayResolution {
    pub const HD: Self = Self {
        width: 1280,
        height: 720,
    };

    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

impl core::fmt::Display for DisplayResolution {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct GameSettings {
    pub resolution: DisplayResolution,
    pub fullscreen: bool,
    pub vsync: bool,
    pub fov: f32,
    pub master_volume: f32,
    pub sensitivity: f32,
    pub invert_mouse: bool,
    pub controller_button_layout: ControllerButtonLayout,
    pub controller_stick_layout: ControllerStickLayout,
    pub controller_sensitivity: f32,
    pub controller_invert_pitch: bool,
    pub controller_move_deadzone: f32,
    pub controller_look_deadzone: f32,
    pub controller_rumble: bool,
    pub player_name: String,

    pub revision: u64,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            resolution: DisplayResolution::HD,
            fullscreen: false,
            vsync: true,
            fov: Self::FOV_DEFAULT,
            master_volume: 1.0,
            sensitivity: 5.0,
            invert_mouse: false,
            controller_button_layout: ControllerButtonLayout::Default,
            controller_stick_layout: ControllerStickLayout::Default,
            controller_sensitivity: 1.0,
            controller_invert_pitch: false,
            controller_move_deadzone: 0.18,
            controller_look_deadzone: 0.18,
            controller_rumble: true,
            player_name: "Player".to_owned(),
            revision: 0,
        }
    }
}

impl GameSettings {
    pub const FOV_DEFAULT: f32 = 65.0;
    pub const FOV_MIN: f32 = 65.0;
    pub const FOV_MAX: f32 = 120.0;

    pub fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn sanitize(&mut self) {
        self.resolution.width = self.resolution.width.clamp(640, 7680);
        self.resolution.height = self.resolution.height.clamp(480, 4320);
        self.fov = if self.fov.is_finite() {
            self.fov.clamp(Self::FOV_MIN, Self::FOV_MAX)
        } else {
            Self::FOV_DEFAULT
        };
        self.master_volume = self.master_volume.clamp(0.0, 1.0);
        self.sensitivity = self.sensitivity.clamp(0.1, 30.0);
        self.controller_sensitivity = if self.controller_sensitivity.is_finite() {
            self.controller_sensitivity.clamp(0.1, 5.0)
        } else {
            1.0
        };
        self.controller_move_deadzone = if self.controller_move_deadzone.is_finite() {
            self.controller_move_deadzone.clamp(0.0, 0.4)
        } else {
            0.18
        };
        self.controller_look_deadzone = if self.controller_look_deadzone.is_finite() {
            self.controller_look_deadzone.clamp(0.0, 0.4)
        } else {
            0.18
        };
        self.player_name = self.player_name.trim().chars().take(16).collect();
        if self.player_name.is_empty() {
            self.player_name = "Player".to_owned();
        }
    }
}
