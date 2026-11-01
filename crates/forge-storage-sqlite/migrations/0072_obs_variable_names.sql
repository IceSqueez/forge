UPDATE actions
SET sub_actions = REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(sub_actions, '%obs.scene_names%', '%obs.scenes.names%'), '%obs.scenes.all_names%', '%obs.scenes.names%'), '%obs.scenes.current%', '%obs.scene%'), '%obs.sources.all_names%', '%obs.sources.names%'), '%obs.collection.all_names%', '%obs.collections.names%'), '%obs.profile.all_names%', '%obs.profiles.names%'), '%obs.input.settings%', '%obs.source.settings%'), '%obs.input.kind%', '%obs.source.kind%')
WHERE sub_actions LIKE '%\%obs.%' ESCAPE '\';

UPDATE trigger_instances
SET overrides = REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(overrides, '%obs.scene_names%', '%obs.scenes.names%'), '%obs.scenes.all_names%', '%obs.scenes.names%'), '%obs.scenes.current%', '%obs.scene%'), '%obs.sources.all_names%', '%obs.sources.names%'), '%obs.collection.all_names%', '%obs.collections.names%'), '%obs.profile.all_names%', '%obs.profiles.names%'), '%obs.input.settings%', '%obs.source.settings%'), '%obs.input.kind%', '%obs.source.kind%')
WHERE overrides LIKE '%\%obs.%' ESCAPE '\';
