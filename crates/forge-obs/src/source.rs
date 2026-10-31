use crate::client::ObsClient;

#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub audio_db: Option<f32>,
    pub kind: Option<String>,
}

impl ObsClient {
    pub fn scene_names(&self) -> Vec<String> {
        self.mirror.read(|catalog| catalog.scenes.clone())
    }

    pub fn scene_items(&self, scene: &str) -> Vec<SourceInfo> {
        self.mirror
            .read(|catalog| catalog.sources.get(scene).cloned())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_names_read_the_mirror() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        client.mirror.write_for_test().scenes = vec!["Gameplay".to_owned(), "BRB".to_owned()];

        assert_eq!(client.scene_names(), ["Gameplay", "BRB"]);
    }

    #[test]
    fn scene_items_are_empty_for_an_unknown_scene() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());

        assert!(client.scene_items("NonExistent").is_empty());
    }

    #[test]
    fn scene_items_read_the_mirrored_rows() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        client.mirror.write_for_test().sources.insert(
            "Gameplay".to_owned(),
            vec![SourceInfo {
                name: "Game Capture".to_owned(),
                visible: true,
                locked: false,
                audio_db: None,
                kind: None,
            }],
        );

        let rows = client.scene_items("Gameplay");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "Game Capture");
    }
}
