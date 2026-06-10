## Загальні дії, спільні для всіх екранів

common_cancel = Скасувати
common_save = Зберегти
common_language = Мова

## Навігація — назви екранів (хлібні крихти + бічна панель)

nav_home = Головна
nav_actions = Дії
nav_queues = Черги
nav_triggers = Тригери
nav_platforms = Платформи
nav_stream_apps = Стрім-застосунки
nav_builtin = Вбудоване
nav_integration = Інтеграція
nav_live_chat = Живий чат
nav_event_feed = Стрічка подій
nav_globals = Глобальні змінні
nav_settings = Налаштування
nav_tts = TTS
nav_soundboard = Звукова панель
nav_script_editor = Редактор скриптів
nav_api_reference = Довідник API
nav_server = Сервер
nav_logs = Журнал

## Навігація — заголовки секцій бічної панелі

nav_section_audience = АУДИТОРІЯ
nav_section_automation = АВТОМАТИЗАЦІЯ
nav_section_connections = ПІДКЛЮЧЕННЯ

## Навігація — підписи пунктів бічної панелі

nav_item_home = Головна
nav_item_chat = Чат
nav_item_actions = Дії
nav_item_triggers = Тригери
nav_item_queues = Черги
nav_item_event_feed = Стрічка подій
nav_item_globals = Глобальні змінні
nav_item_platforms = Платформи
nav_item_stream_apps = Стрім-застосунки
nav_item_soundboard = Звукова панель
nav_item_tts = Синтез мовлення
nav_item_ws_server = WebSocket-сервер
nav_item_settings = Налаштування

## Навігація — заглушка "незабаром"

nav_coming_soon = Незабаром

## Головна — секція привітання

home_hero_tagline = Відкрита автоматизація стрімів, створена для стрімерів
home_hero_import = Імпортувати
home_hero_new_action = Нова дія

## Головна — картки швидкого переходу

home_card_audience_section = АУДИТОРІЯ
home_card_audience_title = Чат
home_card_audience_stat_label = глядачів відстежується
home_card_audience_hint = Спілкуйтеся з аудиторією та бачте, хто дивиться
home_card_automation_section = АВТОМАТИЗАЦІЯ
home_card_automation_title = Дії
home_card_automation_hint = Налаштовуйте тригери, команди та таймери
home_card_connections_section = ПІДКЛЮЧЕННЯ
home_card_connections_title = Підключення
home_card_connections_stat_label = підключено
home_card_connections_hint = Керуйте платформами, застосунками та модулями

## Головна — картка здоров'я стріму

home_health_title = Стан стріму
home_health_live = ЖИВИЙ
home_health_refresh_hint = остання хвилина · автооновлення
home_health_throughput_label = ПРОПУСКНА ЗДАТНІСТЬ · под/с
home_health_bitrate_label = БІТРЕЙТ · OBS
home_health_dropped_label = ВТРАЧЕНІ · OBS
home_health_fps_label = FPS
home_health_cpu_label = CPU

## Головна — смуга підключень

home_connections_title = Вбудоване

## Головна — статуси підключень

home_conn_connected = підключено
home_conn_offline = офлайн

## Головна — картка останніх подій

home_events_title = Останні події
home_events_empty = Подій ще немає

## Головна — картка швидкого огляду

home_glance_title = Огляд
home_glance_actions = Дії
home_glance_fired = Спрацювань у сесії
home_glance_globals = Глобальні змінні

## Головна — лічильник дій + спрацювань (з множиною)

home_card_automation_stat_label = { $count ->
    [one] { $count } дія · { $fired } спрацювань сьогодні
    [few] { $count } дії · { $fired } спрацювань сьогодні
    [many] { $count } дій · { $fired } спрацювань сьогодні
   *[other] { $count } дій · { $fired } спрацювань сьогодні
}

## Головна — зведення активних / відключених

home_connections_summary = { $active } активних · { $disconnected } відключено

## Налаштування → панель мови

settings_language_title = Мова
settings_language_subtitle = Оберіть, якою мовою Forge говоритиме з вами
