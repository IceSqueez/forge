-- Trovo decommissioned by upstream: full streaming-features shutdown by 2026-06-30
-- (platform refocus on gaming; new-streamer onboarding stopped 2026-04-01).
-- Drop orphan rows that referenced the removed Trovo integration.

DELETE FROM credentials WHERE id LIKE 'trovo:%';
DELETE FROM viewers WHERE platform = 'trovo';
DELETE FROM action_history WHERE source = 'trovo';
DELETE FROM event_log WHERE source = 'trovo';
