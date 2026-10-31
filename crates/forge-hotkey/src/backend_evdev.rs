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
use crate::evdev_watch::{EVENT_NODE_PREFIX, INPUT_DIR, InputDirChange, InputDirWatcher};
use crate::owned_fd_readiness::register_readiness;
use crate::vocabulary::keys::{Key, Modifier, Modifiers};

const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const SYN_DROPPED: u16 = 3;
const KEY_DOWN: i32 = 1;
const KEY_UP: i32 = 0;
const KEY_LEFTCTRL: u16 = 29;
const KEY_RIGHTCTRL: u16 = 97;
const KEY_LEFTSHIFT: u16 = 42;
const KEY_RIGHTSHIFT: u16 = 54;
const KEY_LEFTALT: u16 = 56;
const KEY_RIGHTALT: u16 = 100;
const KEY_LEFTMETA: u16 = 125;
const KEY_RIGHTMETA: u16 = 126;
const INPUT_EVENT_SIZE: usize = 24;
const READ_CHUNK_EVENTS: usize = 64;
const FIRED_QUEUE_CAPACITY: usize = 64;

pub(crate) struct EvdevBackend {
    registered: Registered,
    fired_rx_slot: Mutex<Option<mpsc::Receiver<HotkeyFiredEvent>>>,
}

type HeldKeys = Arc<Mutex<HashMap<u16, (HotkeyId, HotkeyCombo)>>>;
type Registered = Arc<RwLock<HashMap<HotkeyId, HotkeyCombo>>>;

impl EvdevBackend {
    pub(crate) async fn try_new() -> Result<Self, HotkeyError> {
        let input_dir = Path::new(INPUT_DIR);
        let devices = discover_input_devices(input_dir).await?;
        if devices.is_empty() {
            return Err(HotkeyError::PermissionDenied);
        }

        let registered: Registered = Arc::new(RwLock::new(HashMap::new()));
        let (fired_tx, fired_rx) = mpsc::channel::<HotkeyFiredEvent>(FIRED_QUEUE_CAPACITY);

        let watcher = match InputDirWatcher::new(input_dir) {
            Ok(w) => Some(w),
            Err(e) => {
                tracing::warn!(error = %e, "cannot watch {INPUT_DIR}; keyboards plugged in later will not be read");
                None
            }
        };

        let pool = DevicePool {
            open: HashSet::new(),
            registered: Arc::clone(&registered),
            fired_tx,
        };
        tokio::spawn(pool.run(devices, watcher));

        Ok(Self {
            registered,
            fired_rx_slot: Mutex::new(Some(fired_rx)),
        })
    }
}

#[async_trait::async_trait]
impl HotkeyBackend for EvdevBackend {
    async fn register(&self, id: HotkeyId, combo: &HotkeyCombo) -> Result<(), HotkeyError> {
        self.registered
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id, combo.clone());
        Ok(())
    }

    async fn unregister(&self, id: HotkeyId) -> Result<(), HotkeyError> {
        self.registered
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&id);
        Ok(())
    }

    fn fired_rx(&self) -> Option<mpsc::Receiver<HotkeyFiredEvent>> {
        self.fired_rx_slot
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
    }
}

struct DevicePool {
    open: HashSet<PathBuf>,
    registered: Registered,
    fired_tx: mpsc::Sender<HotkeyFiredEvent>,
}

