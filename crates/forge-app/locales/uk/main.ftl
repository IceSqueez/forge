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

## Налаштування → бічна навігація

settings_page_title = Налаштування
settings_nav_group_preferences = ПЕРЕВАГИ
settings_nav_group_engine = РУШІЙ
settings_nav_group_about = ПРО ПРОГРАМУ
settings_nav_appearance = Зовнішній вигляд
settings_nav_language = Мова
settings_nav_shortcuts = Гарячі клавіші
settings_nav_notifications = Сповіщення
settings_nav_audio = Аудіо
settings_nav_scripting = Скрипти
settings_nav_queues = Черги
settings_nav_storage = Сховище
settings_nav_websocket = WebSocket
settings_nav_hotkeys = Хоткеї
settings_nav_version = Версія
settings_nav_diagnostics = Діагностика
settings_coming_soon_placeholder = Незабаром.

## Налаштування → панель діагностики

settings_about_build_label = Збірка
settings_diagnostics_section_title = Журнали та діагностика
settings_diagnostics_log_dir = Тека журналів: { $path }
settings_diagnostics_open_log_dir = Відкрити теку журналів
settings_diagnostics_log_level_hint = Рівень журналювання: керується через змінну RUST_LOG (наприклад: info, debug, trace).

## Налаштування → панель сховища

settings_storage_section_title = Сховище та резервні копії
settings_storage_db_path = База даних: { $path }
settings_storage_vacuum_btn = Вакуумування (компактний знімок)
settings_storage_vacuum_hint = Записує стиснутий знімок у тимчасовий файл; корисно перед ручними резервними копіями.
settings_storage_backup_btn = Резервна копія зараз
settings_storage_backup_hint = Створює копію бази з міткою часу в теці даних.

## Налаштування → панель черг

settings_queues_section_title = Черги та потоки
settings_queues_thread_hint = Пул потоків Tokio: { $workers } потік(ів) (автоматично за системою).
settings_queues_managed_hint = Ліміти паралелізму та прапорці блокування керуються на екрані Черги.

## Налаштування → панель сповіщень

settings_notifications_section_title = Сповіщення
settings_notifications_hint = Налаштування спливаючих підказок за типом події з'явиться пізніше. Помилки та зміни підключень завжди відображаються в рядку стану.

## Налаштування → панель гарячих клавіш (ярлики)

settings_shortcuts_title = Гарячі клавіші
settings_shortcuts_subtitle = Клавіатурні скорочення у Forge
settings_shortcuts_note = Гарячі клавіші ще не прив'язані — тут лише підписи.
settings_shortcut_save = Зберегти
settings_shortcut_new_action = Нова дія
settings_shortcut_quick_switcher = Швидкий перехід
settings_shortcut_toggle_chat = Живий чат
settings_shortcut_toggle_events = Стрічка подій
settings_shortcut_run_script = Запустити скрипт

## Налаштування → панель WebSocket

settings_ws_title = WebSocket-сервер
settings_ws_subtitle = Налаштуйте підключення оверлеїв та сторонніх інструментів до Forge.
settings_ws_all_saved = Усі зміни збережено
settings_ws_saving = Збереження…
settings_ws_save_failed = Помилка збереження: { $error }
settings_ws_enable_label = Увімкнути сервер
settings_ws_enable_description = Запускається під час старту застосунку, хостить оверлеї, приймає WS-клієнти
settings_ws_bind_section_title = Адреса прив'язки
settings_ws_bind_section_subtitle = Інтерфейс, який слухає сервер
settings_ws_bind_localhost_title = Тільки локальний хост
settings_ws_bind_localhost_description = Підключатися можуть лише застосунки на цьому компʼютері. Browser sources в OBS та локальні плагіни Stream Deck працюють як зазвичай. Безпечний типовий варіант.
settings_ws_bind_lan_title = Усі інтерфейси (LAN)
settings_ws_bind_lan_description = Дозволяє іншим пристроям у вашій мережі (телефон, планшет, другий ПК) підключатися до Forge. Відкриває сервер для будь-кого в тій самій Wi-Fi чи LAN.
settings_ws_bind_lan_restart_warning = Перезапустіть сервер, щоб застосувати зміну адреси прив'язки.
settings_ws_port_section_title = Порт
settings_ws_port_subtitle = За замовчуванням 8081 · діапазон 1024–65535
settings_ws_token_section_title = Bearer-токен
settings_ws_token_clients_send = Клієнти передають його в
settings_ws_auth_section_title = Автентифікація
settings_ws_auth_section_subtitle = Які клієнти мають автентифікуватися
settings_ws_auth_require_ws_label = Вимагати токен для WebSocket-клієнтів
settings_ws_auth_require_ws_sublabel = Відхиляти WS-підключення без дійсного bearer-токена
settings_ws_auth_require_http_label = Вимагати токен для HTTP-файлів оверлею
settings_ws_auth_require_http_sublabel = Браузерні джерела потребують ?token=… в URL
settings_ws_auth_cors_label = Дозволити CORS з будь-якого походження
settings_ws_auth_cors_sublabel = Вимкніть, щоб обмежити лише браузерними джерелами оверлею
settings_ws_overlay_section_title = Коренева тека оверлею
settings_ws_overlay_folder_prefix = Тека доступна за адресою
settings_ws_browse_btn = Огляд
settings_ws_lan_modal_title = Відкрити Forge для вашої мережі?
settings_ws_lan_modal_explanation = Ви перемикаєтеся з 127.0.0.1 (тільки локальний хост) на 0.0.0.0 (всі мережеві інтерфейси). Інші пристрої у вашій локальній мережі — та всі в тій самій Wi-Fi — матимуть доступ до сервера Forge.
settings_ws_lan_modal_confirm_label = Відкрити для LAN
settings_ws_lan_bullet_phone = Телефон / планшет / другий ПК зможуть підключатися до оверлеїв та WS API
settings_ws_lan_bullet_token_warning = Будь-хто у вашій мережі може читати всі події та надсилати повідомлення в чат, якщо знає ваш bearer-токен
settings_ws_lan_bullet_public_wifi = Якщо ви у публічній Wi-Fi (кафе, конференція, готель) — не вмикайте це
settings_ws_lan_bullet_firewall = Ваш брандмауер також повинен дозволяти налаштований порт

