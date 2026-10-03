ALTER TABLE trigger_instances
ADD COLUMN platform_scope TEXT NOT NULL DEFAULT '"any"';
