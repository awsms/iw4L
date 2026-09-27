use std::sync::Arc;
use std::sync::OnceLock;

use bevy::input::gamepad::{GamepadRumbleIntensity, GamepadRumbleRequest};
use bevy::prelude::*;
use net::{LocalPresentClient, PresentedSnapshot};

#[derive(Clone, Debug)]
pub(crate) struct Rumble {
    duration_ms: i32,
    low: Vec<(f32, f32)>,
    high: Vec<(f32, f32)>,
}

impl Rumble {
    fn pulse(duration_ms: i32, strong: f32, weak: f32) -> Arc<Self> {
        Arc::new(Self {
            duration_ms,
            low: vec![(0.0, strong), (1.0, 0.0)],
            high: vec![(0.0, weak), (1.0, 0.0)],
        })
    }

    fn shot() -> Arc<Self> {
        static SHOT: OnceLock<Arc<Rumble>> = OnceLock::new();
        Arc::clone(SHOT.get_or_init(|| Self::pulse(120, 0.55, 0.25)))
    }

    fn test() -> Arc<Self> {
        static TEST: OnceLock<Arc<Rumble>> = OnceLock::new();
        Arc::clone(TEST.get_or_init(|| Self::pulse(450, 0.8, 0.45)))
    }

    pub(crate) fn prepare(
        bank: &assets::SoundCatalog,
        namespace: assets::AssetNamespace,
        name: &str,
    ) -> Result<Arc<Self>, String> {
        let text = |name: &str| -> Result<&str, String> {
            let key = format!("rumble/{name}");
            let bytes = bank
                .rawfiles
                .get(&(namespace, key.clone()))
                .ok_or_else(|| format!("missing {key}"))?;
            std::str::from_utf8(bytes).map_err(|_| format!("invalid text in {key}"))
        };
        let definition = text(name)?.trim_end_matches('\0');
        let mut fields = definition.split('\\');
        if fields.next() != Some("RUMBLE") {
            return Err(format!("invalid rumble header: {name}"));
        }
        let mut duration = None;
        let mut low = None;
        let mut high = None;
        while let Some(key) = fields.next() {
            let value = fields
                .next()
                .ok_or_else(|| format!("missing {key} value: {name}"))?;
            match key {
                "duration" => duration = value.parse::<f32>().ok(),
                "lowRumbleFile" => low = Some(value),
                "highRumbleFile" => high = Some(value),
                _ => {}
            }
        }
        let duration = duration
            .filter(|d| d.is_finite() && *d > 0.0)
            .ok_or_else(|| format!("invalid rumble duration: {name}"))?;
        let graph = |name: Option<&str>| -> Result<Vec<(f32, f32)>, String> {
            let name = name
                .filter(|name| !name.is_empty())
                .ok_or_else(|| "missing rumble graph name".to_owned())?;
            let mut tokens = text(name)?.trim_end_matches('\0').split_whitespace();
            if tokens.next() != Some("RUMBLEGRAPHFILE") {
                return Err(format!("invalid rumble graph header: {name}"));
            }
            let count = tokens
                .next()
                .and_then(|n| n.parse::<usize>().ok())
                .filter(|&n| n > 0 && n <= 16)
                .ok_or_else(|| format!("invalid rumble graph count: {name}"))?;
            let mut points = Vec::with_capacity(count);
            for _ in 0..count {
                let mut number = || {
                    tokens
                        .next()
                        .and_then(|n| n.parse::<f32>().ok())
                        .filter(|v| v.is_finite())
                        .ok_or_else(|| format!("invalid rumble graph point: {name}"))
                };
                let x = number()?;
                let y = number()?;
                if points.last().is_some_and(|&(previous, _)| previous > x) {
                    return Err(format!("unordered rumble graph: {name}"));
                }
                points.push((x, y));
            }
            Ok(points)
        };
        Ok(Arc::new(Self {
            duration_ms: (duration * 1000.0).max(1.0) as i32,
            low: graph(low)?,
            high: graph(high)?,
        }))
    }

    fn intensity(&self, elapsed: i32) -> GamepadRumbleIntensity {
        let fraction = (elapsed as f32 / self.duration_ms as f32).clamp(0.0, 1.0);
        GamepadRumbleIntensity {
            strong_motor: sample(&self.low, fraction),
            weak_motor: sample(&self.high, fraction),
        }
    }
}

fn sample(points: &[(f32, f32)], fraction: f32) -> f32 {
    let mut previous = points[0];
    if fraction <= previous.0 {
        return previous.1.clamp(0.0, 1.0);
    }
    for &next in &points[1..] {
        if fraction <= next.0 {
            let t = if next.0 > previous.0 {
                (fraction - previous.0) / (next.0 - previous.0)
            } else {
                1.0
            };
            return (previous.1 + (next.1 - previous.1) * t).clamp(0.0, 1.0);
        }
        previous = next;
    }
    previous.1.clamp(0.0, 1.0)
}

#[derive(Message)]
pub(crate) struct PlayRumble {
    pub bank_revision: Option<u64>,
    pub rumble: Arc<Rumble>,
}

#[derive(Default)]
struct RumblePlayback {
    owner: Option<(
        frame::WorldGeneration,
        sim::ClientId,
        sim::LifeSequence,
        u64,
    )>,
    last_time: i32,
    active: Vec<(i32, Arc<Rumble>)>,
    output: Option<(Entity, GamepadRumbleIntensity)>,
    next_refresh_ms: i32,
}

pub(crate) fn register(app: &mut App) {
    app.add_message::<PlayRumble>()
        .add_message::<frame::TestControllerRumble>()
        .add_observer(local_weapon_fire_rumble)
        .add_systems(
            Update,
            update
                .in_set(net::ClientSet::Effects)
                .after(crate::entity_events::play_viewmodel_notetrack_messages),
        );
}

