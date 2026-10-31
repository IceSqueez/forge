use forge_types::RecoverMutex;
use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::backend::{InputHandle, MidiBackend};
use crate::error::MidiError;
pub use crate::events::{MidiPortInfo, PortDirection};

type PortSender = mpsc::Sender<(u64, Vec<u8>)>;

pub fn input_port(name: &str) -> MidiPortInfo {
    MidiPortInfo {
        name: name.to_owned(),
        direction: PortDirection::Input,
    }
}

pub fn output_port(name: &str) -> MidiPortInfo {
    MidiPortInfo {
        name: name.to_owned(),
        direction: PortDirection::Output,
    }
}

struct MockInputHandle {
    state: Arc<Mutex<MockState>>,
    token: u64,
}

impl InputHandle for MockInputHandle {}

impl Drop for MockInputHandle {
    fn drop(&mut self) {
        let token = self.token;
        self.state
            .lock_recover()
            .senders
            .retain(|(open, _)| *open != token);
    }
}

#[derive(Default)]
pub(crate) struct MockState {
    pub(crate) input_ports: Vec<MidiPortInfo>,
    pub(crate) output_ports: Vec<MidiPortInfo>,
    pub(crate) senders: Vec<(u64, PortSender)>,
    pub(crate) refused_opens: HashSet<String>,
    pub(crate) next_token: u64,
    pub(crate) sent_outputs: VecDeque<(String, Vec<u8>)>,
    pub(crate) released_outputs: Vec<String>,
}

pub struct MockMidiBackend {
    pub(crate) state: Arc<Mutex<MockState>>,
}

impl MockMidiBackend {
    pub fn new(input_ports: Vec<MidiPortInfo>, output_ports: Vec<MidiPortInfo>) -> Self {
        Self {
            state: Arc::new(Mutex::new(MockState {
                input_ports,
                output_ports,
                senders: Vec::new(),
                refused_opens: HashSet::new(),
                next_token: 0,
                sent_outputs: VecDeque::new(),
                released_outputs: Vec::new(),
            })),
        }
    }

    pub fn set_input_ports(&self, ports: Vec<MidiPortInfo>) {
        self.state.lock_recover().input_ports = ports;
    }

    pub fn set_output_ports(&self, ports: Vec<MidiPortInfo>) {
        self.state.lock_recover().output_ports = ports;
    }

    pub async fn inject_all(&self, ts: u64, data: Vec<u8>) {
        let senders: Vec<PortSender> = {
            let state = self.state.lock_recover();
            state
                .senders
                .iter()
                .map(|(_, sender)| sender)
                .filter(|s| !s.is_closed())
                .cloned()
                .collect()
        };
        for sender in senders {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the receiver may already be gone"
            )]
            let _ = sender.send((ts, data.clone())).await;
        }
    }

    pub fn open_input_count(&self) -> usize {
        self.state.lock_recover().senders.len()
    }

    pub fn released_outputs(&self) -> Vec<String> {
        self.state.lock_recover().released_outputs.clone()
    }

    pub fn sent_outputs(&self) -> Vec<(String, Vec<u8>)> {
        self.state
            .lock_recover()
            .sent_outputs
            .iter()
            .cloned()
            .collect()
    }
}

impl MidiBackend for MockMidiBackend {
    fn list_input_ports(&self) -> Vec<MidiPortInfo> {
        self.state.lock_recover().input_ports.clone()
    }

    fn list_output_ports(&self) -> Vec<MidiPortInfo> {
        self.state.lock_recover().output_ports.clone()
    }

    fn open_input(
        &self,
        port_name: &str,
        event_tx: mpsc::Sender<(u64, Vec<u8>)>,
    ) -> Result<Box<dyn InputHandle>, MidiError> {
        let mut state = self.state.lock_recover();
        if state.refused_opens.contains(port_name)
            || !state.input_ports.iter().any(|p| p.name == port_name)
        {
            return Err(MidiError::PortNotFound {
                name: port_name.to_owned(),
            });
        }
        let token = state.next_token;
        state.next_token += 1;
        state.senders.push((token, event_tx));
        Ok(Box::new(MockInputHandle {
            state: Arc::clone(&self.state),
            token,
        }))
    }

    fn send_output(&self, port_name: &str, data: &[u8]) -> Result<(), MidiError> {
        let mut state = self.state.lock_recover();
        if !state.output_ports.iter().any(|p| p.name == port_name) {
            return Err(MidiError::PortNotFound {
                name: port_name.to_owned(),
            });
        }
        state
            .sent_outputs
            .push_back((port_name.to_owned(), data.to_vec()));
        Ok(())
    }

    fn release_output(&self, port_name: &str) {
        self.state
            .lock_recover()
            .released_outputs
            .push(port_name.to_owned());
    }
}
