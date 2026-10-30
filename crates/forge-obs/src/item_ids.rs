use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct SceneItemIds {
    sources_by_item: HashMap<(String, i64), String>,
    items_by_source: HashMap<(String, String), i64>,
}

impl SceneItemIds {
    pub(crate) fn insert(&mut self, scene: &str, item_id: i64, source: &str) {
        self.remove_item(scene, item_id);
        self.sources_by_item
            .insert((scene.to_owned(), item_id), source.to_owned());
        self.items_by_source
            .insert((scene.to_owned(), source.to_owned()), item_id);
    }

    pub(crate) fn item_id(&self, scene: &str, source: &str) -> Option<i64> {
        self.items_by_source
            .get(&(scene.to_owned(), source.to_owned()))
            .copied()
    }

    pub(crate) fn source_name(&self, scene: &str, item_id: u64) -> Option<String> {
        let item_id = i64::try_from(item_id).ok()?;
        self.sources_by_item
            .get(&(scene.to_owned(), item_id))
            .cloned()
    }

    pub(crate) fn remove_item(&mut self, scene: &str, item_id: i64) {
        if let Some(source) = self.sources_by_item.remove(&(scene.to_owned(), item_id)) {
            self.items_by_source.remove(&(scene.to_owned(), source));
        }
    }

    pub(crate) fn remove_source(&mut self, source: &str) {
        self.sources_by_item.retain(|_, name| name != source);
        self.items_by_source.retain(|(_, name), _| name != source);
    }

    pub(crate) fn rename_source(&mut self, old: &str, new: &str) {
        for name in self.sources_by_item.values_mut() {
            if name == old {
                new.clone_into(name);
            }
        }
        let renamed: Vec<((String, String), i64)> = self
            .items_by_source
            .iter()
            .filter(|((_, name), _)| name == old)
            .map(|(key, id)| (key.clone(), *id))
            .collect();
        for ((scene, _), id) in renamed {
            self.items_by_source
                .remove(&(scene.clone(), old.to_owned()));
            self.items_by_source.insert((scene, new.to_owned()), id);
        }
    }

    pub(crate) fn remove_scene(&mut self, scene: &str) {
        self.sources_by_item.retain(|(name, _), _| name != scene);
        self.items_by_source.retain(|(name, _), _| name != scene);
    }

    pub(crate) fn rename_scene(&mut self, old: &str, new: &str) {
        let items: Vec<(i64, String)> = self
            .sources_by_item
            .iter()
            .filter(|((scene, _), _)| scene == old)
            .map(|((_, id), source)| (*id, source.clone()))
            .collect();
        self.remove_scene(old);
        for (id, source) in items {
            self.insert(new, id, &source);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.sources_by_item.clear();
        self.items_by_source.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolves(ids: &SceneItemIds, scene: &str, item_id: i64, source: &str) -> bool {
        ids.item_id(scene, source) == Some(item_id)
            && ids.source_name(scene, item_id as u64).as_deref() == Some(source)
    }

    fn forgets(ids: &SceneItemIds, scene: &str, item_id: i64, source: &str) -> bool {
        ids.item_id(scene, source).is_none() && ids.source_name(scene, item_id as u64).is_none()
    }

    fn two_scenes_sharing_a_source() -> SceneItemIds {
        let mut ids = SceneItemIds::default();
        ids.insert("Gameplay", 1, "Mic");
        ids.insert("Gameplay", 2, "Webcam");
        ids.insert("BRB", 1, "Mic");
        ids
    }

    #[test]
    fn an_inserted_item_resolves_from_its_name_and_from_its_id() {
        let ids = two_scenes_sharing_a_source();

        assert!(resolves(&ids, "Gameplay", 2, "Webcam"));
        assert!(forgets(&ids, "BRB", 2, "Webcam"));
    }

    #[test]
    fn an_item_id_past_the_signed_range_resolves_to_nothing() {
        let ids = two_scenes_sharing_a_source();

        assert_eq!(ids.source_name("Gameplay", u64::MAX), None);
    }

    #[test]
    fn reinserting_an_id_under_a_new_source_retires_the_old_name() {
        let mut ids = two_scenes_sharing_a_source();

        ids.insert("Gameplay", 2, "Replay");

        assert!(resolves(&ids, "Gameplay", 2, "Replay"));
        assert_eq!(ids.item_id("Gameplay", "Webcam"), None);
    }

    #[test]
    fn removing_an_item_keeps_the_same_source_in_other_scenes() {
        let mut ids = two_scenes_sharing_a_source();

        ids.remove_item("Gameplay", 1);

        assert!(forgets(&ids, "Gameplay", 1, "Mic"));
        assert!(resolves(&ids, "BRB", 1, "Mic"));
    }

    #[test]
    fn removing_a_source_clears_it_from_every_scene_and_leaves_the_rest() {
        let mut ids = two_scenes_sharing_a_source();

        ids.remove_source("Mic");

        assert!(forgets(&ids, "Gameplay", 1, "Mic"));
        assert!(forgets(&ids, "BRB", 1, "Mic"));
        assert!(resolves(&ids, "Gameplay", 2, "Webcam"));
    }

    #[test]
    fn renaming_a_source_rekeys_both_directions_in_every_scene() {
        let mut ids = two_scenes_sharing_a_source();

        ids.rename_source("Mic", "Studio Mic");

        for scene in ["Gameplay", "BRB"] {
            assert!(resolves(&ids, scene, 1, "Studio Mic"), "{scene}");
            assert_eq!(ids.item_id(scene, "Mic"), None, "{scene}");
        }
        assert!(resolves(&ids, "Gameplay", 2, "Webcam"));
    }

    #[test]
    fn removing_a_scene_keeps_the_other_scenes() {
        let mut ids = two_scenes_sharing_a_source();

        ids.remove_scene("Gameplay");

        assert!(forgets(&ids, "Gameplay", 2, "Webcam"));
        assert!(resolves(&ids, "BRB", 1, "Mic"));
    }

    #[test]
    fn renaming_a_scene_moves_every_item_and_leaves_the_other_scenes() {
        let mut ids = two_scenes_sharing_a_source();

        ids.rename_scene("Gameplay", "Live");

        assert!(resolves(&ids, "Live", 1, "Mic"));
        assert!(resolves(&ids, "Live", 2, "Webcam"));
        assert!(forgets(&ids, "Gameplay", 1, "Mic"));
        assert!(forgets(&ids, "Gameplay", 2, "Webcam"));
        assert!(resolves(&ids, "BRB", 1, "Mic"));
    }

    #[test]
    fn renaming_a_scene_onto_itself_keeps_its_items() {
        let mut ids = two_scenes_sharing_a_source();

        ids.rename_scene("Gameplay", "Gameplay");

        assert!(resolves(&ids, "Gameplay", 1, "Mic"));
    }

    #[test]
    fn clearing_forgets_every_scene() {
        let mut ids = two_scenes_sharing_a_source();

        ids.clear();

        assert!(forgets(&ids, "Gameplay", 2, "Webcam"));
        assert!(forgets(&ids, "BRB", 1, "Mic"));
    }
}
