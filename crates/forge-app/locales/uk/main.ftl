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

## Дії — заголовок сторінки / хлібні крихти

actions_breadcrumb_automation = Автоматизація
actions_breadcrumb_actions = Дії
actions_filter_all = Всі
actions_filter_chat = Чат
actions_filter_timers = Таймери
actions_filter_points = Поінти
actions_search_placeholder = Пошук дій...
actions_new_btn = + Нова дія
actions_loading = Завантаження...
actions_empty = Дій ще немає

## Дії — панель деталей

actions_detail_empty_title = Дію не обрано
actions_detail_empty_hint = Оберіть дію зі списку, щоб переглянути деталі.
actions_detail_loading = Завантаження...
actions_detail_enabled = Увімкнено
actions_detail_disabled = Вимкнено
actions_detail_test_run = Тестовий запуск
actions_detail_duplicate = Дублювати
actions_detail_section_triggers = ТРИГЕРИ · { $count }
actions_detail_section_sub_actions = ПІДПУНКТИ · { $count }
actions_detail_add_trigger = Додати тригер
actions_detail_add_sub_action = Додати крок
actions_detail_no_triggers = Тригерів немає — ця дія не спрацює самостійно
actions_detail_no_steps = Кроків ще немає — додайте перший

## Дії — контекстне меню

actions_menu_rename = Перейменувати…
actions_menu_duplicate = Дублювати
actions_menu_enable = Увімкнути
actions_menu_disable = Вимкнути
actions_menu_delete = Видалити…

## Дії — нижній рядок

actions_footer_showing = Показано { $visible } з { $total } · згруповано за тригером
actions_footer_storage = Сховище: —
actions_footer_autosaved = Автозбереження щойно

## Дії — підказка ESC

actions_esc_hint = ESC — скасувати

## Дії — модальне вікно нової дії

actions_modal_new_action_title = Нова дія
actions_modal_section_name = НАЗВА
actions_modal_section_group = ГРУПА
actions_modal_section_queue = ЧЕРГА
actions_modal_section_description = ОПИС
actions_modal_section_behavior = ПОВЕДІНКА
actions_modal_enabled_label = Увімкнено
actions_modal_enabled_desc = Дія виконується, коли спрацьовує тригер.
actions_modal_concurrent_label = Паралельне виконання
actions_modal_concurrent_desc = Дозволити паралельні запуски в цій черзі.
actions_modal_bypass_label = Обходити паузу черги
actions_modal_bypass_desc = Завжди виконувати, навіть якщо черга на паузі.
actions_modal_random_pick_label = Випадковий вибір
actions_modal_random_pick_desc = Виконувати ОДИН випадковий крок замість усіх.
actions_modal_create_btn = Створити дію
actions_modal_cancel_btn = Скасувати

## Дії — модальне вікно кроку

actions_sub_modal_add_title = Додати крок
actions_sub_modal_edit_title = Редагувати крок
actions_sub_chip_send_chat = Чат-повідомлення
actions_sub_chip_set_global = Глобальна змінна
actions_sub_chip_delay = Затримка
actions_sub_chip_log = Журнал
actions_sub_chip_play_sound = Відтворити звук
actions_sub_chip_speak = Синтез мовлення
actions_sub_chip_read_file = Читати файл
actions_sub_chip_random_int = Випадкове число
actions_sub_modal_add_btn = Додати крок
actions_sub_modal_save_btn = Зберегти зміни
actions_sub_modal_cancel_btn = Скасувати

## Дії — секції конфігурації кроку

