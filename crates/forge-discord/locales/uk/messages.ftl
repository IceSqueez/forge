msg-discord-health_webhook_latency = Затримка вебхука
msg-discord-health_rate_limit = Ліміт запитів
msg-discord-health_last_send = Остання відправка
msg-discord-health_errors = Помилки
msg-discord-network_failed = Не вдалося зв'язатися з Discord. Перевірте мережеве з'єднання.
msg-discord-rate_limited = Discord обмежує частоту запитів до цього вебхука. Спробуйте знову через { $seconds } с.
msg-discord-webhook_not_found = Вебхук "{ $webhook }" не налаштований.
msg-discord-serialization_failed = Не вдалося закодувати повідомлення для Discord.
msg-discord-credential_failed = Не вдалося прочитати збережений вебхук.
msg-discord-http_rejected = Discord відхилив запит (HTTP { $status }).
msg-discord-embed_color_out_of_range = Колір ембеда { $color } перевищує максимум 0xFFFFFF.
msg-discord-embed_too_many_fields = В ембеді { $count } полів, а ліміт - { $max }.
msg-discord-embed_part_too_long = Частина ембеда { $part } має { $length } символів, а ліміт - { $max }.
msg-discord-embed_total_too_long = Текст ембеда має { $total } символів загалом, а ліміт - { $max }.
msg-discord-embed_control_character = Частина ембеда { $part } містить керівний символ, який Discord не приймає.
msg-discord-embed_image_url_scheme = Частина ембеда { $part } має починатися з http://, https:// або attachment://.
msg-discord-message_id_invalid = Ідентифікатор повідомлення не схожий на ідентифікатор повідомлення Discord.
msg-discord-not_webhook_endpoint = Ця адреса не є адресою вебхука Discord.
msg-discord-webhook_name_empty = Назва вебхука порожня.
msg-discord-body_encoding_failed = Не вдалося закодувати тіло запиту для Discord.
msg-discord-file_name_unavailable = Не вдалося взяти назву файлу з { $path }.
msg-discord-file_unreadable = Не вдалося прочитати файл { $path }.
msg-discord-health_latency_millis = { $millis } мс
msg-discord-health_latency_samples = { $count ->
    [one] p50 останньої { $count } відправки
    [few] p50 останніх { $count } відправок
   *[other] p50 останніх { $count } відправок
}
msg-discord-health_reset_in = скидання через { $span }
msg-discord-health_send_ok = OK
msg-discord-health_send_failed = Помилка
msg-discord-health_send_none = Ще не надсилалось
msg-discord-health_errors_window = за останню годину
msg-discord-file_not_uploadable = Файл { $path } не є звичайним файлом у межах ліміту розміру завантаження.
msg-discord-content_too_long = Повідомлення має { $length } символів, ліміт { $max }.
msg-discord-nothing_to_edit = Для редагування потрібен текст повідомлення або embed.
msg-discord-label_edit_message = Редагувати повідомлення
msg-discord-label_embed = картка
msg-discord-label_error = ПОМИЛКА
msg-discord-label_ok = OK
msg-discord-label_post_embed = Надіслати картку
msg-discord-label_post_text = Надіслати текст
msg-discord-label_ready = ГОТОВО
msg-discord-label_recent_posts = Останні дописи
msg-discord-label_test_webhook = Перевірити вебхук
msg-discord-label_webhooks = Вебхуки
