UPDATE soundboard_clips
SET hotkey = NULL
WHERE hotkey IS NOT NULL;
