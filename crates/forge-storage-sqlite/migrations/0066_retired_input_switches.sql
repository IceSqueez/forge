INSERT OR IGNORE INTO settings (key, value, portability, disclosure)
SELECT 'integration.enabled.midi', trim(value), 'portable', 'verbatim'
  FROM settings
 WHERE key = 'midi.enabled' AND trim(value) IN ('true', 'false');

INSERT OR IGNORE INTO settings (key, value, portability, disclosure)
SELECT 'integration.enabled.hotkey', trim(value), 'portable', 'verbatim'
  FROM settings
 WHERE key = 'hotkey.enabled' AND trim(value) IN ('true', 'false');

DELETE FROM settings WHERE key IN ('midi.enabled', 'hotkey.enabled');
