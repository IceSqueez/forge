#![cfg(target_os = "linux")]

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::{Arc, RwLock};

use tokio::io::unix::AsyncFd;
use tokio::sync::mpsc;

use crate::backend::{HotkeyBackend, HotkeyEdge, HotkeyFiredEvent, HotkeyId};
use crate::combo::HotkeyCombo;
use crate::error::HotkeyError;

const EV_KEY: u16 = 1;
const KEY_DOWN: i32 = 1;
const KEY_UP: i32 = 0;
const INPUT_EVENT_SIZE: usize = 24;
const READ_CHUNK_EVENTS: usize = 64;

pub(crate) struct EvdevBackend {
    cmd_tx: mpsc::Sender<EvdevCmd>,
    fired_rx_slot: Mutex<Option<mpsc::Receiver<HotkeyFiredEvent>>>,
}

enum EvdevCmd {
    Register(HotkeyId, HotkeyCombo),
    Unregister(HotkeyId),
}

type HeldKeys = Arc<Mutex<HashMap<u16, (HotkeyId, HotkeyCombo)>>>;

impl EvdevBackend {
    pub(crate) async fn try_new() -> Result<Self, HotkeyError> {
        let devices = discover_input_devices().await?;
        if devices.is_empty() {
            return Err(HotkeyError::PermissionDenied);
        }

        let registered: Arc<RwLock<HashMap<HotkeyId, HotkeyCombo>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let (cmd_tx, cmd_rx) = mpsc::channel::<EvdevCmd>(64);
        let (fired_tx, fired_rx) = mpsc::channel::<HotkeyFiredEvent>(64);
        let modifier_state: Arc<Mutex<HashSet<u16>>> = Arc::new(Mutex::new(HashSet::new()));
        let held_keys: HeldKeys = Arc::new(Mutex::new(HashMap::new()));

        for device_path in devices {
            let modifiers = Arc::clone(&modifier_state);
            let held = Arc::clone(&held_keys);
            let reg = Arc::clone(&registered);
            let tx = fired_tx.clone();
            tokio::spawn(async move {
                read_device_events(device_path, modifiers, held, reg, tx).await;
            });
        }

        tokio::spawn(handle_evdev_commands(cmd_rx, Arc::clone(&registered)));

        Ok(Self {
            cmd_tx,
            fired_rx_slot: Mutex::new(Some(fired_rx)),
        })
    }
}

impl HotkeyBackend for EvdevBackend {
    fn register(&self, id: HotkeyId, combo: &HotkeyCombo) -> Result<(), HotkeyError> {
        self.cmd_tx
            .try_send(EvdevCmd::Register(id, combo.clone()))
            .map_err(|e| HotkeyError::Backend(e.to_string()))
    }

    fn unregister(&self, id: HotkeyId) -> Result<(), HotkeyError> {
        self.cmd_tx
            .try_send(EvdevCmd::Unregister(id))
            .map_err(|e| HotkeyError::Backend(e.to_string()))
    }

    fn fired_rx(&self) -> Option<mpsc::Receiver<HotkeyFiredEvent>> {
        self.fired_rx_slot
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
    }
}

async fn handle_evdev_commands(
    mut cmd_rx: mpsc::Receiver<EvdevCmd>,
    registered: Arc<RwLock<HashMap<HotkeyId, HotkeyCombo>>>,
) {
    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            EvdevCmd::Register(id, combo) => {
                if let Ok(mut guard) = registered.write() {
                    guard.insert(id, combo);
                }
            }
            EvdevCmd::Unregister(id) => {
                if let Ok(mut guard) = registered.write() {
                    guard.remove(&id);
                }
            }
        }
    }
}

async fn discover_input_devices() -> Result<Vec<PathBuf>, HotkeyError> {
    let mut dir = match tokio::fs::read_dir("/dev/input").await {
        Ok(d) => d,
        Err(e) if e.raw_os_error() == Some(13) => return Err(HotkeyError::PermissionDenied),
        Err(e) => return Err(HotkeyError::Backend(format!("read /dev/input: {e}"))),
    };

    let mut devices = Vec::new();
    while let Ok(Some(entry)) = dir.next_entry().await {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with("event") {
            let path = entry.path();
            if open_device_nonblocking(&path).is_ok() {
                devices.push(path);
            }
        }
    }

    if devices.is_empty() {
        return Err(HotkeyError::PermissionDenied);
    }
    Ok(devices)
}

// A blocking-pool read on an idle device never returns and stalls tokio runtime shutdown.
fn open_device_nonblocking(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
}

