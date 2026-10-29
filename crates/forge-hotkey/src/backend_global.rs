#![cfg(any(target_os = "windows", target_os = "macos"))]

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tokio::sync::mpsc;

use crate::backend::{HotkeyBackend, HotkeyEdge, HotkeyFiredEvent, HotkeyId};
use crate::combo::HotkeyCombo;
use crate::error::HotkeyError;
use crate::main_thread::MainThreadLink;
use crate::vocabulary::keys::{Key, Modifier};

const FIRED_QUEUE: usize = 64;
const EVENT_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(5);

thread_local! {
    static MANAGER: RefCell<Option<GlobalHotKeyManager>> = const { RefCell::new(None) };
}

struct Registration {
    hotkey: HotKey,
    caller_id: HotkeyId,
    combo: HotkeyCombo,
    held: bool,
}

type RegistrationMap = Arc<Mutex<HashMap<u32, Registration>>>;

pub(crate) struct GlobalHotkeyBackend {
    main_thread: MainThreadLink,
    registrations: RegistrationMap,
    fired_rx_slot: Mutex<Option<mpsc::Receiver<HotkeyFiredEvent>>>,
}

impl GlobalHotkeyBackend {
    pub(crate) async fn new(main_thread: MainThreadLink) -> Result<Self, HotkeyError> {
        main_thread.run(install_manager).await??;

        let (fired_tx, fired_rx) = mpsc::channel::<HotkeyFiredEvent>(FIRED_QUEUE);

        let registrations: RegistrationMap = Arc::new(Mutex::new(HashMap::new()));
        tokio::spawn(poll_global_hotkey_events(
            fired_tx,
            Arc::clone(&registrations),
        ));

        Ok(Self {
            main_thread,
            registrations,
            fired_rx_slot: Mutex::new(Some(fired_rx)),
        })
    }
}

fn install_manager() -> Result<(), HotkeyError> {
    let manager = GlobalHotKeyManager::new().map_err(|e| HotkeyError::Backend(e.to_string()))?;
    MANAGER.with(|slot| *slot.borrow_mut() = Some(manager));
    Ok(())
}

fn with_manager(
    op: impl FnOnce(&GlobalHotKeyManager) -> global_hotkey::Result<()>,
    combo: &HotkeyCombo,
) -> Result<(), HotkeyError> {
    MANAGER.with(|slot| match slot.borrow().as_ref() {
        Some(manager) => op(manager).map_err(|e| map_manager_error(e, combo)),
        None => Err(HotkeyError::Backend(
            "global hotkey manager is not installed".to_owned(),
        )),
    })
}

fn map_manager_error(error: global_hotkey::Error, combo: &HotkeyCombo) -> HotkeyError {
    match error {
        global_hotkey::Error::AlreadyRegistered(_) => HotkeyError::AlreadyRegistered {
            combo: combo.as_str().to_owned(),
        },
        other => HotkeyError::Backend(other.to_string()),
    }
}

#[async_trait]
impl HotkeyBackend for GlobalHotkeyBackend {
    async fn register(&self, id: HotkeyId, combo: &HotkeyCombo) -> Result<(), HotkeyError> {
        let hotkey = combo_to_hotkey(combo);
        if combo.swallows_typing() {
            tracing::warn!(
                combo = %combo,
                "a bare typing key is bound as a global hotkey: the system delivers it to forge only, so it cannot be typed in other apps"
            );
        }
        let job_combo = combo.clone();
        self.main_thread
            .run(move || with_manager(|manager| manager.register(hotkey), &job_combo))
            .await??;
        self.registrations
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(
                hotkey.id(),
                Registration {
                    hotkey,
                    caller_id: id,
                    combo: combo.clone(),
                    held: false,
                },
            );
        Ok(())
    }

    async fn unregister(&self, id: HotkeyId) -> Result<(), HotkeyError> {
        let target = {
            let guard = self.registrations.lock().unwrap_or_else(|p| p.into_inner());
            guard
                .iter()
                .find(|(_, reg)| reg.caller_id == id)
                .map(|(&internal_id, reg)| (internal_id, reg.hotkey, reg.combo.clone()))
        };
        let Some((internal_id, hotkey, combo)) = target else {
            return Ok(());
        };
        self.main_thread
            .run(move || with_manager(|manager| manager.unregister(hotkey), &combo))
            .await??;
        self.registrations
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&internal_id);
        Ok(())
    }

    fn fired_rx(&self) -> Option<mpsc::Receiver<HotkeyFiredEvent>> {
        self.fired_rx_slot
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
    }
}

async fn poll_global_hotkey_events(
    fired_tx: mpsc::Sender<HotkeyFiredEvent>,
    registrations: RegistrationMap,
) {
    loop {
        if let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            let pressed = event.state == HotKeyState::Pressed;

            let transition = {
                let mut guard = registrations.lock().unwrap_or_else(|p| p.into_inner());
                guard.get_mut(&event.id).and_then(|reg| {
                    let changed = reg.held != pressed;
                    reg.held = pressed;
                    changed.then(|| (reg.caller_id, reg.combo.clone()))
                })
            };

            if let Some((caller_id, combo)) = transition {
                let ev = HotkeyFiredEvent {
                    id: caller_id,
                    combo,
                    timestamp_us: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_micros() as u64,
                    edge: if pressed {
                        HotkeyEdge::Press
                    } else {
                        HotkeyEdge::Release
                    },
                };
                if fired_tx.send(ev).await.is_err() {
                    break;
                }
            }
        }
        tokio::time::sleep(EVENT_POLL_INTERVAL).await;
    }
}

