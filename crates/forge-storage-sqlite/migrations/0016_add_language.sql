-- Seed the default language preference so first-run reads via SettingsRepo::language()
-- find a row rather than falling back to the in-code default. INSERT OR IGNORE is
-- idempotent: a row already present (set by the user before this migration) is preserved.
INSERT OR IGNORE INTO settings (key, value) VALUES ('app.language', 'en');