## Налаштування → панель хоткеїв

settings_hotkeys_bind_section = ПРИВ'ЯЗАТИ ХОТКЕЙ
settings_hotkeys_registered_section = ЗАРЕЄСТРОВАНІ
settings_hotkeys_backend_section = БЕКЕНД
settings_hotkeys_select_action = Оберіть дію…
settings_hotkeys_bind_btn = Прив'язати
settings_hotkeys_no_bindings = Хоткеїв ще не зареєстровано.
settings_hotkeys_conflict_body_prefix = Комбінація
settings_hotkeys_conflict_body_suffix = вже зареєстрована. Замінити або скасувати?
settings_hotkeys_replace_btn = Замінити
settings_hotkeys_error_no_combo = Спочатку захопіть комбінацію хоткея.
settings_hotkeys_error_no_action = Оберіть дію для прив'язки.
settings_hotkeys_error_unavailable = Система хоткеїв недоступна.
settings_hotkeys_error_load_actions = Не вдалося завантажити дії: { $error }
settings_hotkeys_error_load_bindings = Не вдалося завантажити прив'язки: { $error }
settings_hotkeys_error_unbind = Помилка відв'язки: { $error }
settings_hotkeys_error_replace = Помилка заміни: { $error }
settings_hotkeys_error_conflict_not_found = Конфліктний хоткей не знайдено в локальному кеші. Оновіть і спробуйте знову.

## Налаштування → панель скриптів

settings_scripting_title = Скрипти (Rhai)
settings_scripting_all_saved = Усі зміни збережено
settings_scripting_saving = Збереження…
settings_scripting_unsaved = Незбережені зміни
settings_scripting_save_failed = Помилка збереження: { $error }
settings_scripting_engine_section = Ліміти рушія
settings_scripting_op_limit_label = Ліміт операцій
settings_scripting_op_limit_hint = Діапазон 1 000 – 10 000 000 (за замовчуванням 100 000)
settings_scripting_engine_timeout_label = Тайм-аут (мс)
settings_scripting_engine_timeout_hint = Діапазон 50 – 10 000 (за замовчуванням 500)
settings_scripting_http_section = HTTP-пісочниця
settings_scripting_allowed_domains_label = Дозволені домени
settings_scripting_allowed_domains_hint = Запити до незареєстрованих доменів блокуються. Маски: *.example.com
settings_scripting_domains_placeholder = наприклад: api.example.com
settings_scripting_max_calls_label = Максимум запитів на скрипт
settings_scripting_max_calls_hint = Діапазон 1 – 100 (за замовчуванням 10)
settings_scripting_http_timeout_label = Тайм-аут запиту (мс)
settings_scripting_http_timeout_hint = Діапазон 100 – 30 000 (за замовчуванням 5 000)
settings_scripting_max_response_label = Максимальний розмір відповіді (КіБ)
settings_scripting_max_response_hint = Діапазон 1 – 10 240 (за замовчуванням 1 024 КіБ = 1 МіБ)
settings_scripting_allow_local_label = Дозволити localhost / приватні IP
settings_scripting_allow_local_description = Вимикає захист від SSRF. Вмикайте лише для локальної розробки.
settings_scripting_ssrf_warning = УВАГА — вимикає захист від SSRF. Вмикайте лише для локальної розробки.