fn combo_to_hotkey(combo: &HotkeyCombo) -> HotKey {
    let modifiers: Modifiers = combo
        .modifiers()
        .iter()
        .map(modifier_flag)
        .fold(Modifiers::empty(), |all, flag| all | flag);
    let modifiers = (!modifiers.is_empty()).then_some(modifiers);
    HotKey::new(modifiers, key_code(combo.key()))
}

fn modifier_flag(modifier: Modifier) -> Modifiers {
    match modifier {
        Modifier::Ctrl => Modifiers::CONTROL,
        Modifier::Shift => Modifiers::SHIFT,
        Modifier::Alt => Modifiers::ALT,
        Modifier::Meta => Modifiers::META,
    }
}

fn key_code(key: Key) -> Code {
    match key {
        Key::A => Code::KeyA,
        Key::B => Code::KeyB,
        Key::C => Code::KeyC,
        Key::D => Code::KeyD,
        Key::E => Code::KeyE,
        Key::F => Code::KeyF,
        Key::G => Code::KeyG,
        Key::H => Code::KeyH,
        Key::I => Code::KeyI,
        Key::J => Code::KeyJ,
        Key::K => Code::KeyK,
        Key::L => Code::KeyL,
        Key::M => Code::KeyM,
        Key::N => Code::KeyN,
        Key::O => Code::KeyO,
        Key::P => Code::KeyP,
        Key::Q => Code::KeyQ,
        Key::R => Code::KeyR,
        Key::S => Code::KeyS,
        Key::T => Code::KeyT,
        Key::U => Code::KeyU,
        Key::V => Code::KeyV,
        Key::W => Code::KeyW,
        Key::X => Code::KeyX,
        Key::Y => Code::KeyY,
        Key::Z => Code::KeyZ,
        Key::Digit0 => Code::Digit0,
        Key::Digit1 => Code::Digit1,
        Key::Digit2 => Code::Digit2,
        Key::Digit3 => Code::Digit3,
        Key::Digit4 => Code::Digit4,
        Key::Digit5 => Code::Digit5,
        Key::Digit6 => Code::Digit6,
        Key::Digit7 => Code::Digit7,
        Key::Digit8 => Code::Digit8,
        Key::Digit9 => Code::Digit9,
        Key::F1 => Code::F1,
        Key::F2 => Code::F2,
        Key::F3 => Code::F3,
        Key::F4 => Code::F4,
        Key::F5 => Code::F5,
        Key::F6 => Code::F6,
        Key::F7 => Code::F7,
        Key::F8 => Code::F8,
        Key::F9 => Code::F9,
        Key::F10 => Code::F10,
        Key::F11 => Code::F11,
        Key::F12 => Code::F12,
        Key::Delete => Code::Delete,
        Key::Insert => Code::Insert,
        Key::Home => Code::Home,
        Key::End => Code::End,
        Key::PageUp => Code::PageUp,
        Key::PageDown => Code::PageDown,
        Key::Backspace => Code::Backspace,
        Key::Tab => Code::Tab,
        Key::Enter => Code::Enter,
        Key::Escape => Code::Escape,
        Key::Space => Code::Space,
        Key::ArrowUp => Code::ArrowUp,
        Key::ArrowDown => Code::ArrowDown,
        Key::ArrowLeft => Code::ArrowLeft,
        Key::ArrowRight => Code::ArrowRight,
        Key::Num0 => Code::Numpad0,
        Key::Num1 => Code::Numpad1,
        Key::Num2 => Code::Numpad2,
        Key::Num3 => Code::Numpad3,
        Key::Num4 => Code::Numpad4,
        Key::Num5 => Code::Numpad5,
        Key::Num6 => Code::Numpad6,
        Key::Num7 => Code::Numpad7,
        Key::Num8 => Code::Numpad8,
        Key::Num9 => Code::Numpad9,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctrl_f5() -> HotkeyCombo {
        HotkeyCombo::parse("Ctrl+F5").unwrap()
    }

    #[test]
    fn a_combo_another_app_already_holds_is_reported_as_a_conflict() {
        let combo = ctrl_f5();
        let hotkey = combo_to_hotkey(&combo);

        let error = map_manager_error(global_hotkey::Error::AlreadyRegistered(hotkey), &combo);

        assert!(
            matches!(error, HotkeyError::AlreadyRegistered { ref combo } if combo == "Ctrl+F5"),
            "an OS-level conflict was not surfaced as AlreadyRegistered: {error:?}"
        );
    }

    #[test]
    fn any_other_manager_failure_stays_a_backend_error() {
        let error = map_manager_error(
            global_hotkey::Error::FailedToRegister("denied".to_owned()),
            &ctrl_f5(),
        );

        assert!(matches!(error, HotkeyError::Backend(_)), "got {error:?}");
    }
}
