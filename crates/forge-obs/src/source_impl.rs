use async_trait::async_trait;

use crate::client::ObsClient;
use crate::error::ObsError;
use crate::source::{ObsSource, SourceInfo};

#[async_trait]
impl ObsSource for ObsClient {
    async fn scenes(&self) -> Result<Vec<String>, ObsError> {
        let catalog = self
            .catalog_state
            .try_read()
            .map_err(|_| ObsError::Protocol("catalog lock poisoned".to_owned()))?;
        Ok(catalog.scenes.clone())
    }

    async fn current_scene(&self) -> Result<Option<String>, ObsError> {
        let catalog = self
            .catalog_state
            .try_read()
            .map_err(|_| ObsError::Protocol("catalog lock poisoned".to_owned()))?;
        Ok(catalog.current_scene.clone())
    }

    async fn sources(&self, scene: &str) -> Result<Vec<SourceInfo>, ObsError> {
        let catalog = self
            .catalog_state
            .try_read()
            .map_err(|_| ObsError::Protocol("catalog lock poisoned".to_owned()))?;
        Ok(catalog
            .sources
            .get(scene)
            .map(|v| {
                v.iter()
                    .map(|s| SourceInfo {
                        name: s.name.clone(),
                        visible: s.visible,
                        locked: s.locked,
                        audio_db: s.audio_db,
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn audio_inputs(&self) -> Result<Vec<String>, ObsError> {
        let catalog = self
            .catalog_state
            .try_read()
            .map_err(|_| ObsError::Protocol("catalog lock poisoned".to_owned()))?;
        Ok(catalog.audio_inputs.clone())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::client::ObsClient;
    use crate::source::{ObsSource, SourceInfo};

    #[tokio::test]
    async fn scenes_returns_catalog_scenes() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        {
            let mut catalog = client.catalog_state.write().unwrap();
            catalog.scenes = vec!["Gameplay".to_owned(), "BRB".to_owned()];
        }
        let scenes = client.scenes().await.unwrap();
        assert_eq!(scenes, vec!["Gameplay", "BRB"]);
    }

    #[tokio::test]
    async fn current_scene_returns_catalog_current() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        {
            let mut catalog = client.catalog_state.write().unwrap();
            catalog.current_scene = Some("Gameplay".to_owned());
        }
        let scene = client.current_scene().await.unwrap();
        assert_eq!(scene, Some("Gameplay".to_owned()));
    }

    #[tokio::test]
    async fn current_scene_returns_none_when_unset() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        let scene = client.current_scene().await.unwrap();
        assert_eq!(scene, None);
    }

    #[tokio::test]
    async fn sources_returns_empty_for_unknown_scene() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        let sources = client.sources("NonExistent").await.unwrap();
        assert!(sources.is_empty());
    }

    #[tokio::test]
    async fn sources_returns_cached_sources() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        {
            let mut catalog = client.catalog_state.write().unwrap();
            catalog.sources.insert(
                "Gameplay".to_owned(),
                vec![SourceInfo {
                    name: "Game Capture".to_owned(),
                    visible: true,
                    locked: false,
                    audio_db: None,
                }],
            );
        }
        let sources = client.sources("Gameplay").await.unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].name, "Game Capture");
    }

    #[tokio::test]
    async fn audio_inputs_returns_catalog_inputs() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        {
            let mut catalog = client.catalog_state.write().unwrap();
            catalog.audio_inputs = vec!["Mic/Aux".to_owned(), "Desktop Audio".to_owned()];
        }
        let inputs = client.audio_inputs().await.unwrap();
        assert_eq!(inputs, vec!["Mic/Aux", "Desktop Audio"]);
    }
}
