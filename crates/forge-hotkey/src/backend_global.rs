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

const FIRED_QUEUE: usize = 64;
const EVENT_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(5);

thread_local! {
    /// Only ever populated on the event-loop thread: `MainThreadLink` jobs are the sole accessors.
    static MANAGER: RefCell<Option<GlobalHotKeyManager>> = const { RefCell::new(None) };
}

struct Registration {
    hotkey: HotKey,
    caller_id: HotkeyId,
    combo: HotkeyCombo,
    /// Latched press state; both edges repeat while the combo is held, so only a change emits.
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
        let hotkey = combo_to_hotkey(combo)?;
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

fn combo_to_hotkey(combo: &HotkeyCombo) -> Result<HotKey, HotkeyError> {
    let s = combo.as_str();
    let parts: Vec<&str> = s.split('+').collect();
    let Some((&key_str, modifier_parts)) = parts.split_last() else {
        return Err(HotkeyError::InvalidCombo(s.to_owned()));
    };

    let mut modifiers = Modifiers::empty();
    for m in modifier_parts {
        match *m {
            "Ctrl" => modifiers |= Modifiers::CONTROL,
            "Shift" => modifiers |= Modifiers::SHIFT,
            "Alt" => modifiers |= Modifiers::ALT,
            "Meta" => modifiers |= Modifiers::META,
            _ => return Err(HotkeyError::InvalidCombo(s.to_owned())),
        }
    }

    let code = str_to_code(key_str).ok_or_else(|| HotkeyError::InvalidCombo(s.to_owned()))?;

    let mods = if modifiers.is_empty() {
        None
    } else {
        Some(modifiers)
    };

    Ok(HotKey::new(mods, code))
}

fn str_to_code(s: &str) -> Option<Code> {
    match s {
        "A" => Some(Code::KeyA),
        "B" => Some(Code::KeyB),
        "C" => Some(Code::KeyC),
        "D" => Some(Code::KeyD),
        "E" => Some(Code::KeyE),
        "F" => Some(Code::KeyF),
        "G" => Some(Code::KeyG),
        "H" => Some(Code::KeyH),
        "I" => Some(Code::KeyI),
        "J" => Some(Code::KeyJ),
        "K" => Some(Code::KeyK),
        "L" => Some(Code::KeyL),
        "M" => Some(Code::KeyM),
        "N" => Some(Code::KeyN),
        "O" => Some(Code::KeyO),
        "P" => Some(Code::KeyP),
        "Q" => Some(Code::KeyQ),
        "R" => Some(Code::KeyR),
        "S" => Some(Code::KeyS),
        "T" => Some(Code::KeyT),
        "U" => Some(Code::KeyU),
        "V" => Some(Code::KeyV),
        "W" => Some(Code::KeyW),
        "X" => Some(Code::KeyX),
        "Y" => Some(Code::KeyY),
        "Z" => Some(Code::KeyZ),
        "0" => Some(Code::Digit0),
        "1" => Some(Code::Digit1),
        "2" => Some(Code::Digit2),
        "3" => Some(Code::Digit3),
        "4" => Some(Code::Digit4),
        "5" => Some(Code::Digit5),
        "6" => Some(Code::Digit6),
        "7" => Some(Code::Digit7),
        "8" => Some(Code::Digit8),
        "9" => Some(Code::Digit9),
        "F1" => Some(Code::F1),
        "F2" => Some(Code::F2),
        "F3" => Some(Code::F3),
        "F4" => Some(Code::F4),
        "F5" => Some(Code::F5),
        "F6" => Some(Code::F6),
        "F7" => Some(Code::F7),
        "F8" => Some(Code::F8),
        "F9" => Some(Code::F9),
        "F10" => Some(Code::F10),
        "F11" => Some(Code::F11),
        "F12" => Some(Code::F12),
        "Delete" => Some(Code::Delete),
        "Insert" => Some(Code::Insert),
        "Home" => Some(Code::Home),
        "End" => Some(Code::End),
        "PageUp" => Some(Code::PageUp),
        "PageDown" => Some(Code::PageDown),
        "Backspace" => Some(Code::Backspace),
        "Tab" => Some(Code::Tab),
        "Enter" => Some(Code::Enter),
        "Escape" => Some(Code::Escape),
        "Space" => Some(Code::Space),
        "ArrowUp" => Some(Code::ArrowUp),
        "ArrowDown" => Some(Code::ArrowDown),
        "ArrowLeft" => Some(Code::ArrowLeft),
        "ArrowRight" => Some(Code::ArrowRight),
        "Num0" => Some(Code::Numpad0),
        "Num1" => Some(Code::Numpad1),
        "Num2" => Some(Code::Numpad2),
        "Num3" => Some(Code::Numpad3),
        "Num4" => Some(Code::Numpad4),
        "Num5" => Some(Code::Numpad5),
        "Num6" => Some(Code::Numpad6),
        "Num7" => Some(Code::Numpad7),
        "Num8" => Some(Code::Numpad8),
        "Num9" => Some(Code::Numpad9),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn ctrl_f5() -> HotkeyCombo {
        HotkeyCombo::parse("Ctrl+F5").unwrap()
    }

    #[test]
    fn a_combo_another_app_already_holds_is_reported_as_a_conflict() {
        let combo = ctrl_f5();
        let hotkey = combo_to_hotkey(&combo).unwrap();

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
