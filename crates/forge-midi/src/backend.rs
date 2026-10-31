use forge_types::RecoverMutex;
use std::collections::HashMap;
use std::sync::Mutex;

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

    fn release_output(&self, _port_name: &str) {}
}

pub(crate) struct MidirBackend {
    client_name: String,
    outputs: Mutex<HashMap<String, midir::MidiOutputConnection>>,
}

impl MidirBackend {
    pub(crate) fn new(client_name: impl Into<String>) -> Result<Self, MidiError> {
        let client_name = client_name.into();
        midir::MidiInput::new(&client_name).map_err(|e| MidiError::MidirInit(e.to_string()))?;
        Ok(Self {
            client_name,
            outputs: Mutex::new(HashMap::new()),
        })
    }

    fn connect_output(&self, port_name: &str) -> Result<midir::MidiOutputConnection, MidiError> {
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
        output
            .connect(&port, "forge-midi")
            .map_err(|e| MidiError::MidirConnect(e.to_string()))
    }
}

struct MidirInputHandle(
    #[expect(dead_code, reason = "held to keep the input connection open")]
    midir::MidiInputConnection<()>,
);

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
                    if let Err(mpsc::error::TrySendError::Full(_)) =
                        event_tx.try_send((timestamp, data.to_vec()))
                    {
                        tracing::warn!("midi input queue full; event dropped");
                    }
                },
                (),
            )
            .map_err(|e| MidiError::MidirConnect(e.to_string()))?;
        Ok(Box::new(MidirInputHandle(conn)))
    }

    fn send_output(&self, port_name: &str, data: &[u8]) -> Result<(), MidiError> {
        let mut outputs = self.outputs.lock_recover();
        let reused = outputs
            .remove(port_name)
            .and_then(|mut conn| conn.send(data).is_ok().then_some(conn));
        let conn = match reused {
            Some(conn) => conn,
            None => {
                let mut conn = self.connect_output(port_name)?;
                conn.send(data)
                    .map_err(|e| MidiError::OutputSend(e.to_string()))?;
                conn
            }
        };
        outputs.insert(port_name.to_owned(), conn);
        Ok(())
    }

    fn release_output(&self, port_name: &str) {
        self.outputs.lock_recover().remove(port_name);
    }
}
