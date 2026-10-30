msg-speak-engine_not_registered = The speech engine { $engine } is not set up.
msg-speak-synthesis_timed_out = The speech engine did not answer within { $seconds ->
    [one] { $seconds } second
   *[other] { $seconds } seconds
}.
msg-speak-engine_auth_failed = The speech engine rejected the sign-in. Check its credentials.
msg-speak-engine_unavailable = The speech engine { $engine } is unavailable.
msg-speak-engine_rate_limited = The speech engine is busy. Try again in { $seconds ->
    [one] { $seconds } second
   *[other] { $seconds } seconds
}.
msg-speak-voice_not_recognized = The speech engine does not know the voice { $voice }.
msg-speak-network_failed = The speech engine could not be reached over the network.
msg-speak-ssml_unsupported = The speech engine { $engine } does not support SSML.
msg-speak-engine_io_failed = The speech engine failed to read or write its data.
msg-speak-quota_exhausted = The speech engine { $engine } has run out of quota.
msg-speak-synthesis_failed = The speech engine { $engine } could not speak this message.
msg-speak-no_output_device = No audio output device is available.
msg-speak-audio_host_failed = The audio system reported an error.
msg-speak-audio_device_lost = The audio output device stopped during playback.
msg-speak-audio_conversion_failed = The speech audio could not be converted for the output device.
msg-speak-audio_decode_failed = The speech audio could not be decoded.
msg-speak-audio_io_failed = The speech audio could not be read or written.
msg-speak-playback_task_failed = The playback task stopped unexpectedly.
msg-speak-no_audio_route = No audio route is set up for speech.
msg-speak-audio_route_unknown = The audio route { $route } does not exist.
msg-speak-audio_routes_failed = Every audio route for speech failed.
msg-speak-audio_remote_failed = The remote audio destination failed.
msg-speak-no_overlay_listener = No open page is listening on the audio overlay { $destination }.
msg-speak-skipped_rule_matched = Skipped: { $rule ->
    [contains_url] the message contains a link.
    [starts_with_prefix] the message starts with a skipped prefix.
    [from_bot_account] the message is from a bot account.
    [exceeds_max_length] the message is too long.
    [repeats_recent_message] the message repeats a recent one.
    [emote_only] the message has only emotes.
    [mostly_non_latin] the message is mostly non-Latin script.
    [custom_regex] the message matched a custom skip pattern.
   *[other] the message matched a skip rule.
}
msg-speak-skipped_word_filter = Skipped: the message was blocked by the word filter.
msg-speak-skipped_empty = Skipped: nothing was left to say after filtering.
msg-speak-skipped_alias = Skipped: the viewer is blocked by a voice alias.
msg-speak-skipped_no_voices = Skipped: no voices are available.
msg-speak-skipped_voice_override_missing = Skipped: the chosen voice is not installed.
msg-speak-skipped_voice_unresolved = Skipped: no voice could be chosen for this message.
msg-speak-skipped_by_user = Skipped by you.
msg-speak-skipped_promoted = Skipped because you moved another message ahead.
msg-speak-skipped_cancelled = Skipped: the request was cancelled.
msg-speak-skipped_cleared = Stopped when the queue was cleared.
msg-speak-rejected_queue_full = Not queued: the queue is full ({ $limit } at most).
msg-speak-rejected_per_user_limit = Not queued: this viewer already has { $limit } waiting.
msg-speak-queue_stopped = The speech queue has stopped.
msg-speak-queue_lagging = The speech queue fell behind and dropped events.
msg-speak-wait_cancelled = Waiting for the speech was cancelled.
msg-speak-event_stream_closed = The speech queue closed before this message finished.
msg-speak-outcome_unknown = It is unknown whether this message was spoken: the queue dropped events while waiting.
msg-speak-wait_timed_out = Waiting for the speech took too long.
msg-speak-removed_from_queue = The message was removed from the queue.
msg-speak-show_speech_unavailable = Speech for an overlay show is not available.