async fn read_device_events(
    path: PathBuf,
    modifier_state: Arc<Mutex<HashSet<u16>>>,
    held_keys: HeldKeys,
    registered: Arc<RwLock<HashMap<HotkeyId, HotkeyCombo>>>,
    fired_tx: mpsc::Sender<HotkeyFiredEvent>,
) {
    let file = match open_device_nonblocking(&path) {
        Ok(f) => f,
        Err(_) => return,
    };
    let async_fd = match AsyncFd::new(file) {
        Ok(fd) => fd,
        Err(_) => return,
    };

    let mut buf = [0u8; INPUT_EVENT_SIZE * READ_CHUNK_EVENTS];
    loop {
        let mut guard = match async_fd.readable().await {
            Ok(g) => g,
            Err(_) => return,
        };
        let read = match guard.try_io(|inner| inner.get_ref().read(&mut buf)) {
            Ok(Ok(0)) => return,
            Ok(Ok(n)) => n,
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Ok(Err(_)) => return,
            Err(_would_block) => continue,
        };
        drop(guard);

        // chunks_exact guards against acting on a partial trailing chunk.
        for raw in buf[..read].chunks_exact(INPUT_EVENT_SIZE) {
            handle_key_event(raw, &modifier_state, &held_keys, &registered, &fired_tx).await;
        }
    }
}

async fn handle_key_event(
    raw: &[u8],
    modifier_state: &Mutex<HashSet<u16>>,
    held_keys: &HeldKeys,
    registered: &Arc<RwLock<HashMap<HotkeyId, HotkeyCombo>>>,
    fired_tx: &mpsc::Sender<HotkeyFiredEvent>,
) {
    let ev_type = u16::from_ne_bytes([raw[16], raw[17]]);
    let code = u16::from_ne_bytes([raw[18], raw[19]]);
    let value = i32::from_ne_bytes([raw[20], raw[21], raw[22], raw[23]]);

    if ev_type != EV_KEY {
        return;
    }

    if let Some(modifier) = key_code_to_modifier(code) {
        let mut state = modifier_state.lock().unwrap_or_else(|p| p.into_inner());
        if value == KEY_DOWN {
            state.insert(modifier);
        } else if value == KEY_UP {
            state.remove(&modifier);
        }
    } else if let Some(key_name) = key_code_to_name(code) {
        if value == KEY_DOWN {
            let modifiers_held: HashSet<u16> = modifier_state
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone();
            let combo_str = build_combo_string(&modifiers_held, key_name);
            press_key(code, &combo_str, held_keys, registered, fired_tx).await;
        } else if value == KEY_UP {
            release_key(code, held_keys, fired_tx).await;
        }
    }
}

async fn press_key(
    code: u16,
    combo_str: &str,
    held_keys: &HeldKeys,
    registered: &Arc<RwLock<HashMap<HotkeyId, HotkeyCombo>>>,
    fired_tx: &mpsc::Sender<HotkeyFiredEvent>,
) {
    let matched = {
        let guard = registered.read().unwrap_or_else(|p| p.into_inner());
        guard
            .iter()
            .find(|(_, combo)| combo.as_str() == combo_str)
            .map(|(id, combo)| (*id, combo.clone()))
    };
    let Some((id, combo)) = matched else {
        return;
    };

    {
        let mut guard = held_keys.lock().unwrap_or_else(|p| p.into_inner());
        guard.insert(code, (id, combo.clone()));
    }

    let _ = fired_tx
        .send(HotkeyFiredEvent {
            id,
            combo,
            timestamp_us: current_timestamp_us(),
            edge: HotkeyEdge::Press,
        })
        .await;
}

async fn release_key(code: u16, held_keys: &HeldKeys, fired_tx: &mpsc::Sender<HotkeyFiredEvent>) {
    let held = {
        let mut guard = held_keys.lock().unwrap_or_else(|p| p.into_inner());
        guard.remove(&code)
    };
    let Some((id, combo)) = held else {
        return;
    };

    let _ = fired_tx
        .send(HotkeyFiredEvent {
            id,
            combo,
            timestamp_us: current_timestamp_us(),
            edge: HotkeyEdge::Release,
        })
        .await;
}

fn build_combo_string(modifiers: &HashSet<u16>, key: &str) -> String {
    const CTRL_CODE: u16 = 29;
    const SHIFT_CODE: u16 = 42;
    const ALT_CODE: u16 = 56;
    const META_CODE: u16 = 125;

    let mut parts: Vec<&str> = Vec::new();
    if modifiers.contains(&CTRL_CODE) {
        parts.push("Ctrl");
    }
    if modifiers.contains(&SHIFT_CODE) {
        parts.push("Shift");
    }
    if modifiers.contains(&ALT_CODE) {
        parts.push("Alt");
    }
    if modifiers.contains(&META_CODE) {
        parts.push("Meta");
    }
    parts.push(key);
    parts.join("+")
}

fn current_timestamp_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

fn key_code_to_modifier(code: u16) -> Option<u16> {
    match code {
        29 | 97 => Some(29),
        42 | 54 => Some(42),
        56 | 100 => Some(56),
        125 | 126 => Some(125),
        _ => None,
    }
}