fn local_weapon_fire_rumble(
    fire: On<net::EntityWeaponFire>,
    identities: Query<&net::CEntity>,
    local: Res<LocalPresentClient>,
    settings: Res<frame::GameSettings>,
    mut output: MessageWriter<PlayRumble>,
) {
    if !settings.controller_rumble
        || !identities
            .get(fire.entity)
            .is_ok_and(|identity| identity.client() == Some(local.0))
    {
        return;
    }
    output.write(PlayRumble {
        bank_revision: None,
        rumble: Rumble::shot(),
    });
}

fn update(
    mut requests: MessageReader<PlayRumble>,
    mut tests: MessageReader<frame::TestControllerRumble>,
    bank: Option<Res<crate::SoundBank>>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    generation: Res<frame::WorldGeneration>,
    time: Res<Time<Real>>,
    settings: Res<frame::GameSettings>,
    gamepads: Query<Entity, With<Gamepad>>,
    mut state: Local<RumblePlayback>,
    mut output: MessageWriter<GamepadRumbleRequest>,
) {
    let now = time.elapsed().as_millis().min(i32::MAX as u128) as i32;
    let owner = bank.as_ref().and_then(|bank| {
        let meta = presented.snapshot()?.meta.for_client(local.0)?;
        (meta.lifecycle == sim::ClientLifecycle::Alive).then_some((
            *generation,
            local.0,
            meta.life_sequence,
            bank.0.revision(),
        ))
    });
    if owner != state.owner || now < state.last_time {
        state.active.clear();
        if let Some((gamepad, _)) = state.output.take() {
            output.write(GamepadRumbleRequest::Stop { gamepad });
        }
        state.owner = owner;
        state.next_refresh_ms = 0;
    }
    state.last_time = now;
    state
        .active
        .retain(|(start, rumble)| now.saturating_sub(*start) < rumble.duration_ms);
    if !settings.controller_rumble {
        state.active.clear();
        for _ in requests.read() {}
    } else {
        for request in requests.read() {
            if request
                .bank_revision
                .is_some_and(|revision| owner.is_none_or(|owner| owner.3 != revision))
            {
                continue;
            }
            push_rumble(&mut state.active, now, Arc::clone(&request.rumble));
        }
    }
    for _ in tests.read() {
        push_rumble(&mut state.active, now, Rumble::test());
    }
    let mut intensity = GamepadRumbleIntensity {
        strong_motor: 0.0,
        weak_motor: 0.0,
    };
    for (start, rumble) in &state.active {
        let current = rumble.intensity(now.saturating_sub(*start));
        intensity.strong_motor = intensity.strong_motor.max(current.strong_motor);
        intensity.weak_motor = intensity.weak_motor.max(current.weak_motor);
    }
    let selected = gamepads.iter().min_by_key(|entity| entity.to_bits());
    if state
        .output
        .is_some_and(|(entity, _)| Some(entity) != selected)
    {
        if let Some((gamepad, _)) = state.output.take() {
            output.write(GamepadRumbleRequest::Stop { gamepad });
        }
        state.next_refresh_ms = 0;
    }
    if let Some(gamepad) = selected {
        let zero = intensity.strong_motor == 0.0 && intensity.weak_motor == 0.0;
        if zero {
            if state.output.is_some_and(|(_, previous)| {
                previous.strong_motor > 0.0 || previous.weak_motor > 0.0
            }) {
                output.write(GamepadRumbleRequest::Stop { gamepad });
            }
            state.next_refresh_ms = 0;
        } else if should_refresh(
            state.output.map(|(_, value)| value),
            intensity,
            now,
            state.next_refresh_ms,
        ) {
            if state.output.is_some() {
                output.write(GamepadRumbleRequest::Stop { gamepad });
            }
            output.write(GamepadRumbleRequest::Add {
                gamepad,
                duration: std::time::Duration::from_millis(160),
                intensity,
            });
            state.next_refresh_ms = now.saturating_add(80);
        }
        state.output = Some((gamepad, intensity));
    }
}

fn should_refresh(
    previous: Option<GamepadRumbleIntensity>,
    next: GamepadRumbleIntensity,
    now: i32,
    next_refresh_ms: i32,
) -> bool {
    previous.is_none_or(|previous| {
        now >= next_refresh_ms
            || next.strong_motor > previous.strong_motor + 0.15
            || next.weak_motor > previous.weak_motor + 0.15
    })
}

fn push_rumble(active: &mut Vec<(i32, Arc<Rumble>)>, now: i32, rumble: Arc<Rumble>) {
    if active.len() == 32
        && let Some((index, _)) = active
            .iter()
            .enumerate()
            .min_by_key(|(_, (start, rumble))| start.saturating_add(rumble.duration_ms))
    {
        active.swap_remove(index);
    }
    active.push((now, rumble));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_rumble_pulses_decay_to_zero() {
        for rumble in [Rumble::shot(), Rumble::test()] {
            let start = rumble.intensity(0);
            let end = rumble.intensity(rumble.duration_ms);
            assert!(start.strong_motor > 0.0);
            assert!(start.weak_motor > 0.0);
            assert_eq!(end.strong_motor, 0.0);
            assert_eq!(end.weak_motor, 0.0);
        }
    }

    #[test]
    fn steady_rumble_is_not_restarted_every_frame() {
        let intensity = GamepadRumbleIntensity {
            strong_motor: 0.5,
            weak_motor: 0.2,
        };
        assert!(should_refresh(None, intensity, 0, 80));
        assert!(!should_refresh(Some(intensity), intensity, 16, 80));
        assert!(should_refresh(Some(intensity), intensity, 80, 80));
    }
}
