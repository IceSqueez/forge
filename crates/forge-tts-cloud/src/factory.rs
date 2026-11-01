use std::marker::PhantomData;

use forge_speech_types::EngineId;
use forge_tts_core::{EngineClass, TtsEngine, TtsEngineFactory, TtsError};

use crate::network::{CloudGateways, ProviderGateway};

pub trait CloudEngine: TtsEngine + Sized + 'static {
    type Credentials: Clone + Send + Sync + 'static;

    const ENGINE_ID: &'static str;

    fn connect(
        credentials: Self::Credentials,
        provider: &ProviderGateway,
    ) -> Result<Self, TtsError>;
}

pub struct CloudEngineFactory<E: CloudEngine> {
    credentials: E::Credentials,
    provider: ProviderGateway,
    engine: PhantomData<fn() -> E>,
}

impl<E: CloudEngine> CloudEngineFactory<E> {
    pub fn new(credentials: E::Credentials, gateways: &CloudGateways) -> Self {
        Self {
            credentials,
            provider: gateways.provider(E::ENGINE_ID),
            engine: PhantomData,
        }
    }
}

impl<E: CloudEngine> TtsEngineFactory for CloudEngineFactory<E> {
    fn engine_id(&self) -> EngineId {
        EngineId(E::ENGINE_ID.into())
    }

    fn class(&self) -> EngineClass {
        EngineClass::Cloud
    }

    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Ok(Box::new(E::connect(
            self.credentials.clone(),
            &self.provider,
        )?))
    }
}
