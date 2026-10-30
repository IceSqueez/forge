msg-twitch-health_chat = Чат IRC
msg-twitch-health_eventsub = EventSub
msg-twitch-health_api_calls = Виклики API
msg-twitch-health_chat_joined = Приєднано
msg-twitch-health_eventsub_subs = { $count ->
    [one] { $count } підписка
    [few] { $count } підписки
    [many] { $count } підписок
   *[other] { $count } підписки
}
msg-twitch-health_viewers_live = в ефірі
msg-twitch-health_viewers_offline = офлайн
msg-twitch-health_resets_in = скидається за { $seconds } с
msg-twitch-health_no_usage = ще немає даних
msg-twitch-rate_limited = Ліміт запитів Twitch вичерпано, спробуйте трохи згодом
msg-twitch-reauth_required = Вхід у Twitch застарів, увійдіть знову
msg-twitch-credentials_unavailable = Не вдалося прочитати облікові дані Twitch
msg-twitch-request_refused = Twitch відхилив запит (HTTP { $status })
msg-twitch-request_failed = Запит до Twitch не вдався
msg-twitch-response_unreadable = Twitch надіслав відповідь, яку forge не зміг прочитати
msg-twitch-field_required = Поле { $field } обов'язкове
msg-twitch-target_empty = Поля { $login } і { $id } порожні після підстановки змінних
msg-twitch-category_empty = Поля category_id і category_name порожні після підстановки змінних
msg-twitch-category_not_found = Категорію не знайдено: { $category }
msg-twitch-empty_response = Twitch повернув порожню відповідь
msg-twitch-response_field_missing = У відповіді Twitch немає поля { $field }
msg-twitch-message_empty = Повідомлення порожнє після підстановки змінних
msg-twitch-message_too_long = Повідомлення довше за { $limit } символів
msg-twitch-chat_not_connected = Чат Twitch не підключено
msg-twitch-bot_not_signed_in = Акаунт бота не підключено
msg-twitch-bot_reauth_required = Акаунту бота потрібно увійти знову
msg-twitch-bot_credentials_unreadable = Не вдалося прочитати облікові дані бота
msg-twitch-bot_phone_unverified = В акаунта бота немає підтвердженого номера телефону, тому Twitch відхилив шепіт
msg-twitch-bot_not_moderator = Бот не є модератором, призначте його в розділі Інтеграції - Twitch
msg-twitch-permission_denied = Twitch відхилив запит
msg-twitch-message_forbidden = Twitch відхилив повідомлення
msg-twitch-message_dropped = Twitch відкинув повідомлення ({ $code })
msg-twitch-recipient_empty = Одержувача шепоту не вказано
msg-twitch-recipient_rejects_whispers = Одержувач не приймає шепіт
msg-twitch-shoutout_cooldown = Шаутаут на перезарядці, спробуйте знову за { $seconds } с
msg-twitch-unresolved_reference = Містить непідставлену змінну
msg-twitch-invalid_hex_color = Має бути кольором у форматі #RRGGBB
msg-twitch-condition_any_shared_chat_message = будь-яке повідомлення спільного чату
msg-twitch-condition_any_profile_change = будь-яка зміна профілю
msg-twitch-condition_any_reward = будь-яка нагорода
msg-twitch-condition_any_amount = будь-яка сума
msg-twitch-condition_any_state = будь-який стан
msg-twitch-condition_any_whisper = будь-який шепіт
msg-twitch-condition_any_status = будь-який статус
msg-twitch-condition_reward_id = id нагороди = { $reward }
msg-twitch-condition_reward = нагорода = { $reward }
msg-twitch-condition_state = стан = { $state }
msg-twitch-condition_status = статус = { $status ->
    [fulfilled] виконано
    [canceled] скасовано
    [approved] схвалено
    [denied] відхилено
    [expired] прострочено
   *[other] { $status }
}
msg-twitch-condition_whisper_from = від { $sender }
msg-twitch-condition_whisper_text = текст збігається з "{ $text }"
msg-twitch-condition_amount_at_least = сума >= { $min } { $min ->
    [one] цент
    [few] центи
   *[many] центів
}
msg-twitch-condition_bits_at_least = >= { $min } { $min ->
    [one] біт
    [few] біти
   *[many] бітів
}
msg-twitch-condition_level_at_least = рівень >= { $level }
msg-twitch-condition_bits_between = { $min }-{ $max } { $max ->
    [one] біт
    [few] біти
   *[many] бітів
}