fn key_code_to_name(code: u16) -> Option<&'static str> {
    match code {
        2 => Some("1"),
        3 => Some("2"),
        4 => Some("3"),
        5 => Some("4"),
        6 => Some("5"),
        7 => Some("6"),
        8 => Some("7"),
        9 => Some("8"),
        10 => Some("9"),
        11 => Some("0"),
        16 => Some("Q"),
        17 => Some("W"),
        18 => Some("E"),
        19 => Some("R"),
        20 => Some("T"),
        21 => Some("Y"),
        22 => Some("U"),
        23 => Some("I"),
        24 => Some("O"),
        25 => Some("P"),
        30 => Some("A"),
        31 => Some("S"),
        32 => Some("D"),
        33 => Some("F"),
        34 => Some("G"),
        35 => Some("H"),
        36 => Some("J"),
        37 => Some("K"),
        38 => Some("L"),
        44 => Some("Z"),
        45 => Some("X"),
        46 => Some("C"),
        47 => Some("V"),
        48 => Some("B"),
        49 => Some("N"),
        50 => Some("M"),
        59 => Some("F1"),
        60 => Some("F2"),
        61 => Some("F3"),
        62 => Some("F4"),
        63 => Some("F5"),
        64 => Some("F6"),
        65 => Some("F7"),
        66 => Some("F8"),
        67 => Some("F9"),
        68 => Some("F10"),
        87 => Some("F11"),
        88 => Some("F12"),
        1 => Some("Escape"),
        14 => Some("Backspace"),
        15 => Some("Tab"),
        28 => Some("Enter"),
        57 => Some("Space"),
        102 => Some("Home"),
        107 => Some("End"),
        104 => Some("PageUp"),
        109 => Some("PageDown"),
        110 => Some("Insert"),
        111 => Some("Delete"),
        103 => Some("ArrowUp"),
        108 => Some("ArrowDown"),
        105 => Some("ArrowLeft"),
        106 => Some("ArrowRight"),
        71 => Some("Num7"),
        72 => Some("Num8"),
        73 => Some("Num9"),
        75 => Some("Num4"),
        76 => Some("Num5"),
        77 => Some("Num6"),
        79 => Some("Num1"),
        80 => Some("Num2"),
        81 => Some("Num3"),
        82 => Some("Num0"),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const CTRL: u16 = 29;
    const F1: u16 = 59;
    const X: u16 = 45;
    const AUTO_REPEAT: i32 = 2;

    fn key_event_bytes(code: u16, value: i32) -> [u8; INPUT_EVENT_SIZE] {
        let mut raw = [0u8; INPUT_EVENT_SIZE];
        raw[16..18].copy_from_slice(&EV_KEY.to_ne_bytes());
        raw[18..20].copy_from_slice(&code.to_ne_bytes());
        raw[20..24].copy_from_slice(&value.to_ne_bytes());
        raw
    }

    async fn feed(script: &[(u16, i32)], combos: &[(HotkeyId, &str)]) -> Vec<HotkeyFiredEvent> {
        let modifier_state = Mutex::new(HashSet::new());
        let held_keys: HeldKeys = Arc::new(Mutex::new(HashMap::new()));
        let registered = Arc::new(RwLock::new(
            combos
                .iter()
                .map(|(id, s)| (*id, HotkeyCombo::parse(s).unwrap()))
                .collect::<HashMap<HotkeyId, HotkeyCombo>>(),
        ));
        let (fired_tx, mut fired_rx) = mpsc::channel(16);

        for (code, value) in script {
            handle_key_event(
                &key_event_bytes(*code, *value),
                &modifier_state,
                &held_keys,
                &registered,
                &fired_tx,
            )
            .await;
        }

        drop(fired_tx);
        let mut fired = Vec::new();
        while let Some(event) = fired_rx.recv().await {
            fired.push(event);
        }
        fired
    }

    #[tokio::test]
    async fn a_key_up_resolves_its_release_from_the_held_map_after_the_modifier_lifted() {
        let id = HotkeyId(7);
        let script = [
            (CTRL, KEY_DOWN),
            (F1, KEY_DOWN),
            (CTRL, KEY_UP),
            (F1, KEY_UP),
        ];

        let fired = feed(&script, &[(id, "Ctrl+F1")]).await;

        let edges: Vec<(HotkeyId, &str, HotkeyEdge)> = fired
            .iter()
            .map(|e| (e.id, e.combo.as_str(), e.edge))
            .collect();
        assert_eq!(
            edges,
            vec![
                (id, "Ctrl+F1", HotkeyEdge::Press),
                (id, "Ctrl+F1", HotkeyEdge::Release),
            ]
        );
    }

    #[tokio::test]
    async fn no_edge_leaves_the_backend_for_auto_repeat_or_an_unmatched_key_up() {
        let id = HotkeyId(7);
        let cases = [
            (
                "auto-repeat while the combo is held",
                vec![(CTRL, KEY_DOWN), (F1, KEY_DOWN), (F1, AUTO_REPEAT)],
                1,
            ),
            ("key-up for a key that never matched", vec![(X, KEY_UP)], 0),
        ];

        for (case, script, expected) in cases {
            let fired = feed(&script, &[(id, "Ctrl+F1")]).await;
            assert_eq!(fired.len(), expected, "wrong edge count for {case}");
        }
    }
}
