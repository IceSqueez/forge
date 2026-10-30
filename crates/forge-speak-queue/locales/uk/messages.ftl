msg-speak-engine_not_registered = Рушій озвучення { $engine } не налаштований.
msg-speak-synthesis_timed_out = Рушій озвучення не відповів за { $seconds } { $seconds ->
    [one] секунду
    [few] секунди
   *[other] секунд
}.
msg-speak-engine_auth_failed = Рушій озвучення відхилив вхід. Перевірте його облікові дані.
msg-speak-engine_unavailable = Рушій озвучення { $engine } недоступний.
msg-speak-engine_rate_limited = Рушій озвучення перевантажений. Спробуйте за { $seconds } { $seconds ->
    [one] секунду
    [few] секунди
   *[other] секунд
}.
msg-speak-voice_not_recognized = Рушій озвучення не знає голосу { $voice }.
msg-speak-network_failed = Не вдалося зв'язатися з рушієм озвучення через мережу.
msg-speak-ssml_unsupported = Рушій озвучення { $engine } не підтримує SSML.
msg-speak-engine_io_failed = Рушій озвучення не зміг прочитати чи записати свої дані.
msg-speak-quota_exhausted = Рушій озвучення { $engine } вичерпав ліміт.
msg-speak-synthesis_failed = Рушій озвучення { $engine } не зміг озвучити це повідомлення.
msg-speak-no_output_device = Немає жодного пристрою виведення звуку.
msg-speak-audio_host_failed = Звукова система повідомила про помилку.
msg-speak-audio_device_lost = Пристрій виведення звуку зупинився під час відтворення.
msg-speak-audio_conversion_failed = Не вдалося перетворити озвучений звук для пристрою виведення.
msg-speak-audio_decode_failed = Не вдалося розкодувати озвучений звук.
msg-speak-audio_io_failed = Не вдалося прочитати чи записати озвучений звук.
msg-speak-playback_task_failed = Завдання відтворення несподівано зупинилося.
msg-speak-no_audio_route = Для озвучення не налаштовано жодного звукового маршруту.
msg-speak-audio_route_unknown = Звукового маршруту { $route } не існує.
msg-speak-audio_routes_failed = Усі звукові маршрути для озвучення не спрацювали.
msg-speak-audio_remote_failed = Віддалене місце призначення звуку не спрацювало.
msg-speak-audio_start_timed_out = Відтворення звуку не почалося за { $seconds } с.
msg-speak-no_overlay_listener = Жодна відкрита сторінка не слухає звуковий оверлей { $destination }.
msg-speak-skipped_rule_matched = Пропущено: { $rule ->
    [contains_url] повідомлення містить посилання.
    [starts_with_prefix] повідомлення починається з пропущеного префікса.
    [from_bot_account] повідомлення від бот-акаунта.
    [exceeds_max_length] повідомлення занадто довге.
    [repeats_recent_message] повідомлення повторює недавнє.
    [emote_only] повідомлення лише з емоцій.
    [mostly_non_latin] повідомлення переважно не латиницею.
    [custom_regex] повідомлення підпало під власний шаблон пропуску.
   *[other] повідомлення підпало під правило пропуску.
}
msg-speak-skipped_word_filter = Пропущено: повідомлення заблоковано фільтром слів.
msg-speak-skipped_empty = Пропущено: після фільтрації нічого озвучувати.
msg-speak-skipped_alias = Пропущено: глядача заблоковано голосовим псевдонімом.
msg-speak-skipped_no_voices = Пропущено: немає доступних голосів.
msg-speak-skipped_voice_override_missing = Пропущено: вибраний голос не встановлений.
msg-speak-skipped_voice_unresolved = Пропущено: для цього повідомлення не вдалося вибрати голос.
msg-speak-skipped_by_user = Пропущено вами.
msg-speak-skipped_promoted = Пропущено, бо ви висунули вперед інше повідомлення.
msg-speak-skipped_cancelled = Пропущено: запит скасовано.
msg-speak-skipped_cleared = Зупинено під час очищення черги.
msg-speak-rejected_queue_full = Не додано до черги: черга заповнена (не більше { $limit }).
msg-speak-rejected_per_user_limit = Не додано до черги: у цього глядача вже { $limit } у черзі.
msg-speak-queue_stopped = Чергу озвучення зупинено.
msg-speak-queue_lagging = Черга озвучення відстала й втратила події.
msg-speak-wait_cancelled = Очікування озвучення скасовано.
msg-speak-event_stream_closed = Черга озвучення закрилася, не дочекавшись кінця цього повідомлення.
msg-speak-outcome_unknown = Невідомо, чи озвучено це повідомлення: черга втратила події під час очікування.
msg-speak-wait_timed_out = Очікування озвучення тривало надто довго.
msg-speak-removed_from_queue = Повідомлення прибрано з черги.
msg-speak-show_speech_unavailable = Озвучення для показу оверлея недоступне.