impl DevicePool {
    async fn run(mut self, initial: Vec<PathBuf>, mut watcher: Option<InputDirWatcher>) {
        let (closed_tx, mut closed_rx) = mpsc::unbounded_channel::<PathBuf>();
        for path in initial {
            self.open_device(path, &closed_tx);
        }

        loop {
            tokio::select! {
                Some(path) = closed_rx.recv() => {
                    self.open.remove(&path);
                }
                change = next_dir_change(watcher.as_mut()) => {
                    match change {
                        Ok(InputDirChange::Appeared(paths)) => {
                            for path in paths {
                                self.open_device(path, &closed_tx);
                            }
                        }
                        Ok(InputDirChange::Rescan) => {
                            if let Some(dir) = watcher.as_ref().map(|w| w.dir().to_path_buf())
                                && let Ok(paths) = discover_input_devices(&dir).await
                            {
                                for path in paths {
                                    self.open_device(path, &closed_tx);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "input device directory watch failed; keyboards plugged in later will not be read");
                            watcher = None;
                        }
                    }
                }
                () = self.fired_tx.closed() => break,
            }
        }
    }

    fn open_device(&mut self, path: PathBuf, closed_tx: &mpsc::UnboundedSender<PathBuf>) {
        if self.open.contains(&path) {
            return;
        }
        let file = match open_device_nonblocking(&path) {
            Ok(f) => f,
            Err(e) => {
                tracing::debug!(device = %path.display(), error = %e, "input device not readable yet");
                return;
            }
        };
        let async_fd = match register_readiness(file) {
            Ok(fd) => fd,
            Err(e) => {
                tracing::debug!(device = %path.display(), error = %e, "input device cannot be polled");
                return;
            }
        };

        self.open.insert(path.clone());
        let registered = Arc::clone(&self.registered);
        let fired_tx = self.fired_tx.clone();
        let closed_tx = closed_tx.clone();
        tokio::spawn(async move {
            let modifier_state = Mutex::new(HashSet::new());
            let held_keys: HeldKeys = Arc::new(Mutex::new(HashMap::new()));
            let end = read_device_events(
                async_fd,
                &modifier_state,
                &held_keys,
                &registered,
                &fired_tx,
            )
            .await;
            tracing::info!(device = %path.display(), reason = %end, "input device reader stopped");
            release_all_held(&held_keys, &fired_tx).await;
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the device watcher may already have stopped"
            )]
            let _ = closed_tx.send(path);
        });
    }
}

async fn next_dir_change(watcher: Option<&mut InputDirWatcher>) -> std::io::Result<InputDirChange> {
    match watcher {
        Some(w) => w.next_change().await,
        None => std::future::pending().await,
    }
}

