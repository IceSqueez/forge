msg-twitch-health_chat = Chat IRC
msg-twitch-health_eventsub = EventSub
msg-twitch-health_api_calls = API Calls
msg-twitch-health_chat_joined = Joined
msg-twitch-health_eventsub_subs = { $count ->
    [one] { $count } subscription
   *[other] { $count } subscriptions
}
msg-twitch-health_viewers_live = live
msg-twitch-health_viewers_offline = offline
msg-twitch-health_resets_in = resets in { $seconds } s
msg-twitch-health_no_usage = no data yet
msg-twitch-rate_limited = Twitch rate limit reached, try again shortly
msg-twitch-reauth_required = Twitch sign-in expired, sign in again
msg-twitch-credentials_unavailable = Twitch credentials could not be read
msg-twitch-request_refused = Twitch refused the request (HTTP { $status })
msg-twitch-request_failed = The request to Twitch failed
msg-twitch-response_unreadable = Twitch sent a response forge could not read
msg-twitch-field_required = { $field } is required
msg-twitch-target_empty = Both { $login } and { $id } are empty after variable substitution
msg-twitch-category_empty = Both category_id and category_name are empty after variable substitution
msg-twitch-empty_response = Twitch returned an empty response
msg-twitch-response_field_missing = The Twitch response has no { $field }
msg-twitch-message_empty = The message is empty after variable substitution
msg-twitch-message_too_long = The message is longer than { $limit } characters
msg-twitch-chat_not_connected = Twitch chat is not connected
msg-twitch-bot_not_signed_in = No bot account is signed in
msg-twitch-bot_reauth_required = The bot account needs to sign in again
msg-twitch-bot_credentials_unreadable = The bot credentials could not be read
msg-twitch-bot_phone_unverified = The bot account has no verified phone number, so Twitch refused the whisper
msg-twitch-bot_not_moderator = The bot is not a moderator, make it one in Integrations - Twitch
msg-twitch-permission_denied = Twitch refused the request
msg-twitch-message_forbidden = Twitch refused the message
msg-twitch-message_dropped = Twitch dropped the message ({ $code })
msg-twitch-recipient_empty = The whisper recipient is empty
msg-twitch-recipient_rejects_whispers = The recipient does not accept whispers
msg-twitch-shoutout_cooldown = A shoutout is on cooldown, try again in { $seconds } s
msg-twitch-unresolved_reference = Has an unresolved variable reference
msg-twitch-invalid_hex_color = Must be a #RRGGBB color
msg-twitch-condition_any_shared_chat_message = any shared chat message
msg-twitch-condition_any_profile_change = any profile change
msg-twitch-condition_any_reward = any reward
msg-twitch-condition_any_amount = any amount
msg-twitch-condition_any_state = any state
msg-twitch-condition_any_whisper = any whisper
msg-twitch-condition_any_status = any status
msg-twitch-condition_reward_id = reward id = { $reward }
msg-twitch-condition_reward = reward = { $reward }
msg-twitch-condition_state = state = { $state }
msg-twitch-condition_status = status = { $status }
msg-twitch-condition_whisper_from = from { $sender }
msg-twitch-condition_whisper_text = text matches "{ $text }"
msg-twitch-condition_amount_at_least = amount >= { $min } cents
msg-twitch-condition_bits_at_least = >= { $min } bits
msg-twitch-condition_level_at_least = level >= { $level }
msg-twitch-condition_bits_between = { $min }-{ $max } bits
