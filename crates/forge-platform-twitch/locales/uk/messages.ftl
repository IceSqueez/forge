msg-twitch-stored_sign_in_unreadable = Не вдалося прочитати збережений вхід до Twitch
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
msg-twitch-whisper_pattern_invalid = Некоректний регулярний вираз
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
msg-twitch-label_subscription_dropped = Підписку { $kind } втрачено
msg-twitch-label_ad_120_seconds = 120 секунд
msg-twitch-label_ad_150_seconds = 150 секунд
msg-twitch-label_ad_180_seconds = 180 секунд
msg-twitch-label_ad_30_seconds = 30 секунд
msg-twitch-label_ad_60_seconds = 60 секунд
msg-twitch-label_ad_90_seconds = 90 секунд
msg-twitch-label_ad_cooldown_hint = Між рекламою мінімум 8 хв
msg-twitch-label_add_mod = Додати модератора
msg-twitch-label_add_vip = Додати VIP
msg-twitch-label_affiliate = Affiliate
msg-twitch-label_announcement = Оголошення
msg-twitch-label_announcement_text = Текст оголошення
msg-twitch-label_background_color = Колір фону (#RRGGBB)
msg-twitch-label_ban_user = Забанити користувача
msg-twitch-label_blue = Синій
msg-twitch-label_cancel_poll = Скасувати опитування
msg-twitch-label_cancel_raid = Скасувати рейд
msg-twitch-label_cancel_refund = Скасувати й повернути
msg-twitch-label_category = Категорія
msg-twitch-label_category_example = напр. Just Chatting
msg-twitch-label_channel = Канал
msg-twitch-label_channel_name = назва каналу
msg-twitch-label_channel_point_rewards = Нагороди за бали каналу
msg-twitch-label_channel_points = Бали каналу
msg-twitch-label_channel_points_voting = Голосування балами каналу
msg-twitch-label_chat = Чат
msg-twitch-label_choices_one_per_line = Варіанти (по одному в рядку)
msg-twitch-label_clear_chat = Очистити чат
msg-twitch-label_color = Колір
msg-twitch-label_create_stream_marker = Створити маркер стріму
msg-twitch-label_created_outside_forge = Створено поза forge - редагуйте в панелі Twitch
msg-twitch-label_duration = Тривалість
msg-twitch-label_duration_seconds = Тривалість (секунд)
msg-twitch-label_emote_only_mode = Режим лише емоутів
msg-twitch-label_enabled = Увімкнено
msg-twitch-label_end_poll_now = Завершити опитування зараз
msg-twitch-label_end_shield_mode = Вимкнути режим щита
msg-twitch-label_eventsub_subscriptions = Підписки EventSub
msg-twitch-label_followers_only_mode = Режим лише для фоловерів
msg-twitch-label_fulfill_redemption = Виконати активацію
msg-twitch-label_global_cooldown = Загальна затримка, секунди (0 = вимк.)
msg-twitch-label_green = Зелений
msg-twitch-label_lock_prediction = Заблокувати прогноз
msg-twitch-label_manage_rewards = Керувати нагородами
msg-twitch-label_marker_note = Нотатка маркера
msg-twitch-label_max_per_stream = Максимум за стрім (0 = без обмежень)
msg-twitch-label_max_per_viewer = Максимум на глядача за стрім (0 = без обмежень)
msg-twitch-label_message = Повідомлення
msg-twitch-label_min_account_age = Мін. вік акаунта (хвилин)
msg-twitch-label_moderation = Модерація
msg-twitch-label_oauth_scopes = Дозволи OAuth
msg-twitch-label_off = Вимк.
msg-twitch-label_on = Увімк.
msg-twitch-label_one_choice_per_line = Один варіант у рядку
msg-twitch-label_one_outcome_per_line = Один результат у рядку
msg-twitch-label_optional = необов'язково
msg-twitch-label_orange = Помаранчевий
msg-twitch-label_outcomes_one_per_line = Результати (по одному в рядку)
msg-twitch-label_partner = Partner
msg-twitch-label_paused = Призупинено
msg-twitch-label_poll_question = Питання опитування
msg-twitch-label_polls = Опитування
msg-twitch-label_prediction_question = Питання прогнозу
msg-twitch-label_predictions = Прогнози
msg-twitch-label_primary = Основний
msg-twitch-label_prompt = Підказка
msg-twitch-label_purple = Фіолетовий
msg-twitch-label_question = Питання
msg-twitch-label_raid_target = Ціль рейду
msg-twitch-label_raids_ads = Рейди та реклама
msg-twitch-label_reason = Причина
msg-twitch-label_reject_refund = Відхилити й повернути
msg-twitch-label_remove_mod = Прибрати модератора
msg-twitch-label_remove_vip = Прибрати VIP
msg-twitch-label_require_viewer_input = Вимагати введення від глядача
msg-twitch-label_resolve_pay_out = Завершити / виплатити
msg-twitch-label_retry_pending = очікує повтору
msg-twitch-label_reward_cost = Вартість (бали каналу)
msg-twitch-label_reward_title_placeholder = Випий води!
msg-twitch-label_run_commercial = Запустити рекламу
msg-twitch-label_send_message = Надіслати повідомлення
msg-twitch-label_send_shoutout = Надіслати шаут-аут
msg-twitch-label_set_category_game = Задати категорію / гру
msg-twitch-label_shield_scope_missing = Увійдіть у Twitch ще раз, щоб надати доступ до режиму щита
msg-twitch-label_slow_mode = Повільний режим
msg-twitch-label_snooze_next_ad = Відкласти наступну рекламу
msg-twitch-label_standard = Стандартний
msg-twitch-label_start_poll = Почати опитування
msg-twitch-label_start_prediction = Почати прогноз
msg-twitch-label_start_raid = Почати рейд
msg-twitch-label_start_shield_mode = Увімкнути режим щита
msg-twitch-label_stream_info = Інформація про стрім
msg-twitch-label_stream_title = Назва стріму
msg-twitch-label_tags_comma_sep = Теги (через кому)
msg-twitch-label_tags_example = напр. Українська, Відпочинок
msg-twitch-label_tier_locked = Потрібен статус Twitch Affiliate або Partner
msg-twitch-label_timeout_user = Дати тайм-аут користувачу
msg-twitch-label_title = Назва
msg-twitch-label_unban_user = Розбанити користувача
msg-twitch-label_unchanged = Без змін
msg-twitch-label_update_tags = Оновити теги
msg-twitch-label_update_title = Оновити назву
msg-twitch-label_username = Ім'я користувача
msg-twitch-label_username_placeholder = ім'я користувача
msg-twitch-label_wait_time_seconds = Час очікування (секунд)
msg-twitch-label_window_seconds = Вікно (секунд)
msg-twitch-label_queued_reward_hint = Показано нагороди, створені у forge, активації яких чекають у черзі
msg-twitch-label_poll = Опитування
msg-twitch-label_prediction = Прогноз
msg-twitch-label_redemption = Активація
msg-twitch-label_reward = Нагорода
msg-twitch-label_winning_outcome = Переможний варіант
msg-twitch-endpoint_summary = Підключено через код пристрою
msg-twitch-user_id_badge = user_id { $id }
msg-twitch-subscription_incoming = { $kind } (вхідний)
msg-twitch-subscription_outgoing = { $kind } (вихідний)
msg-twitch-subscription_unauthorized = немає авторизації
msg-twitch-subscription_missing_scope = бракує дозволу
msg-twitch-subscription_already_exists = вже існує в іншій сесії
msg-twitch-subscription_refused = HTTP { $status }
msg-twitch-subscription_unreadable = нечитабельна відповідь
msg-twitch-subscription_rate_limited = перевищено ліміт запитів
msg-twitch-subscription_transport = збій з'єднання
msg-twitch-subscription_credentials = облікові дані недоступні
msg-twitch-subscription_revoked = відкликано: { $status }
msg-twitch-reward_title_required = Назва обов'язкова
msg-twitch-reward_color_format = Колір фону має бути порожнім або #RRGGBB
msg-twitch-reward_zero_or_more = Має бути 0 або більше
msg-twitch-reward_duplicate_title = Нагорода з такою назвою вже існує
msg-twitch-reward_automod_rejected = Twitch AutoMod відхилив текст нагороди
msg-twitch-reward_settings_rejected = Twitch відхилив налаштування нагороди
msg-twitch-reward_gone = Цієї нагороди більше не існує
msg-twitch-reward_rejected_per_viewer = Twitch відхилив ліміт на глядача
msg-twitch-reward_rejected_per_stream = Twitch відхилив ліміт на стрім
msg-twitch-reward_rejected_cooldown = Twitch відхилив перезарядку
msg-twitch-reward_rejected_color = Twitch відхилив колір тла
msg-twitch-reward_rejected_prompt = Twitch відхилив підказку
msg-twitch-reward_rejected_cost = Twitch відхилив вартість
msg-twitch-reward_rejected_title = Twitch відхилив назву
msg-twitch-reward_unknown_collection = Невідома колекція
msg-twitch-reward_unknown_toggle = Невідомий перемикач нагороди
msg-twitch-reward_title_max = Назва має містити не більше { $max } символів
msg-twitch-reward_prompt_max = Підказка має містити не більше { $max } символів
msg-twitch-reward_cost_min = Вартість має бути не меншою за { $min }
msg-twitch-unknown_option_list = Невідомий список варіантів: { $list }