async fn discover_input_devices(input_dir: &Path) -> Result<Vec<PathBuf>, HotkeyError> {
    let mut dir = match tokio::fs::read_dir(input_dir).await {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            return Err(HotkeyError::PermissionDenied);
        }
        Err(e) => {
            return Err(HotkeyError::Backend(format!(
                "read {}: {e}",
                input_dir.display()
            )));
        }
    };

    let mut devices = Vec::new();
    while let Ok(Some(entry)) = dir.next_entry().await {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with(EVENT_NODE_PREFIX) {
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

fn open_device_nonblocking(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
}

async fn read_device_events(
    async_fd: AsyncFd<std::fs::File>,
    modifier_state: &Mutex<HashSet<u16>>,
    held_keys: &HeldKeys,
    registered: &Registered,
    fired_tx: &mpsc::Sender<HotkeyFiredEvent>,
) -> String {
    let mut buf = [0u8; INPUT_EVENT_SIZE * READ_CHUNK_EVENTS];
    loop {
        let mut guard = match async_fd.readable().await {
            Ok(g) => g,
            Err(e) => return e.to_string(),
        };
        let read = match guard.try_io(|inner| inner.get_ref().read(&mut buf)) {
            Ok(Ok(0)) => return "end of stream".to_owned(),
            Ok(Ok(n)) => n,
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Ok(Err(e)) => return e.to_string(),
            Err(_would_block) => continue,
        };
        drop(guard);

        let (events, _partial) = buf[..read].as_chunks::<INPUT_EVENT_SIZE>();
        for raw in events {
            handle_key_event(raw, modifier_state, held_keys, registered, fired_tx).await;
        }
    }
}

async fn release_all_held(held_keys: &HeldKeys, fired_tx: &mpsc::Sender<HotkeyFiredEvent>) {
    let held: Vec<(HotkeyId, HotkeyCombo)> = held_keys
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .drain()
        .map(|(_, binding)| binding)
        .collect();
    for (id, combo) in held {
        if fired_tx
            .send(HotkeyFiredEvent {
                id,
                combo,
                timestamp_us: current_timestamp_us(),
                edge: HotkeyEdge::Release,
            })
            .await
            .is_err()
        {
            tracing::debug!("hotkey supervisor stopped; release edge dropped");
        }
    }
}

async fn handle_key_event(
    raw: &[u8],
    modifier_state: &Mutex<HashSet<u16>>,
    held_keys: &HeldKeys,
    registered: &Registered,
    fired_tx: &mpsc::Sender<HotkeyFiredEvent>,
) {
    let ev_type = u16::from_ne_bytes([raw[16], raw[17]]);
    let code = u16::from_ne_bytes([raw[18], raw[19]]);
    let value = i32::from_ne_bytes([raw[20], raw[21], raw[22], raw[23]]);

    if ev_type == EV_SYN && code == SYN_DROPPED {
        modifier_state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        return;
    }

    if ev_type != EV_KEY {
        return;
    }

    if modifier_at(code).is_some() {
        let mut state = modifier_state.lock().unwrap_or_else(|p| p.into_inner());
        if value == KEY_DOWN {
            state.insert(code);
        } else if value == KEY_UP {
            state.remove(&code);
        }
    } else if let Some(key) = key_at(code) {
        if value == KEY_DOWN {
            let modifiers =
                held_modifiers(&modifier_state.lock().unwrap_or_else(|p| p.into_inner()));
            let pressed = HotkeyCombo::new(modifiers, key);
            press_key(code, &pressed, held_keys, registered, fired_tx).await;
        } else if value == KEY_UP {
            release_key(code, held_keys, fired_tx).await;
        }
    }
}

async fn press_key(
    code: u16,
    pressed: &HotkeyCombo,
    held_keys: &HeldKeys,
    registered: &Registered,
    fired_tx: &mpsc::Sender<HotkeyFiredEvent>,
) {
    let matched = {
        let guard = registered.read().unwrap_or_else(|p| p.into_inner());
        guard
            .iter()
            .find(|(_, combo)| *combo == pressed)
            .map(|(id, combo)| (*id, combo.clone()))
    };
    let Some((id, combo)) = matched else {
        return;
    };

    {
        let mut guard = held_keys.lock().unwrap_or_else(|p| p.into_inner());
        guard.insert(code, (id, combo.clone()));
    }

    if fired_tx
        .send(HotkeyFiredEvent {
            id,
            combo,
            timestamp_us: current_timestamp_us(),
            edge: HotkeyEdge::Press,
        })
        .await
        .is_err()
    {
        tracing::debug!("hotkey supervisor stopped; press edge dropped");
    }
}

async fn release_key(code: u16, held_keys: &HeldKeys, fired_tx: &mpsc::Sender<HotkeyFiredEvent>) {
    let held = {
        let mut guard = held_keys.lock().unwrap_or_else(|p| p.into_inner());
        guard.remove(&code)
    };
    let Some((id, combo)) = held else {
        return;
    };

    if fired_tx
        .send(HotkeyFiredEvent {
            id,
            combo,
            timestamp_us: current_timestamp_us(),
            edge: HotkeyEdge::Release,
        })
        .await
        .is_err()
    {
        tracing::debug!("hotkey supervisor stopped; release edge dropped");
    }
}

fn current_timestamp_us() -> u64 {
    u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros(),
    )
    .unwrap_or(u64::MAX)
}

fn held_modifiers(codes: &HashSet<u16>) -> Modifiers {
    codes.iter().filter_map(|&code| modifier_at(code)).collect()
}

fn modifier_at(code: u16) -> Option<Modifier> {
    match code {
        KEY_LEFTCTRL | KEY_RIGHTCTRL => Some(Modifier::Ctrl),
        KEY_LEFTSHIFT | KEY_RIGHTSHIFT => Some(Modifier::Shift),
        KEY_LEFTALT | KEY_RIGHTALT => Some(Modifier::Alt),
        KEY_LEFTMETA | KEY_RIGHTMETA => Some(Modifier::Meta),
        _ => None,
    }
}

fn key_at(code: u16) -> Option<Key> {
    Key::ALL.into_iter().find(|&key| scancode(key) == code)
}

fn scancode(key: Key) -> u16 {
    match key {
        Key::A => 30,
        Key::B => 48,
        Key::C => 46,
        Key::D => 32,
        Key::E => 18,
        Key::F => 33,
        Key::G => 34,
        Key::H => 35,
        Key::I => 23,
        Key::J => 36,
        Key::K => 37,
        Key::L => 38,
        Key::M => 50,
        Key::N => 49,
        Key::O => 24,
        Key::P => 25,
        Key::Q => 16,
        Key::R => 19,
        Key::S => 31,
        Key::T => 20,
        Key::U => 22,
        Key::V => 47,
        Key::W => 17,
        Key::X => 45,
        Key::Y => 21,
        Key::Z => 44,
        Key::Digit0 => 11,
        Key::Digit1 => 2,
        Key::Digit2 => 3,
        Key::Digit3 => 4,
        Key::Digit4 => 5,
        Key::Digit5 => 6,
        Key::Digit6 => 7,
        Key::Digit7 => 8,
        Key::Digit8 => 9,
        Key::Digit9 => 10,
        Key::F1 => 59,
        Key::F2 => 60,
        Key::F3 => 61,
        Key::F4 => 62,
        Key::F5 => 63,
        Key::F6 => 64,
        Key::F7 => 65,
        Key::F8 => 66,
        Key::F9 => 67,
        Key::F10 => 68,
        Key::F11 => 87,
        Key::F12 => 88,
        Key::Delete => 111,
        Key::Insert => 110,
        Key::Home => 102,
        Key::End => 107,
        Key::PageUp => 104,
        Key::PageDown => 109,
        Key::Backspace => 14,
        Key::Tab => 15,
        Key::Enter => 28,
        Key::Escape => 1,
        Key::Space => 57,
        Key::ArrowUp => 103,
        Key::ArrowDown => 108,
        Key::ArrowLeft => 105,
        Key::ArrowRight => 106,
        Key::Num0 => 82,
        Key::Num1 => 79,
        Key::Num2 => 80,
        Key::Num3 => 81,
        Key::Num4 => 75,
        Key::Num5 => 76,
        Key::Num6 => 77,
        Key::Num7 => 71,
        Key::Num8 => 72,
        Key::Num9 => 73,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTRL: u16 = 29;
    const RIGHT_CTRL: u16 = 97;
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

    fn syn_dropped_bytes() -> [u8; INPUT_EVENT_SIZE] {
        let mut raw = [0u8; INPUT_EVENT_SIZE];
        raw[16..18].copy_from_slice(&EV_SYN.to_ne_bytes());
        raw[18..20].copy_from_slice(&SYN_DROPPED.to_ne_bytes());
        raw
    }

    async fn feed(script: &[(u16, i32)], combos: &[(HotkeyId, &str)]) -> Vec<HotkeyFiredEvent> {
        let raw: Vec<[u8; INPUT_EVENT_SIZE]> = script
            .iter()
            .map(|(code, value)| key_event_bytes(*code, *value))
            .collect();
        feed_raw(&raw, combos).await
    }

    async fn feed_raw(
        script: &[[u8; INPUT_EVENT_SIZE]],
        combos: &[(HotkeyId, &str)],
    ) -> Vec<HotkeyFiredEvent> {
        let modifier_state = Mutex::new(HashSet::new());
        let held_keys: HeldKeys = Arc::new(Mutex::new(HashMap::new()));
        let registered = Arc::new(RwLock::new(
            combos
                .iter()
                .map(|(id, s)| (*id, HotkeyCombo::parse(s).unwrap()))
                .collect::<HashMap<HotkeyId, HotkeyCombo>>(),
        ));
        let (fired_tx, mut fired_rx) = mpsc::channel(16);

        for raw in script {
            handle_key_event(raw, &modifier_state, &held_keys, &registered, &fired_tx).await;
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

        assert_eq!(
            edges(&fired),
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

    fn edges(fired: &[HotkeyFiredEvent]) -> Vec<(HotkeyId, &str, HotkeyEdge)> {
        fired
            .iter()
            .map(|e| (e.id, e.combo.as_str(), e.edge))
            .collect()
    }

    #[tokio::test]
    async fn releasing_one_side_of_a_modifier_keeps_the_other_side_held() {
        let id = HotkeyId(7);
        let script = [
            (CTRL, KEY_DOWN),
            (RIGHT_CTRL, KEY_DOWN),
            (RIGHT_CTRL, KEY_UP),
            (F1, KEY_DOWN),
        ];

        let fired = feed(&script, &[(id, "Ctrl+F1")]).await;

        assert_eq!(edges(&fired), vec![(id, "Ctrl+F1", HotkeyEdge::Press)]);
    }

    #[tokio::test]
    async fn syn_dropped_forgets_the_modifiers_held_before_it() {
        let with_ctrl = HotkeyId(1);
        let bare = HotkeyId(2);
        let script = [
            key_event_bytes(CTRL, KEY_DOWN),
            syn_dropped_bytes(),
            key_event_bytes(F1, KEY_DOWN),
        ];

        let fired = feed_raw(&script, &[(with_ctrl, "Ctrl+F1"), (bare, "F1")]).await;

        assert_eq!(edges(&fired), vec![(bare, "F1", HotkeyEdge::Press)]);
    }

    #[test]
    fn every_key_has_a_scancode_no_other_key_or_modifier_claims() {
        for key in Key::ALL {
            let code = scancode(key);
            assert_eq!(key_at(code), Some(key), "{key:?} scancode {code} is shared");
            assert_eq!(
                modifier_at(code),
                None,
                "{key:?} scancode {code} is a modifier"
            );
        }
    }

    struct FakeDevice {
        _dir: tempfile::TempDir,
        path: PathBuf,
    }

    impl FakeDevice {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("event0");
            make_fifo(&path, 0o600);
            Self { _dir: dir, path }
        }

        fn writer(&self) -> std::fs::File {
            fifo_writer(&self.path)
        }

        fn open_fd_count(&self) -> usize {
            open_fd_count(&self.path)
        }
    }

    fn make_fifo(path: &Path, mode: u32) {
        rustix::fs::mkfifoat(rustix::fs::CWD, path, rustix::fs::Mode::from_raw_mode(mode)).unwrap();
    }

    fn fifo_writer(path: &Path) -> std::fs::File {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap()
    }

    fn open_fd_count(path: &Path) -> usize {
        std::fs::read_dir("/proc/self/fd")
            .unwrap()
            .filter_map(|entry| std::fs::read_link(entry.ok()?.path()).ok())
            .filter(|target| target == path)
            .count()
    }

    fn write_keys(writer: &mut std::fs::File, script: &[(u16, i32)]) {
        use std::io::Write;
        for (code, value) in script {
            writer.write_all(&key_event_bytes(*code, *value)).unwrap();
        }
    }

    async fn recv_soon<T>(rx: impl std::future::Future<Output = Option<T>>) -> T {
        tokio::time::timeout(std::time::Duration::from_secs(30), rx)
            .await
            .unwrap()
            .unwrap()
    }

    fn device_pool(
        combos: &[(HotkeyId, &str)],
    ) -> (
        DevicePool,
        mpsc::Receiver<HotkeyFiredEvent>,
        mpsc::UnboundedSender<PathBuf>,
        mpsc::UnboundedReceiver<PathBuf>,
    ) {
        let registered: Registered = Arc::new(RwLock::new(
            combos
                .iter()
                .map(|(id, s)| (*id, HotkeyCombo::parse(s).unwrap()))
                .collect(),
        ));
        let (fired_tx, fired_rx) = mpsc::channel(16);
        let (closed_tx, closed_rx) = mpsc::unbounded_channel();
        let pool = DevicePool {
            open: HashSet::new(),
            registered,
            fired_tx,
        };
        (pool, fired_rx, closed_tx, closed_rx)
    }

    #[tokio::test]
    async fn a_device_reader_that_ends_releases_the_combo_it_was_holding() {
        let id = HotkeyId(7);
        let device = FakeDevice::new();
        let mut writer = device.writer();
        let (mut pool, mut fired_rx, closed_tx, _closed_rx) = device_pool(&[(id, "Ctrl+F1")]);
        pool.open_device(device.path.clone(), &closed_tx);

        write_keys(&mut writer, &[(CTRL, KEY_DOWN), (F1, KEY_DOWN)]);
        drop(writer);
        let press = recv_soon(fired_rx.recv()).await;
        let release = recv_soon(fired_rx.recv()).await;

        assert_eq!(
            edges(&[press, release]),
            vec![
                (id, "Ctrl+F1", HotkeyEdge::Press),
                (id, "Ctrl+F1", HotkeyEdge::Release),
            ]
        );
    }

    #[tokio::test]
    async fn a_device_reader_that_ends_hands_its_path_back_and_closes_its_fd() {
        let device = FakeDevice::new();
        let writer = device.writer();
        let (mut pool, _fired_rx, closed_tx, mut closed_rx) = device_pool(&[]);
        pool.open_device(device.path.clone(), &closed_tx);

        drop(writer);
        let freed = recv_soon(closed_rx.recv()).await;

        assert_eq!((freed, device.open_fd_count()), (device.path.clone(), 0));
    }

    #[tokio::test]
    async fn opening_an_already_open_device_again_keeps_a_single_reader() {
        let device = FakeDevice::new();
        let _writer = device.writer();
        let (mut pool, _fired_rx, closed_tx, _closed_rx) = device_pool(&[]);

        pool.open_device(device.path.clone(), &closed_tx);
        pool.open_device(device.path.clone(), &closed_tx);

        assert_eq!(device.open_fd_count(), 2);
    }

    fn spawn_watched_pool(
        dir: &Path,
        initial: Vec<PathBuf>,
        combos: &[(HotkeyId, &str)],
    ) -> mpsc::Receiver<HotkeyFiredEvent> {
        let (pool, fired_rx, _closed_tx, _closed_rx) = device_pool(combos);
        let watcher = InputDirWatcher::new(dir).unwrap();
        tokio::spawn(pool.run(initial, Some(watcher)));
        fired_rx
    }

    fn overflow_watch_queue(dir: &Path) {
        use std::os::unix::fs::PermissionsExt;
        let entries = [dir.join("mouse0"), dir.join("mouse1")];
        for entry in &entries {
            std::fs::File::create(entry).unwrap();
        }
        let limit: usize = std::fs::read_to_string("/proc/sys/fs/inotify/max_queued_events")
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        for change in 0..=limit {
            std::fs::set_permissions(
                &entries[change % entries.len()],
                std::fs::Permissions::from_mode(0o600),
            )
            .unwrap();
        }
    }

    #[tokio::test]
    async fn a_rescan_after_an_overflow_opens_new_nodes_without_reopening_open_ones() {
        let id = HotkeyId(7);
        let dir = tempfile::tempdir().unwrap();
        let already_open = dir.path().join("event0");
        let missed = dir.path().join("event1");
        make_fifo(&already_open, 0o600);
        let _already_open_writer = fifo_writer(&already_open);
        let mut fired_rx =
            spawn_watched_pool(dir.path(), vec![already_open.clone()], &[(id, "F1")]);

        overflow_watch_queue(dir.path());
        make_fifo(&missed, 0o600);
        let mut missed_writer = fifo_writer(&missed);
        write_keys(&mut missed_writer, &[(F1, KEY_DOWN)]);
        let press = recv_soon(fired_rx.recv()).await;

        assert_eq!(
            (edges(&[press]), open_fd_count(&already_open)),
            (vec![(id, "F1", HotkeyEdge::Press)], 2)
        );
    }

    #[tokio::test]
    async fn a_node_unreadable_when_created_is_read_after_its_permissions_change() {
        use std::os::unix::fs::PermissionsExt;
        let id = HotkeyId(7);
        let dir = tempfile::tempdir().unwrap();
        let locked = dir.path().join("event3");
        let probe = dir.path().join("event9");
        let mut fired_rx = spawn_watched_pool(dir.path(), Vec::new(), &[(id, "F1")]);

        make_fifo(&locked, 0o000);
        make_fifo(&probe, 0o600);
        let mut probe_writer = fifo_writer(&probe);
        write_keys(&mut probe_writer, &[(F1, KEY_DOWN)]);
        recv_soon(fired_rx.recv()).await;
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o600)).unwrap();
        let mut locked_writer = fifo_writer(&locked);
        write_keys(&mut locked_writer, &[(F1, KEY_DOWN)]);
        let press = recv_soon(fired_rx.recv()).await;

        assert_eq!(edges(&[press]), vec![(id, "F1", HotkeyEdge::Press)]);
    }

    #[tokio::test]
    async fn discovery_lists_only_event_nodes_that_open() {
        let dir = tempfile::tempdir().unwrap();
        let readable = dir.path().join("event0");
        make_fifo(&readable, 0o600);
        make_fifo(&dir.path().join("mouse0"), 0o600);
        std::os::unix::fs::symlink(dir.path().join("gone"), dir.path().join("event1")).unwrap();

        let found = discover_input_devices(dir.path()).await.unwrap();

        assert_eq!(found, vec![readable]);
    }

    #[tokio::test]
    async fn discovery_fails_for_a_directory_without_event_nodes_or_a_missing_one() {
        let empty = tempfile::tempdir().unwrap();
        let missing = empty.path().join("absent");

        let without_nodes = discover_input_devices(empty.path()).await;
        let without_dir = discover_input_devices(&missing).await;

        assert!(matches!(without_nodes, Err(HotkeyError::PermissionDenied)));
        assert!(matches!(
            without_dir,
            Err(HotkeyError::Backend(ref message)) if message.contains("absent")
        ));
    }
}
