#![cfg(any(target_os = "windows", target_os = "macos"))]

use std::collections::HashMap;
use std::sync::Mutex;

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tokio::sync::mpsc;

use crate::backend::{HotkeyBackend, HotkeyFiredEvent, HotkeyId};
use crate::combo::HotkeyCombo;
use crate::error::HotkeyError;

pub(crate) struct GlobalHotkeyBackend {
    manager: GlobalHotKeyManager,
    hotkeys: Mutex<HashMap<HotkeyId, HotKey>>,
    fired_rx_slot: Mutex<Option<mpsc::Receiver<HotkeyFiredEvent>>>,
}

impl GlobalHotkeyBackend {
    pub(crate) fn new() -> Result<Self, HotkeyError> {
        let manager =
            GlobalHotKeyManager::new().map_err(|e| HotkeyError::Backend(e.to_string()))?;

        let (fired_tx, fired_rx) = mpsc::channel::<HotkeyFiredEvent>(64);

        tokio::spawn(poll_global_hotkey_events(fired_tx));

        Ok(Self {
            manager,
            hotkeys: Mutex::new(HashMap::new()),
            fired_rx_slot: Mutex::new(Some(fired_rx)),
        })
    }
}

impl HotkeyBackend for GlobalHotkeyBackend {
    fn register(&self, id: HotkeyId, combo: &HotkeyCombo) -> Result<(), HotkeyError> {
        let hotkey = combo_to_hotkey(combo)?;
        self.manager
            .register(hotkey)
            .map_err(|e| HotkeyError::Backend(e.to_string()))?;
        self.hotkeys
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id, hotkey);
        Ok(())
    }

    fn unregister(&self, id: HotkeyId) -> Result<(), HotkeyError> {
        let hotkey = {
            let mut guard = self.hotkeys.lock().unwrap_or_else(|p| p.into_inner());
            guard.remove(&id)
        };
        if let Some(hk) = hotkey {
            self.manager
                .unregister(hk)
                .map_err(|e| HotkeyError::Backend(e.to_string()))?;
        }
        Ok(())
    }

    fn fired_rx(&self) -> Option<mpsc::Receiver<HotkeyFiredEvent>> {
        self.fired_rx_slot
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
    }
}

async fn poll_global_hotkey_events(fired_tx: mpsc::Sender<HotkeyFiredEvent>) {
    loop {
        if let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state == HotKeyState::Pressed {
                let combo_str = format!("{:?}", event.hotkey());
                let combo = match HotkeyCombo::parse(&combo_str) {
                    Ok(c) => c,
                    Err(_) => {
                        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                        continue;
                    }
                };
                let ev = HotkeyFiredEvent {
                    id: HotkeyId(event.id()),
                    combo,
                    timestamp_us: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_micros() as u64,
                };
                if fired_tx.send(ev).await.is_err() {
                    break;
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
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
