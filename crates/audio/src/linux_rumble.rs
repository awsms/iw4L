//! Linux Xbox rumble through evdev. Gilrs rejects rumble-only devices even
//! though the kernel exposes FF_RUMBLE and can play it directly.

use std::fs::{File, OpenOptions, read_dir, read_to_string};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::path::Path;

const EV_FF: u16 = 0x15;
const FF_RUMBLE: u16 = 0x50;

#[repr(C)]
#[derive(Default)]
struct Trigger {
    button: u16,
    interval: u16,
}

#[repr(C)]
#[derive(Default)]
struct Replay {
    length: u16,
    delay: u16,
}

#[repr(C)]
struct Effect {
    kind: u16,
    id: i16,
    direction: u16,
    trigger: Trigger,
    replay: Replay,
    #[cfg(target_pointer_width = "64")]
    data: [u64; 4],
    #[cfg(target_pointer_width = "32")]
    data: [u32; 7],
}

#[repr(C)]
struct InputEvent {
    time: libc::timeval,
    kind: u16,
    code: u16,
    value: i32,
}

pub(super) struct LinuxRumble {
    file: File,
    effect_id: i16,
}

impl LinuxRumble {
    pub(super) fn open_xbox() -> io::Result<Self> {
        for entry in read_dir("/sys/class/input")? {
            let entry = entry?;
            let event = entry.file_name();
            let Some(event) = event.to_str() else {
                continue;
            };
            if !event.starts_with("event") {
                continue;
            }
            let device = entry.path().join("device");
            let Ok(name) = read_to_string(device.join("name")) else {
                continue;
            };
            if !name.trim().starts_with("Xbox") {
                continue;
            }
            let Ok(capabilities) = read_to_string(device.join("capabilities/ff")) else {
                continue;
            };
            if !supports_rumble(&capabilities) {
                continue;
            }
            let path = Path::new("/dev/input").join(event);
            let file = OpenOptions::new().write(true).open(path)?;
            let mut rumble = Self {
                file,
                effect_id: -1,
            };
            rumble.upload(0, 0, 1)?;
            return Ok(rumble);
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Xbox FF_RUMBLE device not found",
        ))
    }

    fn upload(&mut self, strong: u16, weak: u16, duration_ms: u16) -> io::Result<()> {
        let mut effect = Effect {
            kind: FF_RUMBLE,
            id: self.effect_id,
            direction: 0,
            trigger: Trigger::default(),
            replay: Replay {
                length: duration_ms,
                delay: 0,
            },
            #[cfg(target_pointer_width = "64")]
            data: [0; 4],
            #[cfg(target_pointer_width = "32")]
            data: [0; 7],
        };
        #[cfg(target_pointer_width = "64")]
        {
            effect.data[0] = u64::from(strong) | (u64::from(weak) << 16);
        }
        #[cfg(target_pointer_width = "32")]
        {
            effect.data[0] = u32::from(strong) | (u32::from(weak) << 16);
        }
        let request = ioctl_write(b'E', 0x80, std::mem::size_of::<Effect>());
        // SAFETY: effect is a Linux input ff_effect layout, and the file is an evdev node.
        if unsafe { libc::ioctl(self.file.as_raw_fd(), request, &mut effect) } < 0 {
            return Err(io::Error::last_os_error());
        }
        self.effect_id = effect.id;
        Ok(())
    }

    fn play(&mut self, value: i32) -> io::Result<()> {
        let event = InputEvent {
            time: libc::timeval {
                tv_sec: 0,
                tv_usec: 0,
            },
            kind: EV_FF,
            code: self.effect_id as u16,
            value,
        };
        // SAFETY: InputEvent has the Linux input_event layout and is fully initialized.
        let bytes = unsafe {
            std::slice::from_raw_parts(
                (&event as *const InputEvent).cast(),
                std::mem::size_of::<InputEvent>(),
            )
        };
        self.file.write_all(bytes)
    }

    pub(super) fn set(&mut self, strong: f32, weak: f32) -> io::Result<()> {
        if strong <= 0.0 && weak <= 0.0 {
            return self.play(0);
        }
        self.upload(magnitude(strong), magnitude(weak), 160)?;
        self.play(1)
    }
}

impl Drop for LinuxRumble {
    fn drop(&mut self) {
        if self.effect_id >= 0 {
            let _ = self.play(0);
            let request = ioctl_write(b'E', 0x81, std::mem::size_of::<libc::c_int>());
            // SAFETY: the effect id was allocated by EVIOCSFF on this file.
            unsafe {
                libc::ioctl(
                    self.file.as_raw_fd(),
                    request,
                    self.effect_id as libc::c_ulong,
                )
            };
        }
    }
}

fn magnitude(value: f32) -> u16 {
    (value.clamp(0.0, 1.0) * u16::MAX as f32) as u16
}

fn supports_rumble(capabilities: &str) -> bool {
    let word = 80 / usize::BITS as usize;
    let bit = 80 % usize::BITS as usize;
    capabilities
        .split_whitespace()
        .rev()
        .nth(word)
        .and_then(|word| u64::from_str_radix(word, 16).ok())
        .is_some_and(|word| word & (1 << bit) != 0)
}

const fn ioctl_none(group: u8, command: u8) -> libc::c_ulong {
    ((group as libc::c_ulong) << 8) | command as libc::c_ulong
}

const fn ioctl_write(group: u8, command: u8, size: usize) -> libc::c_ulong {
    (1 << 30) | ((size as libc::c_ulong) << 16) | ioctl_none(group, command)
}

#[cfg(test)]
mod tests {
    use super::{Effect, InputEvent, supports_rumble};

    #[test]
    fn finds_kernel_rumble_capability() {
        assert!(supports_rumble("10000 0"));
        assert!(!supports_rumble("0 0"));
    }

    #[test]
    fn evdev_structures_match_kernel_layout() {
        #[cfg(target_pointer_width = "64")]
        {
            assert_eq!(std::mem::size_of::<Effect>(), 48);
            assert_eq!(std::mem::size_of::<InputEvent>(), 24);
        }
    }
}