actions_sub_section_message = ПОВІДОМЛЕННЯ
actions_sub_section_target_platform = ПЛАТФОРМА-ЦІЛЬ
actions_sub_section_variable_name = НАЗВА ЗМІННОЇ
actions_sub_section_value = ЗНАЧЕННЯ
actions_sub_section_milliseconds = МІЛІСЕКУНДИ
actions_sub_section_level = РІВЕНЬ
actions_sub_section_clip = КЛІП
actions_sub_section_text = ТЕКСТ
actions_sub_section_voice_override = ГОЛОС (необов'язково)
actions_sub_section_path = ШЛЯХ (відносно пісочниці assets)
actions_sub_section_target_var = ЦІЛЬОВА ЗМІННА
actions_sub_section_min = МІН
actions_sub_section_max = МАКС
actions_sub_helper_variables = Змінні: %user%, %message%, %args%
actions_sub_helper_interpolation = Підтримує інтерполяцію змінних
actions_sub_voice_hint = Залишіть порожнім, щоб використати резолвер псевдонімів
actions_sub_path_hint = Пісочниця data_dir/assets/ · без ../ · максимум 1 МіБ
actions_sub_no_clips = Кліпів немає — спочатку додайте їх на екрані Звукова панель.

## Дії — вибір тригера (бічна панель)

actions_picker_title = Додати тригер
actions_picker_loading = Завантаження тригерів…
actions_picker_cancel = Скасувати
actions_picker_select_platform = Оберіть платформу
actions_picker_no_triggers = Тригерів немає
actions_picker_select_hint = Оберіть платформу, щоб переглянути тригери.
actions_picker_no_triggers_selection = Для цього вибору тригерів немає.
actions_picker_default_label = (типовий)

## Дії — назви категорій тригерів (заголовки секцій)

actions_cat_chat_commands = ЧАТ-КОМАНДИ
actions_cat_subs_bits = ПІДПИСКИ ТА БІТСИ
actions_cat_bits = БІТСИ
actions_cat_raids = РЕЙДИ
actions_cat_obs_events = ПОДІЇ OBS
actions_cat_server_events = ПОДІЇ СЕРВЕРА
actions_cat_timers = ТАЙМЕРИ
actions_cat_ungrouped = БЕЗ ГРУПИ
actions_cat_all = ВСІ

## Дії — підписи типів тригерів

actions_kind_twitch_chat_command = Twitch · команда в чаті
actions_kind_twitch_chat_message = Twitch · будь-яке повідомлення
actions_kind_twitch_subscriber = Twitch · новий підписник
actions_kind_twitch_resubscriber = Twitch · повторна підписка
actions_kind_twitch_gift_sub = Twitch · подарункові підписки
actions_kind_twitch_cheer = Twitch · відправлено бітсів
actions_kind_twitch_raid = Twitch · отримано рейд
actions_kind_obs_scene_changed = OBS · зміна сцени
actions_kind_server_custom_event = Сервер · користувацька подія
actions_kind_unknown = Невідомий тригер

## Дії — опис типів тригерів

actions_summary_twitch_chat_command = Користувач вводить !команду в чаті
actions_summary_twitch_chat_message = Кожне повідомлення в чаті спрацьовує
actions_summary_twitch_subscriber = Спрацьовує, коли хтось підписується
actions_summary_twitch_resubscriber = Наявний підписник продовжує підписку
actions_summary_twitch_gift_sub = Хтось дарує підписки каналу
actions_summary_twitch_cheer = Глядач відправляє бітси
actions_summary_twitch_raid = Інший стрім робить рейд на вас
actions_summary_obs_scene_changed = Спрацьовує, коли OBS перемикає активну сцену
actions_summary_server_custom_event = Спрацьовує при виклику triggerCodeEvent через WebSocket API

## Редактор дій — хлібні крихти / дерево / деталі

action_editor_breadcrumb_automation = Автоматизація
action_editor_breadcrumb_actions = Дії
action_editor_loading = Завантаження дії…
action_editor_no_description = Без опису
action_editor_test_run = Тестовий запуск
action_editor_duplicate = Дублювати
action_editor_add_trigger = Додати тригер
action_editor_add_step = Додати крок
action_editor_no_triggers = Тригерів немає · натисніть «Додати тригер», щоб почати
action_editor_delete = Видалити
action_editor_section_triggers = ТРИГЕРИ
action_editor_section_sub_actions = ПІДПУНКТИ · { $count }
action_editor_sub_count = { $count } кр.
action_editor_enabled = Увімкнено
action_editor_disabled = Вимкнено

## Редактор дій — меню кроку

action_editor_step_menu_edit = Редагувати крок…
action_editor_step_menu_duplicate = Дублювати
action_editor_step_menu_move_top = Перемістити вгору
action_editor_step_menu_move_bottom = Перемістити вниз
action_editor_step_menu_delete = Видалити крок

## Редактор дій — назви типів кроків

action_editor_kind_send_chat = Надіслати в чат
action_editor_kind_set_global = Встановити змінну
action_editor_kind_delay = Затримка
action_editor_kind_log = Журнал
action_editor_kind_play_sound = Відтворити звук
action_editor_kind_speak = Синтез мовлення
action_editor_kind_read_file = Читати файл
action_editor_kind_random_int = Випадкове число
action_editor_kind_sub_action = Підпункт

## Реєстр тригерів — заголовок / фільтри

triggers_breadcrumb_automation = Автоматизація
triggers_breadcrumb_triggers = Тригери
triggers_open_create_btn = + Створити
triggers_search_placeholder = Пошук тригерів…
triggers_filter_twitch = Twitch
triggers_filter_obs = OBS
triggers_filter_script = Скрипт
triggers_filter_all = Всі
triggers_usage_all = Всі
triggers_usage_used = Використовуються
triggers_usage_unused = Не використовуються

## Реєстр тригерів — список / порожні стани

triggers_empty_title = Власних тригерів ще немає
triggers_empty_hint = Створіть іменований тригер із власними налаштуваннями для повторного використання в кількох діях.
triggers_empty_create = + Створити тригер
triggers_no_results_title = Нічого не знайдено
triggers_no_results_hint = Змініть або скиньте фільтри, щоб знайти тригери.
triggers_clear_filters = Скинути фільтри
triggers_usage_badge = використовується в { $count }
triggers_toggle_on = УВІМК
triggers_toggle_off = ВИМК

## Реєстр тригерів — деталі у бічній панелі

triggers_sheet_section_configuration = КОНФІГУРАЦІЯ
triggers_sheet_no_config = Налаштованих полів немає
triggers_sheet_not_registered = Тип тригера не зареєстровано
triggers_sheet_section_used_in = ВИКОРИСТОВУЄТЬСЯ В
triggers_sheet_section_platform = ПЛАТФОРМА
triggers_sheet_delete_btn = Видалити
triggers_sheet_any_platform = Будь-яка платформа
triggers_sheet_will_fire_on = Спрацює на: { $platform }
triggers_sheet_will_fire_on_scope = Спрацює на: { $scope }

## Реєстр тригерів — діалог підтвердження вимкнення

triggers_confirm_disable_body = Вимкнення цього тригера призупинить його для { $count } дій. Продовжити?
triggers_confirm_disable_dismiss = Скасувати
triggers_confirm_disable_accept = Все одно вимкнути

## Форма створення тригера — вибір типу

triggers_create_select_kind = Оберіть тип тригера
triggers_create_search_placeholder = Пошук типів…
triggers_create_no_results = Відповідних типів тригерів немає
triggers_create_cancel = Скасувати

## Форма створення тригера — заповнення

triggers_create_back = Назад
triggers_create_new_instance = Новий екземпляр: { $kind }
triggers_create_section_name = НАЗВА
triggers_create_name_placeholder = Назва екземпляра (обов'язково)
triggers_create_section_config = КОНФІГУРАЦІЯ
triggers_create_section_platform = ПЛАТФОРМА
triggers_create_scope_any = Будь-яка
triggers_create_scope_custom = Власний вибір…
triggers_create_will_fire = Спрацює на: { $scope }
triggers_create_btn = Створити

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

## Actions trigger picker — category labels

trigger_cat_chat = Чат
trigger_cat_subscriptions = Підписки
trigger_cat_bits = Бітси
trigger_cat_raids = Рейди
trigger_cat_channel_points = Бали каналу
trigger_cat_polls = Опитування
trigger_cat_predictions = Прогнози
trigger_cat_hype = Hype Train
trigger_cat_charity = Благодійність
trigger_cat_goals = Цілі
trigger_cat_clips = Кліпи
trigger_cat_streams = Трансляції
trigger_cat_users = Користувачі
trigger_cat_obs = Сцени
trigger_cat_hotkey = Гарячі клавіші
trigger_cat_core = Ядро
trigger_cat_server = Події сервера
trigger_cat_timer = Таймери
trigger_cat_other = Інше

## Actions modals — placeholder literals

actions_name_placeholder = Моя автоматизація
actions_group_placeholder = Приклади
actions_description_placeholder = Відтворює звук, показує сповіщення…
actions_log_message_placeholder = Дію розпочато
actions_speak_text_placeholder = Текст для озвучення…

## Triggers registry — error messages

triggers_delete_reference_block = Вилучіть цей тригер з усіх дій перед видаленням.
