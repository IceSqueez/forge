use tokio::sync::mpsc;

use crate::error::MidiError;
use crate::events::{MidiPortInfo, PortDirection};

pub(crate) trait InputHandle: Send {}

pub(crate) trait MidiBackend: Send + Sync {
    fn list_input_ports(&self) -> Vec<MidiPortInfo>;
    fn list_output_ports(&self) -> Vec<MidiPortInfo>;

    fn open_input(
        &self,
        port_name: &str,
        event_tx: mpsc::Sender<(u64, Vec<u8>)>,
    ) -> Result<Box<dyn InputHandle>, MidiError>;

    fn send_output(&self, port_name: &str, data: &[u8]) -> Result<(), MidiError>;
}

pub(crate) struct MidirBackend {
    client_name: String,
}

impl MidirBackend {
    pub(crate) fn new(client_name: impl Into<String>) -> Result<Self, MidiError> {
        let client_name = client_name.into();
        midir::MidiInput::new(&client_name).map_err(|e| MidiError::MidirInit(e.to_string()))?;
        Ok(Self { client_name })
    }
}

struct MidirInputHandle(#[allow(dead_code)] midir::MidiInputConnection<()>);

impl InputHandle for MidirInputHandle {}

impl MidiBackend for MidirBackend {
    fn list_input_ports(&self) -> Vec<MidiPortInfo> {
        let Ok(input) = midir::MidiInput::new(&self.client_name) else {
            return vec![];
        };
        input
            .ports()
            .iter()
            .filter_map(|p| {
                input.port_name(p).ok().map(|name| MidiPortInfo {
                    name,
                    direction: PortDirection::Input,
                })
            })
            .collect()
    }

    fn list_output_ports(&self) -> Vec<MidiPortInfo> {
        let Ok(output) = midir::MidiOutput::new(&self.client_name) else {
            return vec![];
        };
        output
            .ports()
            .iter()
            .filter_map(|p| {
                output.port_name(p).ok().map(|name| MidiPortInfo {
                    name,
                    direction: PortDirection::Output,
                })
            })
            .collect()
    }

    fn open_input(
        &self,
        port_name: &str,
        event_tx: mpsc::Sender<(u64, Vec<u8>)>,
    ) -> Result<Box<dyn InputHandle>, MidiError> {
        let input = midir::MidiInput::new(&self.client_name)
            .map_err(|e| MidiError::MidirInit(e.to_string()))?;
        let ports = input.ports();
        let port = ports
            .iter()
            .find(|p| input.port_name(p).ok().as_deref() == Some(port_name))
            .ok_or_else(|| MidiError::PortNotFound {
                name: port_name.to_owned(),
            })?
            .clone();
        let conn = input
            .connect(
                &port,
                "forge-midi",
                move |timestamp, data, _| {
                    let _ = event_tx.try_send((timestamp, data.to_vec()));
                },
                (),
            )
            .map_err(|e| MidiError::MidirConnect(e.to_string()))?;
        Ok(Box::new(MidirInputHandle(conn)))
    }

    fn send_output(&self, port_name: &str, data: &[u8]) -> Result<(), MidiError> {
        let output = midir::MidiOutput::new(&self.client_name)
            .map_err(|e| MidiError::MidirInit(e.to_string()))?;
        let ports = output.ports();
        let port = ports
            .iter()
            .find(|p| output.port_name(p).ok().as_deref() == Some(port_name))
            .ok_or_else(|| MidiError::PortNotFound {
                name: port_name.to_owned(),
            })?
            .clone();
        let mut conn = output
            .connect(&port, "forge-midi")
            .map_err(|e| MidiError::MidirConnect(e.to_string()))?;
        conn.send(data)
            .map_err(|e| MidiError::OutputSend(e.to_string()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
pub(crate) mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use super::*;

    type PortSender = mpsc::Sender<(u64, Vec<u8>)>;

    pub(crate) struct MockInputHandle {
        state: Arc<Mutex<MockState>>,
        token: u64,
    }

    impl InputHandle for MockInputHandle {}

    impl Drop for MockInputHandle {
        fn drop(&mut self) {
            let token = self.token;
            self.state
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .senders
                .retain(|(open, _)| *open != token);
        }
    }

    #[derive(Default)]
    pub(crate) struct MockState {
        pub input_ports: Vec<MidiPortInfo>,
        pub output_ports: Vec<MidiPortInfo>,
        pub senders: Vec<(u64, PortSender)>,
        pub next_token: u64,
        pub sent_outputs: VecDeque<(String, Vec<u8>)>,
    }

    pub(crate) struct MockMidiBackend {
        pub state: Arc<Mutex<MockState>>,
    }

    impl MockMidiBackend {
        pub(crate) fn new(input_ports: Vec<MidiPortInfo>, output_ports: Vec<MidiPortInfo>) -> Self {
            Self {
                state: Arc::new(Mutex::new(MockState {
                    input_ports,
                    output_ports,
                    senders: Vec::new(),
                    next_token: 0,
                    sent_outputs: VecDeque::new(),
                })),
            }
        }

        pub(crate) fn set_input_ports(&self, ports: Vec<MidiPortInfo>) {
            self.state
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .input_ports = ports;
        }

        pub(crate) fn set_output_ports(&self, ports: Vec<MidiPortInfo>) {
            self.state
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .output_ports = ports;
        }

        pub(crate) async fn inject_all(&self, ts: u64, data: Vec<u8>) {
            let senders: Vec<PortSender> = {
                let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
                state
                    .senders
                    .iter()
                    .map(|(_, sender)| sender)
                    .filter(|s| !s.is_closed())
                    .cloned()
                    .collect()
            };
            for sender in senders {
                let _ = sender.send((ts, data.clone())).await;
            }
        }

        pub(crate) fn sent_outputs(&self) -> Vec<(String, Vec<u8>)> {
            self.state
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .sent_outputs
                .iter()
                .cloned()
                .collect()
        }
    }

    impl MidiBackend for MockMidiBackend {
        fn list_input_ports(&self) -> Vec<MidiPortInfo> {
            self.state
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .input_ports
                .clone()
        }

        fn list_output_ports(&self) -> Vec<MidiPortInfo> {
            self.state
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .output_ports
                .clone()
        }

        fn open_input(
            &self,
            port_name: &str,
            event_tx: mpsc::Sender<(u64, Vec<u8>)>,
        ) -> Result<Box<dyn InputHandle>, MidiError> {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            if !state.input_ports.iter().any(|p| p.name == port_name) {
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
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
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
    }
}
