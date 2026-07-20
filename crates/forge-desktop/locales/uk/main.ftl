## Boot - splash and data-open failure screens

boot_starting = Запуск…
boot_upgrade_title = Потрібне оновлення
boot_upgrade_body = Ваші дані forge використовують схему версії { $found }, новішу за версію цієї збірки { $expected }. Оновіть forge до останнього випуску, щоб відкрити їх.
boot_upgrade_reassure = Ваші дані у безпеці й недоторкані.
boot_retry = Повторити
boot_failure_title = Не вдалося відкрити ваші дані
boot_failure_reassure = Ваші дані у безпеці. Якщо це повторюється, повідомте про проблему.

## Загальні дії, спільні для всіх екранів

common_cancel = Скасувати
common_save = Зберегти
common_language = Мова

## Навігація - назви екранів (хлібні крихти + бічна панель)

nav_home = Головна
nav_actions = Дії
nav_queues = Черги
nav_triggers = Тригери
nav_integration = Інтеграція
nav_live_chat = Живий чат
nav_event_feed = Стрічка подій
nav_globals = Глобальні змінні
nav_settings = Налаштування
nav_tts = TTS
nav_soundboard = Звукова панель
nav_script_editor = Скрипти
nav_api_reference = Довідник API
nav_server = Сервер

## Навігація - заголовки секцій бічної панелі

nav_section_audience = АУДИТОРІЯ
nav_section_automation = АВТОМАТИЗАЦІЯ
nav_section_builtin = Вбудовані

## Навігація - підписи пунктів бічної панелі

nav_item_home = Головна
nav_item_chat = Чат
nav_item_actions = Дії
nav_item_triggers = Тригери
nav_item_queues = Черги
nav_item_event_feed = Стрічка подій
nav_item_globals = Глобальні змінні
nav_item_platforms = Платформи
nav_item_stream_apps = Стрім-застосунки
nav_group_modules = Вбудовані
nav_item_soundboard = Звукова панель
nav_item_tts = Синтез мовлення
nav_item_ws_server = WebSocket-сервер
nav_item_discord = Discord
nav_item_midi = MIDI
nav_item_hotkey = Гарячі клавіші
nav_item_settings = Налаштування

## Головна - секція привітання

home_hero_tagline = Відкрита автоматизація стрімів, створена для стрімерів
home_hero_import = Імпортувати
home_hero_new_action = Нова дія
home_import_success = Імпортовано дію «{ $name }»
home_import_failed = Не вдалося імпортувати: { $error }
home_stats_error = Не вдалося завантажити статистику: { $error }
home_stats_retry = Повторити

## Головна - картки швидкого переходу

home_card_audience_section = АУДИТОРІЯ
home_card_audience_title = Чат
home_card_audience_stat_label = глядачів зараз
home_card_audience_hint = Спілкуйтеся з аудиторією та бачте, хто дивиться
home_card_automation_section = АВТОМАТИЗАЦІЯ
home_card_automation_title = Дії
home_card_automation_hint = Налаштовуйте тригери, команди та таймери
home_card_connections_section = ПІДКЛЮЧЕННЯ
home_card_connections_title = Підключення
home_card_connections_stat_label = підключено
home_card_connections_hint = Керуйте платформами, застосунками та модулями

## Головна - картка здоров'я стріму

home_health_title = Стан стріму
home_health_live = ЖИВИЙ
home_health_offline = офлайн
home_health_refresh_hint = остання хвилина · автооновлення
home_health_throughput_label = ПРОПУСКНА ЗДАТНІСТЬ · под/с
home_health_bitrate_label = БІТРЕЙТ · OBS
home_health_dropped_label = ВТРАЧЕНІ · OBS
home_health_fps_label = FPS
home_health_cpu_label = CPU

## Головна - смуга підключень

home_connections_title = Інтеграції

## Головна - статуси підключень

home_conn_connected = підключено
home_conn_offline = офлайн

## Головна - картка останніх подій

home_events_title = Останні події
home_events_empty = Подій ще немає

## Головна - картка швидкого огляду

home_glance_title = Огляд
home_glance_actions = Дії
home_glance_commands = Команди
home_glance_fired = Спрацювань у сесії
home_glance_globals = Глобальні змінні

## Головна - підпис поряд із великим лічильником

home_card_automation_stat_label = дій · { $fired } спрацювань сьогодні

## Головна - зведення активних / відключених

home_connections_summary = { $active } активних · { $disconnected } відключено

## Налаштування → панель зовнішнього вигляду

settings_appearance_title = Зовнішній вигляд
settings_appearance_theme_label = Тема
settings_theme_active = АКТИВНА
settings_theme_mocha_desc = Темна, тепла
settings_theme_tokyo_desc = Темна, холодна
settings_theme_latte_desc = Світла
settings_appearance_density_label = Щільність інтерфейсу
settings_appearance_density_subtitle = Скільки простору отримує інтерфейс - застосовується миттєво
settings_appearance_density_compact = Компактна
settings_appearance_density_compact_hint = Щільніше розташування, більше рядків на екрані
settings_appearance_density_cozy = Затишна
settings_appearance_density_cozy_hint = Збалансовані відступи (типово)
settings_appearance_density_spacious = Простора
settings_appearance_density_spacious_hint = Більше повітря між елементами
settings_appearance_fonts_label = Шрифти
settings_appearance_fonts_subtitle = Шрифти інтерфейсу та коду - застосовується миттєво
settings_appearance_fonts_scanning = Сканування встановлених шрифтів…
settings_appearance_font_body_label = Шрифт інтерфейсу
settings_appearance_font_mono_label = Моноширинний шрифт
settings_appearance_font_default_placeholder = { $family } (типовий)
settings_appearance_font_reset = Скинути до типового
settings_appearance_font_missing = «{ $family }» не встановлено - доки він не з'явиться, діє типовий
settings_appearance_font_show_all = Показати всі шрифти
settings_appearance_font_preview = Жебракують філософи при ґанку церкви в Гадячі · 0123456789
settings_appearance_theme_hint = Який вигляд матиме Forge
settings_appearance_font_interface = ІНТЕРФЕЙС
settings_appearance_font_monospace = МОНОШИРИННИЙ
settings_theme_default = Стандартна
settings_theme_desc_dark = темна
settings_theme_desc_storm = Шторм
settings_theme_desc_light_mode = Світлий режим

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
settings_nav_language_region = Мова та регіон
settings_nav_shortcuts = Скорочення
settings_nav_notifications = Сповіщення
settings_nav_audio = Аудіо
settings_nav_scripting = Скрипти
settings_nav_queues = Черги
settings_nav_storage = Сховище
settings_nav_websocket = WebSocket
settings_nav_hotkeys = Хоткеї
settings_nav_version = Версія
settings_nav_diagnostics = Діагностика

## Налаштування → панель діагностики

settings_about_build_label = Збірка
settings_about_rust_label = Rust
settings_about_os_label = ОС
settings_diagnostics_log_dir_hint = Журнали середовища виконання пишуться в цю теку.
settings_diagnostics_section_title = Журнали та діагностика
settings_diagnostics_log_dir = Тека журналів: { $path }
settings_diagnostics_log_dir_label = Тека журналів
settings_diagnostics_open_log_dir = Відкрити теку журналів
settings_diagnostics_log_level_hint = Рівень журналювання: керується через змінну RUST_LOG (наприклад: info, debug, trace).

## Налаштування → панель версії

settings_version_title = Версія та оновлення
settings_version_license = Відкритий код · MIT OR Apache-2.0
settings_version_check_updates = Перевірити оновлення
settings_version_recent_releases = ОСТАННІ РЕЛІЗИ
settings_version_changelog_empty = Історія релізів поки відсутня.

## Налаштування → панель сховища

settings_storage_section_title = Сховище та резервні копії
settings_storage_db_path = База даних: { $path }
settings_storage_db_path_label = База даних
settings_storage_backup_btn = Резервна копія зараз
settings_storage_backup_hint = Створює копію бази з міткою часу в теці даних.
settings_storage_keep_limit_label = Ліміт зберігання історії чату
settings_storage_keep_limit_hint = Скільки повідомлень чату зберігати в базі даних.
settings_storage_display_limit_label = Показувати при відкритті чату
settings_storage_display_limit_hint = Скільки останніх повідомлень завантажувати під час відкриття чату.

## Налаштування → панель черг

settings_queues_section_title = Черги та потоки
settings_queues_thread_hint = Пул потоків Tokio: { $workers } потік(ів) (автоматично за системою).
settings_queues_workers_label = Робочі потоки
settings_queues_managed_hint = Ліміти паралелізму та прапорці блокування керуються на екрані Черги.

## Налаштування → панель сповіщень

settings_notifications_section_title = Сповіщення
settings_notifications_hint = Налаштування спливаючих підказок за типом події з'явиться пізніше. Помилки та зміни підключень завжди відображаються в рядку стану.

## Налаштування → панель гарячих клавіш (ярлики)

settings_shortcuts_title = Клавіатурні скорочення
settings_shortcuts_subtitle = Ці скорочення працюють лише тоді, коли вікно forge у фокусі. Системні комбінації - у розділі «Хоткеї».
settings_shortcuts_action_nav_home = Перейти на Хаб
settings_shortcuts_action_nav_live_chat = Відкрити живий чат
settings_shortcuts_action_nav_event_feed = Відкрити стрічку подій
settings_shortcuts_action_nav_actions = Відкрити дії
settings_shortcuts_action_nav_triggers = Відкрити тригери
settings_shortcuts_action_nav_twitch = Відкрити Twitch
settings_shortcuts_action_nav_globals = Відкрити глобальні змінні
settings_shortcuts_action_nav_script_editor = Відкрити скрипти
settings_shortcuts_action_nav_settings = Відкрити налаштування
settings_shortcuts_unbound = Не призначено
settings_shortcuts_capture_prompt = Натисніть скорочення... Esc для скасування
settings_shortcuts_rebind = Змінити
settings_shortcuts_reset = Скинути
settings_shortcuts_reset_all = Скинути все до типових
settings_shortcuts_fixed_section = НЕЗМІННІ КЛАВІШІ
settings_shortcuts_fixed_enter = Підтвердити форму чи діалог
settings_shortcuts_fixed_escape = Закрити модальне вікно чи скасувати захоплення
settings_shortcuts_fixed_note = Ці клавіші вбудовані, їх не можна перепризначити.
settings_shortcuts_error_needs_modifier = Поєднайте клавішу з Ctrl, Alt чи Meta або оберіть F-клавішу - звичайні клавіші заважали б набору тексту.
settings_shortcuts_error_global_hotkey = { $chord } вже зайнято глобальним хоткеєм. Спершу звільніть його в Налаштування → Хоткеї.
settings_shortcuts_conflict_body = { $chord } зараз призначено дії «{ $owner }». Перепризначити? Попереднє скорочення стане непризначеним.
settings_shortcuts_conflict_steal = Перепризначити

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
settings_ws_badge_recommended = Рекомендовано
settings_ws_badge_requires_confirmation = Потребує підтвердження
settings_ws_port_section_title = Порт
settings_ws_port_subtitle = За замовчуванням 8081 · діапазон 1024-65535
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
settings_ws_lan_modal_explanation = Ви перемикаєтеся з 127.0.0.1 (тільки локальний хост) на 0.0.0.0 (всі мережеві інтерфейси). Інші пристрої у вашій локальній мережі - та всі в тій самій Wi-Fi - матимуть доступ до сервера Forge.
settings_ws_lan_modal_confirm_label = Відкрити для LAN
settings_ws_lan_bullet_phone = Телефон / планшет / другий ПК зможуть підключатися до оверлеїв та WS API
settings_ws_lan_bullet_token_warning = Будь-хто у вашій мережі може читати всі події та надсилати повідомлення в чат, якщо знає ваш bearer-токен
settings_ws_lan_bullet_public_wifi = Якщо ви у публічній Wi-Fi (кафе, конференція, готель) - не вмикайте це
settings_ws_lan_bullet_firewall = Ваш брандмауер також повинен дозволяти налаштований порт

## Налаштування → панель хоткеїв

settings_hotkeys_scope_subtitle = Ці комбінації реєструються в операційній системі та спрацьовують, навіть коли forge працює у фоні.
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
settings_hotkeys_capture_prompt = Натисніть клавіші... Esc для скасування

## Дії - заголовок сторінки / хлібні крихти

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

## Дії - панель деталей

actions_detail_empty_title = Дію не обрано
actions_detail_empty_hint = Оберіть дію зі списку, щоб переглянути деталі.

## Дії - контекстне меню

actions_menu_rename = Перейменувати…
actions_menu_duplicate = Дублювати
actions_menu_enable = Увімкнути
actions_menu_disable = Вимкнути
actions_menu_delete = Видалити…

## Дії - нижній рядок


## Дії - підказка ESC

actions_esc_hint = ESC - скасувати

## Дії - модальне вікно нової дії

actions_modal_new_action_title = Нова дія
actions_modal_edit_action_title = Редагувати дію
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
actions_modal_save_btn = Зберегти зміни
actions_modal_cancel_btn = Скасувати

## Дії - модальне вікно кроку

actions_sub_select_kind = Оберіть тип кроку
actions_sub_no_config = Цей крок не має налаштувань.
actions_sub_select_placeholder = Оберіть...
actions_sub_select_empty = Немає доступних варіантів
sub_cat_chat = Чат
sub_cat_moderation = Модерація
sub_cat_channel_points = Бали каналу
sub_cat_polls_predictions = Опитування та прогнози
sub_cat_globals = Глобальні змінні
sub_cat_logic = Логіка
sub_cat_delay = Затримка
sub_cat_scripts = Скрипти
sub_cat_files = Файли
sub_cat_hotkey = Гарячі клавіші
sub_cat_audio = Аудіо
sub_cat_tts = Синтез мовлення
sub_cat_http = HTTP
sub_cat_server = Сервер
sub_cat_util = Утиліти
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

## Дії - секції конфігурації кроку

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
actions_sub_no_clips = Кліпів немає - спочатку додайте їх на екрані Звукова панель.

## Дії - вибір тригера (бічна панель)

actions_picker_title = Додати тригер
actions_picker_loading = Завантаження тригерів…
actions_picker_cancel = Скасувати
actions_picker_select_platform = Оберіть платформу
actions_picker_no_triggers = Тригерів немає
actions_picker_select_hint = Оберіть платформу, щоб переглянути тригери.
actions_picker_no_triggers_selection = Для цього вибору тригерів немає.
actions_picker_default_label = (типовий)

## Дії - назви категорій тригерів (заголовки секцій)

actions_cat_chat_commands = ЧАТ-КОМАНДИ
actions_cat_subs_bits = ПІДПИСКИ ТА БІТСИ
actions_cat_bits = БІТСИ
actions_cat_raids = РЕЙДИ
actions_cat_obs_events = ПОДІЇ OBS
actions_cat_server_events = ПОДІЇ СЕРВЕРА
actions_cat_timers = ТАЙМЕРИ
actions_cat_ungrouped = БЕЗ ГРУПИ
actions_cat_all = ВСІ

## Дії - підписи типів тригерів

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

## Дії - опис типів тригерів

actions_summary_twitch_chat_command = Користувач вводить !команду в чаті
actions_summary_twitch_chat_message = Кожне повідомлення в чаті спрацьовує
actions_summary_twitch_subscriber = Спрацьовує, коли хтось підписується
actions_summary_twitch_resubscriber = Наявний підписник продовжує підписку
actions_summary_twitch_gift_sub = Хтось дарує підписки каналу
actions_summary_twitch_cheer = Глядач відправляє бітси
actions_summary_twitch_raid = Інший стрім робить рейд на вас
actions_summary_obs_scene_changed = Спрацьовує, коли OBS перемикає активну сцену
actions_summary_server_custom_event = Спрацьовує при виклику triggerCodeEvent через WebSocket API

## Редактор дій - хлібні крихти / дерево / деталі

action_editor_loading = Завантаження дії…
action_editor_no_description = Без опису
action_editor_test_run = Тестовий запуск
action_editor_duplicate = Дублювати
action_editor_export = Експорт JSON
action_editor_export_done = Дію експортовано до { $path }
action_editor_export_failed = Не вдалося експортувати: { $error }
action_editor_edit = Редагувати
action_editor_menu_delete = Видалити дію
action_editor_edit_modal_title = Редагування дії
action_editor_edit_save_btn = Зберегти
action_editor_add_trigger = Додати тригер
action_editor_add_step = Додати крок
action_editor_health_unknown_var = Використовує %{ $name }%, якої не надає жоден тригер і не створює жоден попередній крок
action_editor_health_produced_later = Використовує %{ $name }%, але її створює лише пізніший крок
action_editor_health_isolated_sibling = Використовує %{ $name }%, яку створює сусідній крок, що виконується ізольовано й не ділиться нею
action_editor_health_some_triggers = Використовує %{ $name }%, яку надають лише деякі тригери цієї дії
action_editor_health_last_run_failed = Останній запуск завершився помилкою: { $message }
action_editor_health_ok = Все гаразд: усі посилання резолвляться, останній запуск успішний
action_editor_health_warn = Статичне попередження
action_editor_health_error = Помилка
action_editor_branch_modal_hint = Редагуйте цю гілку у списку кроків нижче
action_editor_branch_empty = Кроків ще немає · натисніть «Додати крок», щоб почати
action_editor_no_steps = Ця дія ще не має кроків
action_editor_breadcrumb_steps = Кроки
action_editor_branch_then = Якщо так
action_editor_branch_else = Інакше
action_editor_branch_body = Тіло
action_editor_branch_default = За замовчуванням
action_editor_branch_fallback = Гілка
action_editor_branch_case = Випадок
action_editor_branch_chain = Ланцюг
action_editor_add_case = Додати випадок
action_editor_case_multi = багатозначне зіставлення (лише читання)
action_editor_case_match_placeholder = значення зіставлення
action_editor_branch_cap = Досягнуто межі вкладеності · глибше вкладати не можна
action_editor_no_triggers = Тригерів немає · натисніть «Додати тригер», щоб почати
action_editor_delete = Видалити
action_editor_delete_cascade_hint = Також буде видалено { $sub_actions } підпунктів і { $trigger_links } прив'язок тригерів.
action_editor_section_triggers = ТРИГЕРИ
action_editor_section_triggers_count = ТРИГЕРИ · { $count }
action_editor_triggers_hint = Клацніть тригер, щоб редагувати його в реєстрі
action_editor_section_sub_actions = ПІДПУНКТИ · { $count }
action_editor_section_sub_actions_label = ПІДПУНКТИ
action_editor_sub_actions_count = { $count } підпунктів
actions_sub_file_browse = Огляд
actions_sub_datetime_pick = Обрати
actions_sub_datetime_now = Зараз
actions_sub_datetime_set = Встановити
action_editor_sub_count = { $count } кр.
action_editor_enabled = Увімкнено
action_editor_disabled = Вимкнено

## Редактор дій - статистика виконання
action_stat_last_fired = ОСТАННІЙ ЗАПУСК
action_stat_runs_today = ЗАПУСКІВ · СЬОГОДНІ
action_stat_avg_time = СЕР. ЧАС
action_stat_errors_7d = ПОМИЛОК · 7д
action_stat_avg_ms = { $count } мс
action_stat_avg_none = -
action_stat_execution = ВИКОНАННЯ

## Редактор дій - історія запусків

action_editor_run_history = Історія запусків…
action_editor_run_history_title = Історія запусків
action_editor_run_history_loading = Завантаження історії запусків…
action_editor_run_history_empty_title = Запусків ще немає
action_editor_run_history_empty_hint = Ця дія ще не виконувалася
action_editor_run_history_duration_ms = { $count } мс
action_editor_run_history_outcome_success = Успіх
action_editor_run_history_outcome_failed = Помилка
action_editor_run_history_outcome_cancelled = Скасовано
action_editor_run_history_step_ok = ok
action_editor_run_history_step_failed = помилка
action_editor_run_history_step_skipped = пропущено
action_editor_run_history_step_nested = ↳
action_editor_run_history_trigger_fallback = Тригер
action_editor_run_history_step_args_in = @in
action_editor_run_history_step_produced = @out

## Редактор дій - меню кроку

action_editor_step_menu_edit = Редагувати крок…
action_editor_step_menu_duplicate = Дублювати
action_editor_step_menu_move_top = Перемістити вгору
action_editor_step_menu_move_bottom = Перемістити вниз
action_editor_step_menu_delete = Видалити крок
actions_step_disable = Вимкнути
actions_step_enable = Увімкнути
actions_step_continue_on_error = Продовжувати при помилці
actions_step_continue_on_error_hint = Виконувати наступні кроки, навіть якщо цей впаде
actions_step_subtitle = Крок {$index} з {$total} · налаштування
actions_step_advanced = ДОДАТКОВО
actions_step_condition_label = ВИКОНУВАТИ, ЛИШЕ ЯКЩО (умова)
actions_step_condition_hint = Залиште порожнім, щоб крок виконувався завжди

## Редактор дій - тестовий запуск

action_editor_test_failed = Не вдалося запустити тестовий тригер: { $error }
action_editor_test_run_title = Тестовий запуск · { $name }
action_editor_test_run_subtitle_trigger = Змодельований тригер · { $name }
action_editor_test_run_subtitle_none = Тригер не прив'язано
action_editor_test_run_trigger_pick = Змоделювати як тригер
action_editor_test_run_note_no_schema = Тригер не оголошує вихідних даних · запуск із порожніми значеннями
action_editor_test_run_note_no_triggers = Тригер не прив'язано · запуск із порожніми значеннями
action_editor_test_run_empty = Немає підпунктів для виконання.
action_editor_test_run_default_error = Виконання кроку не вдалося
action_editor_test_run_status_queued = у черзі
action_editor_test_run_status_running = виконується…
action_editor_test_run_status_failed = помилка
action_editor_test_run_status_skipped = пропущено
action_editor_test_run_status_ms = { $ms } мс
action_editor_test_run_failed_banner = Збій на кроці { $step } · { $name }
action_editor_test_run_completed = Виконано кроків: { $count } · помилок: { $errors }
action_editor_test_run_notstarted = Дію не запущено · можливо, чергу призупинено
action_editor_test_run_foot_simulating = Моделювання…
action_editor_test_run_foot_finished = Запуск завершено
action_editor_test_run_foot_halted = Зупинено через помилку
action_editor_test_run_foot_notstarted = Не запущено
action_editor_test_run_again = Запустити знову
action_editor_test_run_close = Закрити

## Редактор дій - вибір підпункту / тригера

action_editor_this_action = цю дію
action_editor_saved_triggers = Ваші збережені тригери
action_editor_recent_triggers = Нещодавні
picker_favorites = Обране
picker_favorites_empty = Позначте зірочкою, щоб закріпити тут для швидкого доступу
action_editor_picker_add_sub_title = Додати підпункт
action_editor_picker_inserting_into = Вставлення у
action_editor_picker_sub_count = · { $count } підпунктів
action_editor_picker_footer_hint = Додається з розумними типовими значеннями - редагуйте на місці
action_editor_picker_search = Пошук серед { $count } підпунктів…
action_editor_picker_fires = Запускає
action_editor_picker_available_count = · { $count } доступно
action_editor_trigger_picker_footer_hint = Створює новий тригер обраного типу та прив'язує його
action_editor_no_unlinked_triggers = Немає доступних неприв'язаних тригерів - створіть один на екрані тригерів

## Редактор дій - назви типів кроків

action_editor_kind_send_chat = Надіслати в чат
action_editor_kind_set_global = Встановити змінну
action_editor_kind_delay = Затримка
action_editor_kind_log = Журнал
action_editor_kind_play_sound = Відтворити звук
action_editor_kind_speak = Синтез мовлення
action_editor_kind_read_file = Читати файл
action_editor_kind_random_int = Випадкове число
action_editor_kind_incr_global = Збільшити змінну
action_editor_kind_run_script = Запустити скрипт
action_editor_persisted_note = (збережено)
action_editor_kind_sub_action = Підпункт

## Реєстр тригерів - заголовок / фільтри

triggers_breadcrumb_automation = Автоматизація
triggers_breadcrumb_triggers = Тригери
triggers_open_create_btn = + Створити
triggers_search_placeholder = Пошук тригерів…
triggers_filter_twitch = Twitch
triggers_filter_youtube = YouTube
triggers_filter_kick = Kick
triggers_filter_obs = OBS
triggers_filter_vtube = VTube Studio
triggers_filter_midi = MIDI
triggers_filter_hotkey = Гарячі клавіші
triggers_filter_discord = Discord
triggers_filter_script = Скрипт
triggers_filter_all = Всі
triggers_usage_all = Всі
triggers_usage_used = Використовуються
triggers_usage_unused = Не використовуються
triggers_toast_error = Тригери: { $message }
triggers_stat_instances = екземплярів
triggers_stat_used = використовується
triggers_stat_disabled = вимкнено
triggers_platform_clear = скинути
triggers_platform_timer = Таймер
triggers_platform_script = Скрипт
triggers_platform_core = Ядро
triggers_new_trigger = Новий тригер

## Реєстр тригерів - список / порожні стани

triggers_empty_title = Власних тригерів ще немає
triggers_empty_hint = Створіть іменований тригер із власними налаштуваннями для повторного використання в кількох діях.
triggers_empty_create = + Створити тригер
triggers_no_results_title = Нічого не знайдено
triggers_no_results_hint = Змініть або скиньте фільтри, щоб знайти тригери.
triggers_clear_filters = Скинути фільтри
triggers_usage_badge = використовується в { $count }
triggers_toggle_on = УВІМК
triggers_toggle_off = ВИМК
triggers_col_name = НАЗВА
triggers_col_kind = ТИП
triggers_col_used = ВИКОРИСТАННЯ
triggers_col_on = УВІМК
triggers_override_badge =
    { $count ->
        [one] { $count } перевизначення
        [few] { $count } перевизначення
        [many] { $count } перевизначень
       *[other] { $count } перевизначень
    }
triggers_used_in_prefix = використовується в
triggers_row_unused = не використовується
triggers_empty_create_first = Створити перший тригер

## Реєстр тригерів - меню рядка

triggers_menu_rename = Перейменувати…
triggers_menu_template = Використати як шаблон
triggers_menu_delete = Видалити…
triggers_template_copy_name = { $name } копія

## Реєстр тригерів - деталі у бічній панелі

triggers_sheet_section_configuration = КОНФІГУРАЦІЯ
triggers_sheet_config_overridden = { $count } перевизначено
triggers_sheet_config_all_defaults = усе за замовчуванням
triggers_sheet_config_save = Зберегти
triggers_sheet_config_cancel = Скасувати
triggers_sheet_no_config = Налаштованих полів немає
triggers_sheet_section_cooldown = ПЕРЕЗАРЯДКА
triggers_sheet_cooldown_caption = секунди · 0 = вимк
triggers_sheet_cooldown_value = перезарядка
triggers_sheet_cooldown_scope = Глобальна перезарядка
triggers_cooldown_suffix_global = { " · перезарядка=" }{ $secs }{ "с глобально" }
triggers_cooldown_suffix_per_user = { " · перезарядка=" }{ $secs }{ "с на глядача" }
triggers_sheet_not_registered = Тип тригера не зареєстровано
triggers_sheet_section_used_in = ВИКОРИСТОВУЄТЬСЯ В
triggers_sheet_section_platform = ПЛАТФОРМА
triggers_sheet_delete_btn = Видалити
triggers_sheet_save_btn = Зберегти
triggers_sheet_any_platform = Будь-яка платформа
triggers_sheet_will_fire_on = Спрацює на: { $platform }
triggers_sheet_will_fire_on_scope = Спрацює на: { $scope }
triggers_detail_loading = Завантаження тригера…
triggers_sheet_config_authored = Задається на кроці
triggers_sheet_section_used_in_count = ВИКОРИСТОВУЄТЬСЯ В ({ $count })
triggers_sheet_used_in_empty_title = Ще не пов'язано з жодною дією.
triggers_sheet_used_in_empty_hint = Відкрийте дію та додайте цей тригер з вибору.

## Реєстр тригерів - діалог підтвердження вимкнення

triggers_confirm_disable_title = Вимкнути цей тригер?
triggers_confirm_disable_body = Вимкнення цього тригера призупинить його для { $count } дій. Продовжити?
triggers_confirm_disable_dismiss = Скасувати
triggers_confirm_disable_accept = Все одно вимкнути

## Реєстр тригерів - діалог підтвердження видалення

triggers_confirm_delete_title = Видалити тригер?
triggers_confirm_delete_body = Це назавжди видалить екземпляр тригера.

## Реєстр тригерів - діалог перейменування

triggers_rename_title = Перейменувати тригер
triggers_rename_kbd_hint = ENTER щоб зберегти · ESC щоб скасувати

## Реєстр тригерів - сповіщення про скасування видалення

triggers_toast_deleted = Видалено '{ $name }'

## Форма створення тригера - вибір типу

triggers_create_select_kind = Оберіть тип тригера
triggers_create_search_placeholder = Пошук типів…
triggers_create_no_results = Відповідних типів тригерів немає
triggers_create_cancel = Скасувати
triggers_create_type_count = типів тригерів: { $count }
triggers_create_search_types = Пошук серед { $count } типів тригерів…
triggers_create_footer_hint = Оберіть джерело подій - налаштуйте далі
triggers_create_cat_server = Сервер
triggers_create_cat_timer = Таймер

## Форма створення тригера - заповнення

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
triggers_create_kbd_hint = ENTER щоб створити · ESC щоб скасувати

## Налаштування → панель скриптів

settings_scripting_title = Скрипти (Rhai)
settings_scripting_all_saved = Усі зміни збережено
settings_scripting_saving = Збереження…
settings_scripting_unsaved = Незбережені зміни
settings_scripting_save_failed = Помилка збереження: { $error }
settings_scripting_engine_section = Ліміти рушія
settings_scripting_op_limit_label = Ліміт операцій
settings_scripting_op_limit_hint = Діапазон 1 000 - 10 000 000 (за замовчуванням 100 000)
settings_scripting_engine_timeout_label = Тайм-аут (мс)
settings_scripting_engine_timeout_hint = Діапазон 50 - 10 000 (за замовчуванням 500)
settings_scripting_http_section = HTTP-пісочниця
settings_scripting_allowed_domains_label = Дозволені домени
settings_scripting_allowed_domains_hint = Запити до незареєстрованих доменів блокуються. Маски: *.example.com
settings_scripting_domains_placeholder = наприклад: api.example.com
settings_scripting_max_calls_label = Максимум запитів на скрипт
settings_scripting_max_calls_hint = Діапазон 1 - 100 (за замовчуванням 10)
settings_scripting_http_timeout_label = Тайм-аут запиту (мс)
settings_scripting_http_timeout_hint = Діапазон 100 - 30 000 (за замовчуванням 5 000)
settings_scripting_max_response_label = Максимальний розмір відповіді (КіБ)
settings_scripting_max_response_hint = Діапазон 1 - 10 240 (за замовчуванням 1 024 КіБ = 1 МіБ)
settings_scripting_allow_local_label = Дозволити localhost / приватні IP
settings_scripting_allow_local_description = Вимикає захист від SSRF. Вмикайте лише для локальної розробки.
settings_scripting_ssrf_warning = УВАГА - вимикає захист від SSRF. Вмикайте лише для локальної розробки.

## Actions trigger picker - category labels

trigger_cat_chat = Чат
trigger_cat_subscriptions = Підписки
trigger_cat_bits = Бітси
trigger_cat_raids = Рейди
trigger_cat_moderation = Модерація
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
trigger_subgroup_scenes = Сцени
trigger_subgroup_sources = Джерела
trigger_subgroup_audio = Аудіо
trigger_subgroup_filters = Фільтри
trigger_subgroup_streaming = Трансляція
trigger_subgroup_recording = Запис
trigger_subgroup_studio_mode = Студійний режим
trigger_subgroup_transitions = Переходи
trigger_subgroup_virtual_camera = Віртуальна камера
trigger_subgroup_connection = З'єднання
trigger_subgroup_scene_collections = Колекції сцен
trigger_subgroup_profiles = Профілі

## Actions modals - placeholder literals

actions_name_placeholder = Моя автоматизація
actions_group_placeholder = Приклади
actions_description_placeholder = Відтворює звук, показує сповіщення…
actions_log_message_placeholder = Дію розпочато
actions_speak_text_placeholder = Текст для озвучення…
actions_rename_placeholder = Назва

## Actions - list states, toasts, delete confirm

actions_tree_loading = Завантаження дій…
actions_loading_queues = Завантаження черг…
actions_no_queue = Немає доступної черги
actions_toast_error = Дії: { $message }
actions_rename_taken = Назву '{ $name }' вже зайнято
actions_deleted_toast = Видалено '{ $name }'
actions_delete_title = Видалити дію?
actions_delete_body = Це видалить дію та всі її піддії й тригери.

## Triggers registry - error messages

triggers_delete_reference_block = Вилучіть цей тригер з усіх дій перед видаленням.

## TTS - підписи вкладок

tts_tab_dashboard = Панель
tts_tab_engines = Рушії
tts_tab_aliases = Псевдоніми голосів
tts_tab_filters = Фільтри
tts_tab_triggers = Тригери

## TTS - хлібні крихти

tts_breadcrumb_builtin = Вбудоване
tts_breadcrumb_tts = Text-to-Speech

## TTS Dashboard - смуга керування

tts_dash_pause_btn = Пауза черги
tts_dash_resume_btn = Продовжити
tts_dash_skip_btn = Пропустити
tts_dash_stop_all_btn = Зупинити все
tts_dash_stop_all_confirm_name = Зупинити всю озвучку
tts_dash_stop_all_confirm_hint = Повідомлення, що зараз озвучується, буде обірвано, а всі повідомлення в черзі - скинуто. Рушії залишаться готові обробляти нові повідомлення.
tts_dash_test_placeholder = Введіть текст для тестування…
tts_dash_speak_btn = Озвучити
tts_dash_test_speaker_name = Тест

## TTS Dashboard - зараз говорить

tts_dash_now_speaking_header = ЗАРАЗ ГОВОРИТЬ
tts_dash_no_speaking = -
tts_dash_last_drop = Останній запит відхилено: { $reason }

## TTS Dashboard - черга

tts_dash_queue_header = Наступний
tts_dash_queue_total = ~{ $secs }с усього
tts_dash_queue_empty = Черга порожня
tts_dash_play_now = Відтворити зараз
tts_dash_remove_queued = Прибрати з черги

## TTS Dashboard - статистика сесії

tts_dash_session_header = СЕСІЯ
tts_dash_stat_spoken = Озвучено
tts_dash_stat_skipped = Пропущено
tts_dash_stat_filtered = Відфільтровано
tts_dash_stat_avg_latency = Сер. затримка
tts_dash_engines_header = РУШІЇ
tts_dash_engines_none = Немає доступних рушіїв
tts_dash_engine_no_voices = голоси не встановлено

## TTS Engines - список

tts_engines_header_prefix = НАЛАШТОВАНО
tts_engines_add_engine = Додати рушій
tts_engines_add_none_left = Усі хмарні рушії налаштовано
tts_engines_select_hint = Оберіть рушій для налаштування
tts_header_engines_ready = Готово рушіїв: { $count }
tts_engines_rail_sub = { $kind } · { $count ->
    [one] { $count } голос
    [few] { $count } голоси
    [many] { $count } голосів
   *[other] { $count } голосів
}

## TTS Engines - заголовок деталей

tts_engines_detail_sub = рушій { $kind } · { $count ->
    [one] { $count } голос
    [few] { $count } голоси
    [many] { $count } голосів
   *[other] { $count } голосів
}
tts_engines_detail_sub_region = рушій { $kind } · { $region } · { $count ->
    [one] { $count } голос
    [few] { $count } голоси
    [many] { $count } голосів
   *[other] { $count } голосів
}

## TTS Engines - секції

tts_engines_section_credentials = ОБЛІКОВІ ДАНІ
tts_engines_creds_encrypted_note = Зберігається зашифровано в локальній базі даних
tts_engines_section_params = ПАРАМЕТРИ ГОЛОСУ ЗА ЗАМОВЧУВАННЯМ
tts_engines_param_pitch = Висота тону
tts_engines_param_speed = Швидкість
tts_engines_param_volume = Гучність

## TTS Engines - секція голосів

tts_engines_voices_header_prefix = ГОЛОСИ
tts_engines_voices_available = доступно: { $count }
tts_engines_voices_empty = Голосів не знайдено
tts_engines_voice_preview_sample = Це мій голос.

## TTS Filters - колонка пайплайну

tts_filters_pipeline_header = КОНВЕЄР ОБРОБКИ
tts_filters_pipeline_hint = Кожне повідомлення проходить ці етапи по черзі перед озвученням
tts_filters_pipeline_intro = Конвеєр, який текст проходить перед озвученням.

## TTS Filters - нумеровані картки етапів

tts_filters_stage_emote_url_title = Емоції та обробка URL
tts_filters_stage_skip_title = Стоп-правила
tts_filters_stage_replacements_title = Заміни тексту
tts_filters_stage_blocklist_title = Блок-список слів
tts_filters_stage_output_title = Вивід

## TTS Filters - стоп-правила

tts_filters_skip_contains_url = Містить URL
tts_filters_skip_prefix = Починається з { $prefix }
tts_filters_skip_bot_accounts = Від бот-акаунтів
tts_filters_skip_longer_than = Повідомлення довше за { $chars } символів
tts_filters_skip_repeat = Ідентичне останнім { $window } повідомленням
tts_filters_skip_emote_only = Повідомлення лише з емоцій
tts_filters_skip_mostly_non_latin = Переважно не латиниця
tts_filters_skip_regex_row = Regex: { $pattern }

## TTS Filters - блок-список слів

tts_filters_blocklist_censor = Цензурувати збіги
tts_filters_blocklist_censor_meta = замінити на ***
tts_filters_blocklist_skip = Пропустити все повідомлення при збігу
tts_filters_blocklist_more = +{ $count } ще
tts_filters_blocklist_empty = Заблокованих слів ще немає

## TTS Filters - заміни тексту

tts_filters_replacements_empty = Замін ще немає

## TTS Filters - вивід

tts_filters_output_read_name = Спершу читати відображуване ім'я
tts_filters_output_read_name_meta = напр. "koval_dev каже: ..."
tts_filters_output_emote = Емоція → слово
tts_filters_output_emote_meta = перетворити :pog: → "pog"
tts_filters_output_sanitize = Прибирати повтори пунктуації
tts_filters_output_sanitize_meta = "!!!" → "!"

## TTS Filters - список правил

tts_filters_no_rules = Правил фільтрування ще немає
tts_filters_add_rule_btn = Додати правило
tts_filters_rule_on = УВІМК
tts_filters_rule_off = ВИМК
tts_filters_kind_literal = Заміна тексту
tts_filters_kind_regex = Заміна regex
tts_filters_kind_blocklist = Блок-список
tts_filters_badge_text = ТЕКСТ
tts_filters_badge_regex = REGEX
tts_filters_badge_block = БЛОК
tts_filters_stage_add = Додати

## TTS Filters - модальне вікно додавання фільтра

tts_filters_modal_skip_title = Додати стоп-правило
tts_filters_modal_skip_subtitle = Повідомлення, що збігаються, ніколи не озвучуються
tts_filters_modal_blocklist_title = Додати заблоковані слова
tts_filters_modal_blocklist_subtitle = Збіги цензуруються або повідомлення пропускається
tts_filters_modal_replace_title = Додати заміну тексту
tts_filters_modal_replace_subtitle = Переписати текст перед озвученням
tts_filters_modal_output_title = Додати опцію виводу
tts_filters_modal_output_subtitle = Сформувати остаточний озвучений текст

tts_filters_modal_condition_label = УМОВА
tts_filters_modal_cancel = Скасувати
tts_filters_modal_add_rule = Додати правило
tts_filters_modal_add_words = Додати слова
tts_filters_modal_footer_valid = Виконується по черзі в межах цього етапу
tts_filters_modal_footer_invalid = Заповніть обов'язкові поля

tts_filters_preset_skip_url = Містить URL
tts_filters_preset_skip_prefix = Починається з префіксу
tts_filters_preset_skip_prefix_label = ПРЕФІКС
tts_filters_preset_skip_prefix_placeholder = !
tts_filters_preset_skip_bots = Від бот-акаунтів
tts_filters_preset_skip_length = Довше за N символів
tts_filters_preset_skip_length_label = МАКС. СИМВОЛІВ
tts_filters_preset_skip_length_placeholder = 200
tts_filters_preset_skip_repeat = Ідентичне нещодавнім повідомленням
tts_filters_preset_skip_emote_only = Повідомлення лише з емоцій
tts_filters_preset_skip_non_latin = Переважно не латиниця
tts_filters_preset_skip_regex = Власний regex-збіг
tts_filters_preset_skip_regex_label = REGEX-ШАБЛОН
tts_filters_preset_skip_regex_placeholder = (buy|cheap) followers

tts_filters_preset_output_name_hint = напр. "koval_dev каже: ..."
tts_filters_preset_output_emote_hint = перетворити :pog: → "pog"
tts_filters_preset_output_lang = Автовизначення мови
tts_filters_preset_output_lang_hint = обирати голос за мовою повідомлення
tts_filters_preset_output_maxdur = Обрізати після N секунд
tts_filters_preset_output_maxdur_hint = зупиняти довгі повідомлення раніше
tts_filters_preset_output_sanitize_hint = "!!!" → "!"

tts_filters_modal_blocklist_words_label = СЛОВА АБО ФРАЗИ
tts_filters_modal_blocklist_words_placeholder = по одному на рядок або через кому...
tts_filters_modal_blocklist_note = Збіг не чутливий до регістру та шукає лише цілі слова; багатослівні фрази не збігаються як одне ціле.
tts_filters_modal_blocklist_when_matched_label = ПРИ ЗБІГУ
tts_filters_modal_blocklist_censor_row = Цензурувати слово
tts_filters_modal_blocklist_censor_row_hint = замінити на ***
tts_filters_modal_blocklist_skip_row = Пропустити все повідомлення
tts_filters_modal_blocklist_skip_row_hint = нічого не озвучується

tts_filters_modal_replace_text_tab = ТЕКСТ
tts_filters_modal_replace_regex_tab = REGEX
tts_filters_modal_replace_find_label = ЗНАЙТИ
tts_filters_modal_replace_match_label = ШАБЛОН ЗБІГУ
tts_filters_modal_replace_find_placeholder = POG
tts_filters_modal_replace_match_placeholder = https?://\S+
tts_filters_modal_replace_replace_label = ЗАМІНИТИ НА
tts_filters_modal_replace_replace_text_placeholder = повага
tts_filters_modal_replace_replace_regex_placeholder = "посилання" або $1
tts_filters_modal_replace_note = Залиште заміну порожньою, щоб прибрати збіг.

## TTS Filters - налаштування конвеєра

tts_filters_url_label = ОБРОБКА URL
tts_filters_url_speak = Читати URL вголос
tts_filters_url_replace = Замінити на "link"
tts_filters_url_suppress = Пропустити повідомлення
tts_filters_length_label = МАКС. ДОВЖИНА
tts_filters_length_placeholder = Без обмеження
tts_filters_blocklist_default_label = РЕЖИМ БЛОК-СПИСКУ ЗА ЗАМОВЧУВАННЯМ
tts_filters_strip_twitch = Прибирати емоції Twitch
tts_filters_strip_reward = Прибирати емоції нагород
tts_filters_unsaved = Незбережені зміни
tts_filters_saved = Усі зміни збережено

## TTS Filters - колонка попереднього перегляду

tts_filters_preview_header = Живий перегляд
tts_filters_preview_input_label = ВХІДНЕ ПОВІДОМЛЕННЯ
tts_filters_preview_input_placeholder = Введіть повідомлення для перегляду…
tts_filters_preview_empty = Введіть повідомлення вище для перегляду
tts_filters_preview_output_label = ВИХІД СТАДІЙ
tts_filters_preview_final_label = РЕЗУЛЬТАТ
tts_filters_speak_preview_btn = Озвучити перегляд
tts_filters_preview_speaker_name = Перегляд
tts_filters_stage_pass = без змін
tts_filters_stage_skipped = пропущено
tts_filters_stage_name_skip_rules = СТОП-ПРАВИЛА
tts_filters_stage_name_replacements = ЗАМІНИ
tts_filters_stage_name_blocklist = БЛОК-СПИСОК
tts_filters_stage_name_output = ВИВІД
tts_filters_skip_reason_rule = спрацювало стоп-правило
tts_filters_skip_reason_blocked = заблоковане слово
tts_filters_skip_reason_empty = порожнє після фільтрів
tts_filters_delete_title = Видалити правило?
tts_filters_delete_body = Це правило буде вилучено з конвеєра попередньої обробки.

## TTS Triggers - заголовок

tts_triggers_header = ЩО ОЗВУЧУЄТЬСЯ
tts_triggers_hint = Вмикайте джерела та визначайте, хто може запускати TTS

## TTS Triggers - картка команди

tts_triggers_cmd_title = Команда в чаті
tts_triggers_cmd_subtitle = !tts <повідомлення>
tts_triggers_cmd_meta = затримка 8 с · макс. 250 символів

## TTS Triggers - картка балів каналу

tts_triggers_points_title = Нагорода за бали каналу
tts_triggers_points_subtitle = «Озвучити моє повідомлення» · 500 балів
tts_triggers_points_meta = без затримки · пріоритетна черга

## TTS Triggers - картка бітсів

tts_triggers_bits_title = Бітси / вигуки
tts_triggers_bits_subtitle = Озвучити повідомлення вигуку
tts_triggers_bits_min_label = Мінімум
tts_triggers_bits_min_value = 100 бітів
tts_triggers_bits_meta = більше = довше повідомлення

## TTS Triggers - картка підписок

tts_triggers_subs_title = Повідомлення підписок
tts_triggers_subs_subtitle = Озвучувати ресаб / подаровані підписки
tts_triggers_subs_disabled = Вимкнено - увімкніть перемикач

## TTS Triggers - картка формату

tts_triggers_format_header = ФОРМАТ ПОВІДОМЛЕННЯ
tts_triggers_format_read_username = Читати ім'я користувача перед повідомленням
tts_triggers_format_template_header = ШАБЛОН
tts_triggers_format_speak_emotes = Вимовляти емоути як слова

## TTS Triggers - картка поведінки черги

tts_triggers_queue_header = ПОВЕДІНКА ЧЕРГИ
tts_triggers_queue_max_length = Максимальна довжина черги
tts_triggers_queue_per_user_limit = Ліміт на одного користувача в черзі
tts_triggers_queue_bits_skip = Бітси та бали пропускають чергу

## TTS Triggers - чіпи ролей

tts_triggers_role_subscribers = Підписники
tts_triggers_role_vips = VIP
tts_triggers_role_mods = Модератори
tts_triggers_role_everyone = Усі

## Хмарні рушії - кнопки картки

tts_cloud_test_connection_btn = Перевірити підключення
tts_cloud_testing_btn = Перевірка…
tts_cloud_save_credentials_btn = Зберегти облікові дані

## Хмарні рушії - бейджі статусу

tts_cloud_not_configured = НЕ НАЛАШТОВАНО
tts_cloud_configured = НАЛАШТОВАНО
tts_cloud_connection_failed = ЗБІЙ ПІДКЛЮЧЕННЯ

## Хмарні рушії - результат тесту

tts_cloud_connection_verified = Підключення підтверджено

## Хмарні рушії - сповіщення

tts_cloud_saved_toast = Рушій { $name } готовий до роботи - перезапуск не потрібен.
tts_cloud_save_failed_toast = Не вдалося зберегти облікові дані { $name }: { $error }

## Псевдоніми голосів - банер стратегії

tts_aliases_strategy_label = Типова стратегія призначення
tts_aliases_strategy_sublabel = Як обирається голос для глядачів без ручного псевдоніма
tts_aliases_strategy_deterministic = Детерміновано за ім'ям
tts_aliases_strategy_random = Випадково
tts_aliases_strategy_single = Один голос

## Псевдоніми голосів - панель інструментів

tts_aliases_search_placeholder = Пошук глядачів…
tts_aliases_count = { $count ->
    [one] { $count } псевдонім
    [few] { $count } псевдоніми
    [many] { $count } псевдонімів
   *[other] { $count } псевдонімів
}
tts_aliases_assign_btn = Призначити голос

## Псевдоніми голосів - заголовки таблиці

tts_aliases_col_viewer = ГЛЯДАЧ
tts_aliases_col_voice = ГОЛОС
tts_aliases_col_pitch = ВИСОТА
tts_aliases_col_speed = ШВИДКІСТЬ
tts_aliases_col_actions = ДІЇ

## Псевдоніми голосів - порожній стан

tts_aliases_empty = Псевдонімів голосів ще немає
tts_aliases_loading = Завантаження псевдонімів голосів…

## Псевдоніми голосів - заблокований рядок

tts_aliases_never_speak = Ніколи не озвучувати

## TTS Псевдоніми голосів - бейджі ролей

tts_aliases_role_mod = МОД
tts_aliases_role_vip = VIP
tts_aliases_role_sub = САБ
tts_aliases_role_blocked = БЛОК

## Голосові аліаси - модалка призначення/редагування

tts_aliases_form_title_assign = Призначити голос
tts_aliases_form_title_edit = Редагувати голосовий аліас
tts_aliases_form_viewer_label = ГЛЯДАЧ
tts_aliases_form_viewer_placeholder = Імʼя глядача
tts_aliases_form_engine_label = ДВИГУН
tts_aliases_form_engine_placeholder = Оберіть двигун
tts_aliases_form_voice_label = ГОЛОС
tts_aliases_form_voice_placeholder = Ідентифікатор голосу
tts_aliases_form_pitch_label = ТОН (пт)
tts_aliases_form_pitch_placeholder = 0
tts_aliases_form_rate_label = ТЕМП (x)
tts_aliases_form_rate_placeholder = 1.0
tts_aliases_form_create = Створити
tts_aliases_form_block_label = Ніколи не озвучувати
tts_aliases_form_block_desc = Повідомлення цього глядача ніколи не озвучуються.
tts_aliases_form_blocked_note = Не озвучувати - налаштування голосу не застосовуються.

## Голосові аліаси - підтвердження видалення

tts_aliases_delete_title = Видалити голосовий аліас?
tts_aliases_delete_body = { $viewer } повернеться до типової стратегії призначення голосу.
common_delete = Видалити
common_undo = Відмінити

## Голосові аліаси - попереднє прослуховування

tts_aliases_preview_text = Це попереднє прослуховування голосу.

## Голосові аліаси - підпис під таблицею

tts_aliases_footer_caption = Показано { $shown } з { $total } ручних аліасів · { $auto } глядачів озвучуються автоматично

## Звукова панель - хлібні крихти

soundboard_breadcrumb_builtin = Вбудоване
soundboard_breadcrumb_soundboard = Звукова панель

## Звукова панель - заголовок / модальне вікно

soundboard_add_clip_btn = Додати кліп
soundboard_loading = Завантаження кліпів…
soundboard_empty_title = Кліпів ще немає
soundboard_empty_hint = Натисніть «Додати кліп», щоб додати перший звук.
soundboard_playback_error_prefix = Помилка відтворення: { $error }

## Звукова панель - модальне вікно

soundboard_modal_title_add = Додати кліп
soundboard_modal_title_edit = Редагувати кліп
soundboard_modal_no_file = Файл не обрано
soundboard_modal_browse_btn = Огляд
soundboard_modal_name_placeholder = Назва кліпу
soundboard_modal_hotkey_placeholder = наприклад Ctrl+1
soundboard_modal_devices_loading = Завантаження пристроїв…
soundboard_modal_save_btn = Зберегти
soundboard_modal_saving_btn = Збереження…
soundboard_modal_cancel_btn = Скасувати
soundboard_modal_validation_error = Назва та аудіофайл обов'язкові.

## Звукова панель - назви секцій модального вікна

soundboard_modal_section_file = ФАЙЛ
soundboard_modal_section_name = НАЗВА
soundboard_modal_section_hotkey = ХОТКЕЙ
soundboard_modal_section_device = ПРИСТРІЙ ВИВОДУ
soundboard_device_system_default = Системний за замовчуванням
soundboard_modal_section_volume = ГУЧНІСТЬ

## Звукова панель - помилка завантаження пристроїв

soundboard_modal_device_load_error = Помилка завантаження пристроїв: { $error }

## Звукова панель - помилка аудіоплеєра

soundboard_player_not_init = Аудіоплеєр не ініціалізовано - перевірте Налаштування → Аудіо.

## Звукова панель - зворотний зв'язок

soundboard_playing_feedback = Відтворення "{ $name }" → { $device }. Живе аудіо буде під'єднано через рантайм незабаром.
soundboard_removed_feedback = Видалено "{ $name }".
soundboard_saved_feedback = Збережено "{ $name }". Маршрутизацію відтворення буде під'єднано через рантайм незабаром.
soundboard_modal_kbd_hint = Enter - зберегти · Esc - скасувати

## Черги - заголовок сторінки

queues_breadcrumb_automation = Автоматизація
queues_breadcrumb_queues = Черги
queues_pause_all_btn = Пауза всіх
queues_new_queue_btn = Нова черга
queues_subtitle = Керуйте чергами дій, їхньою паралельністю та станом паузи
queues_stat_queues = черг
queues_stat_running = активні
queues_stat_paused = на паузі
queues_empty = Черг не налаштовано.
queues_loading = Завантаження черг…
queues_drain_feedback = Спустошення «{ $name }».
queues_configure_btn = Налаштувати
queues_drain_btn = Спустошити
queues_pause_btn = Пауза
queues_resume_btn = Продовжити

## Черги - меню картки

queues_menu_configure = Налаштувати…
queues_menu_rename = Перейменувати…
queues_menu_pause = Пауза
queues_menu_resume = Продовжити
queues_menu_drain = Спустошити чергу
queues_menu_delete = Видалити…
queues_delete_confirm_title = Видалити чергу
queues_delete_confirm_body = Дії з цієї черги перейдуть до черги Default. Це не можна скасувати.

## Черги - модальне вікно нової черги

queues_create_title = Нова черга
queues_create_name_label = Назва
queues_create_name_placeholder = Назва черги (обовʼязково)
queues_create_desc_label = Опис
queues_create_desc_placeholder = Для чого ця черга
queues_create_desc_optional = (необовʼязково)
queues_concurrency_label = Паралельність
queues_concurrency_serial = Послідовно - лише одна дія за раз
queues_concurrency_parallel = Паралельно - до { $count } дій одночасно
queues_create_btn = Створити чергу
queues_create_cancel = Скасувати
queues_edit_btn = Зберегти зміни
queues_edit_title = Налаштування { $name }
queues_create_subtitle = Як дії виконуються в цій черзі
queues_create_kbd_hint = Esc - скасувати

## Черги - метрики картки

queues_metric_concurrency = ПАРАЛЕЛІЗМ
queues_metric_pending = ОЧІКУЮТЬ
queues_metric_actions = ДІЇ
queues_metric_assigned = призначено
queues_metric_serial = послідовно
queues_metric_parallel = паралельно
queues_metric_in_flight = виконується
queues_metric_idle = очікування
queues_metric_held = заблоковано

## Черги - панель паузи

queues_paused_with_time = { $pending } дій в очікуванні - на паузі { $mins } хв тому
queues_paused_simple = Черга на паузі

## Черги - панель запущених дій

queues_running_now_header = ВИКОНУЄТЬСЯ ЗАРАЗ
queues_no_actions_running = Дій не виконується
queues_running_label = виконується

## Черги - бейдж статусу

queues_status_paused = ПАУЗА
queues_status_running = ВИКОНУЄТЬСЯ

## Черги - розбіжність живого членства

queues_not_live_badge = НЕ В РОБОТІ · ПЕРЕЗАПУСК

## Черги - чіп переповнення

queues_overflow_more = +{ $count } ще

## Черги - описи вбудованих черг


## TTS дашборд - підписи карток рушіїв / бейдж пріоритету

tts_dash_engine_local_ready = локальний · готовий
tts_dash_priority_high = ВИСОК.
tts_dash_priority_bits = БІТСИ { $amount }

## TTS рушії - запасний підпис невідомого рушія

tts_engines_unknown = Невідомий рушій

## Хмарний TTS - підписи полів форми

tts_cloud_field_api_key = API-ключ
tts_cloud_field_region = Регіон
tts_cloud_field_access_key_id = Ідентифікатор ключа доступу
tts_cloud_field_secret_key = Секретний ключ
tts_cloud_field_placeholder_subscription_key = Ключ підписки

## Звукова панель - фільтр файлового діалогу

soundboard_file_filter_audio = Аудіо

## Огляд платформ

platforms_title = Стримінгові платформи
platforms_subtitle = Підключіть один раз - Forge слухатиме всі чати й події в одному місці.
platforms_breadcrumb = Платформи

platforms_status_connected = Підключено
platforms_status_not_connected = Не підключено

platforms_twitch_desc = Чат, підписки EventSub, нагороди каналу, біти, рейди
platforms_youtube_desc = Живий чат, супер-чати, членство в каналі, підписники
platforms_kick_desc = Чат, події каналу, підписники - нова стримінгова платформа

## Платформи - чіпи можливостей

platforms_feature_irc_chat = IRC-чат
platforms_feature_channel_points = Нагороди каналу
platforms_feature_bits_subs = Бітси та підписки
platforms_feature_live_chat = Живий чат
platforms_feature_super_chat = Супер-чат
platforms_feature_memberships = Членства
platforms_feature_chat = Чат
platforms_feature_subs = Підписки
platforms_feature_channel_events = Події каналу

## Загальна сторінка платформи

platform_generic_features_available = ЩО МОЖНА РОБИТИ ПІСЛЯ ПІДКЛЮЧЕННЯ
platform_generic_features_coming = ЩО БУДЕ ДОСТУПНО
platform_generic_kind_platform = Стримінгова платформа
platform_generic_kind_stream_app = Стрим-додаток
platform_generic_status_available = доступно - натисніть «Підключити» для авторизації
platform_generic_status_coming = ще не реалізовано
platform_generic_parent_platforms = Платформи
platform_generic_parent_stream_apps = Стрим-додатки
platform_generic_connect_btn = Підключити

## Панель Twitch

twitch_breadcrumb_platforms = Платформи
twitch_header_subtitle = Підключіть, щоб увімкнути чат, підписки, біти, рейди, нагороди каналу та EventSub
twitch_auth_title = Авторизувати Forge на Twitch
twitch_auth_subtitle = Twitch використовує авторизацію за кодом пристрою. Ви побачите код тут - введіть його на сайті Twitch, і ми автоматично визначимо, коли ви закінчите. Ми ніколи не бачимо вашого пароля.
twitch_btn_start = Почати авторизацію
twitch_btn_try_again = Спробувати знову
twitch_btn_cancel = Скасувати
twitch_btn_restart = Перезапустити
twitch_btn_open = Відкрити
twitch_requesting = Запит коду авторизації від Twitch…
twitch_authorizing = Код прийнято. Завершення авторизації…
twitch_polling_primary = Очікування авторизації на Twitch…
twitch_polling_secondary = перевірка кожні 5с
twitch_step1_title = Відкрийте це посилання в будь-якому браузері
twitch_step2_title = Підтвердьте у браузері
twitch_step2_detail = Forge прослуховує локальний порт для зворотного виклику OAuth. Вікно оновиться після підтвердження.
twitch_timer_prefix = Закінчується через
twitch_scopes_header = Дозволи, які запитуватиме Forge
twitch_scopes_count = { $count } областей
twitch_missing_client_id = Інтеграцію Twitch не налаштовано. Встановіть FORGE_TWITCH_CLIENT_ID із client_id вашого зареєстрованого додатка та перезапустіть.
twitch_reauth_title = Токен Twitch не має необхідних областей
twitch_reauth_detail = EventSub відхилив підписку на чат. Виконайте повторну авторизацію, щоб оновити токен із поточними областями.
twitch_reauth_btn = Повторна авторизація

## Панель OBS

obs_breadcrumb_stream_apps = Стрим-додатки
obs_header_subtitle = Підключіть для керування сценами, джерелами, аудіо, фільтрами та записом
obs_instructions_title = Перш ніж почати
obs_instructions_lead = У OBS Studio увімкніть вбудований сервер WebSocket, потім скопіюйте налаштування сюди.
obs_step1 = У OBS: Інструменти → Налаштування сервера WebSocket
obs_step2 = Позначте «Увімкнути сервер WebSocket»
obs_step3 = Запишіть порт (за замовчуванням 4455)
obs_step4 = Натисніть «Показати інформацію підключення», щоб побачити пароль
obs_requirements_header = ВИМОГИ
obs_req_version = OBS Studio 28+ (WebSocket v5 вбудовано)
obs_req_network = Та сама машина або доступна в LAN
obs_form_title = Налаштування підключення
obs_field_host = ХОСТ
obs_field_port = ПОРТ
obs_field_password = ПАРОЛЬ
obs_field_keychain = зберігається зашифровано в локальній базі
obs_toggle_reconnect_title = Авто-перепідключення при відключенні
obs_toggle_reconnect_subtitle = Повторні спроби з експоненційним відступом
obs_toggle_launch_title = Підключатися при запуску
obs_toggle_launch_subtitle = Починати підключення при відкритті Forge
obs_btn_test = Тест підключення
obs_btn_connect = Підключити
obs_test_running = Тестування підключення…
obs_test_success = Тест успішний
obs_test_failed = Тест не вдався
obs_tip = Запускаєте OBS на іншому ПК? Вкажіть IP тієї машини. Переконайтеся, що OBS WebSocket прив'язаний до 0.0.0.0, а не до localhost, і порт відкритий у файрволі.
obs_port_invalid = порт має бути числом від 1 до 65535

## Вбудована деталь

builtin_breadcrumb = Вбудований
builtin_picker_scene = Оберіть сцену
builtin_picker_source = Оберіть джерело
builtin_picker_audio_input = Оберіть аудіовхід
builtin_picker_hotkey = Оберіть гарячу клавішу
builtin_picker_expression = Оберіть вираз
builtin_picker_midi_port = Оберіть порт MIDI

## OAuth / локальний callback-потік

oauth_header_subtitle = Підключіть для доступу до живого чату та подій
oauth_auth_title = Авторизувати Forge на { $name }
oauth_auth_subtitle = Ця платформа використовує авторизацію за кодом. Ви побачите посилання нижче - перейдіть на сайт платформи і підтвердьте. Ми ніколи не бачимо вашого пароля.
oauth_btn_connect = Підключити
oauth_btn_retry = Повторити
oauth_btn_cancel = Скасувати
oauth_btn_return = Повернутися до платформ
oauth_step1_title = Відкрийте це посилання в будь-якому браузері
oauth_step1_open = Відкрити
oauth_step2_title = Підтвердьте у браузері
oauth_step2_detail = Forge прослуховує локальний порт для зворотного виклику OAuth. Вікно оновиться після підтвердження.
oauth_polling_primary = Очікування авторизації на платформі…
oauth_polling_secondary = перевірка кожні 5с
oauth_requesting = Запит коду авторизації…
oauth_authorized_title = Підключено до { $name }!
oauth_authorized_subtitle = Авторизацію завершено.
oauth_failed_title = Авторизація не вдалася

## Екран сервера

server_breadcrumb_builtin = Вбудований
server_breadcrumb_server = Сервер
server_header_title = Вбудований сервер
server_header_desc = Внутрішній HTTP + WebSocket сервер для оверлеїв і дистанційного керування
server_status_running = Запущено
server_status_stopped = Зупинено
server_status_error = Помилка
server_not_running = Не запущено
server_up_prefix = Працює { $uptime }
server_bind_address = АДРЕСА ПРИВ'ЯЗКИ
server_bearer_token = BEARER-ТОКЕН
server_btn_restart = Перезапуск
server_btn_restarting = Перезапуск…
server_btn_stop = Зупинити
server_btn_stopping = Зупинка…
server_btn_copy = КОПІЯ
server_stat_clients = КЛІЄНТИ
server_stat_clients_sub = підключено
server_stat_events_out = ПОДІЇ НАЗОВНІ
server_stat_events_sub = сер. { $avg } под/с
server_stat_http = HTTP-ЗАПИТИ
server_stat_http_sub = оверлеїв подано
server_stat_bandwidth = ПРОПУСКНА ЗДАТНІСТЬ
server_stat_bandwidth_sub = пік { $peak } КБ/с
server_clients_header = Підключені клієнти
server_clients_empty = Немає підключених клієнтів
server_col_client = КЛІЄНТ
server_col_subscriptions = ПІДПИСКИ
server_col_evs = ПОД/С
server_col_uptime = АПТАЙМ
server_overlay_files_empty = Файли оверлею не знайдено
server_overlay_dir_items = { $count ->
    [one] { $count } елемент
    [few] { $count } елементи
    [many] { $count } елементів
   *[other] { $count } елементів
}
server_disconnect_confirm_hint = Клієнта { $info } буде відключено від WebSocket-сервера. Інші клієнти не постраждають.
server_btn_regenerate = Оновити
server_regen_warning_title = Оновлення відключить усіх клієнтів
server_regen_warning_body = Підключені WebSocket-клієнти мають перепідключитися з новим токеном.
server_throughput_title = Пропускна здатність
server_throughput_meta = останні { $seconds }с · пік { $peak } КБ/с
server_overlay_files_title = Файли оверлею
server_btn_open = ВІДКРИТИ
server_clients_live = наживо
server_footer_totals = Надіслано всього: { $sent } · Подій всього: { $events }
server_disconnect_confirm_title = Відключити клієнта?
server_disconnect_esc_hint = щоб скасувати
server_btn_disconnect = Відключити

## Загальні бейджі статусу (використовуються на сторінках деталей платформ)

common_status_not_connected = Не підключено
common_status_coming_soon = Незабаром

## Деталі платформи YouTube

youtube_description = Живий чат, супер-чати, членство в каналі, підписники.
youtube_feature_live_chat = Живий чат із маркерами настрою
youtube_feature_super_chat = Сповіщення Super Chat з рівнями аналогічно бітсам
youtube_feature_memberships = Події вступу/підвищення/скасування членства в каналі
youtube_feature_subscribers = Тригери досягнень підписників

## Деталі платформи Kick

kick_description = Чат, підписки, хости - гібрид: офіційний OAuth API для відправки, Pusher WS спільноти для отримання. Не афілійовано з Kick.com.
kick_feature_live_chat = Живий чат (отримання + відправка через OAuth)
kick_feature_subs = Події підписок і подарованих підписок
kick_feature_hosts_bans = Події хостів і банів
kick_feature_deleted_replies = Події видалення повідомлень і відповідей

## Деталі VTube Studio

vtube_description = Керування аватаром Vtuber: гарячі клавіші, вирази, тригери предметів.
vtube_feature_hotkeys = Запускати гарячі клавіші з подій чату
vtube_feature_expressions = Перемикати вирази та вбрання
vtube_feature_item_drops = Спавнити випадання предметів на бітси/підписки

## Огляд стрим-додатків

stream_apps_title = Стрим-додатки
stream_apps_subtitle = Локальні додатки, з якими Forge спілкується через WebSocket. Підключіть для керування з дій.
stream_apps_breadcrumb = Стрим-додатки
stream_apps_obs_desc = Сцени, джерела, керування записом, буфери реплею - повний obs-websocket API
stream_apps_vtube_desc = Керування аватаром Vtuber: гарячі клавіші, вирази, тригери предметів

## Живий чат - заголовок / фільтри

chat_breadcrumb_audience = Аудиторія
chat_breadcrumb_chat = Чат
chat_filter_all = Всі
chat_filter_events = Лише події
chat_filter_hide_bots = Сховати ботів
chat_viewers_unit = глядачів
chat_no_filter_matches = Жодне повідомлення не відповідає цим фільтрам.
chat_send_placeholder_disconnected = Підключіть платформу для відправки...
chat_send_placeholder_connected = Надіслати в чат...
chat_send_placeholder_to = Надіслати в чат {$platform}...
chat_no_messages_title = Немає повідомлень
chat_no_messages_empty = Не підключено - перейдіть до Налаштувань → Платформи.
chat_no_events_yet = Подій ще немає.
chat_no_search_matches = Жодних повідомлень не відповідає пошуку.
chat_messages_count = { $count ->
    [one] { $count } повідомлення
    [few] { $count } повідомлення
    [many] { $count } повідомлень
   *[other] { $count } повідомлень
}
chat_matches_count = { $count ->
    [one] { $count } збіг
    [few] { $count } збіги
    [many] { $count } збігів
   *[other] { $count } збігів
}
chat_header_viewers = { $count ->
    [one] { $formatted } глядач
    [few] { $formatted } глядачі
    [many] { $formatted } глядачів
   *[other] { $formatted } глядачів
}
chat_show_viewers = Показати глядачів
chat_hide_viewers = Сховати глядачів
chat_search_placeholder = Пошук повідомлень...
chat_new_message = 1 нове повідомлення
chat_new_messages = { $count ->
    [one] { $count } нове повідомлення
    [few] { $count } нових повідомлення
    [many] { $count } нових повідомлень
   *[other] { $count } нових повідомлень
}
chat_viewers_title = Глядачі

## Живий чат - бічна панель глядачів

chat_drawer_search_placeholder = Шукати глядачів...
chat_drawer_active_count = { $total } активних · { $shown } показано
chat_drawer_section_active = ЗАРАЗ АКТИВНІ · { $count }
chat_drawer_no_matches = Жоден учасник чату не відповідає пошуку
chat_drawer_click_hint = Натисніть на ім'я в чаті, щоб побачити деталі
chat_drawer_last_seen = Востаннє { $when }
chat_drawer_shoutout = Shoutout
chat_drawer_whisper = Whisper
chat_drawer_whisper_title = Шепіт для { $recipient }
chat_drawer_whisper_placeholder = Введіть повідомлення…
chat_drawer_whisper_send = Надіслати
chat_drawer_whisper_cancel = Скасувати
chat_drawer_set_tts_voice = Встановити голос TTS…
chat_drawer_block_tts = Заблокувати TTS
chat_drawer_timeout = Таймаут 10 хв
chat_drawer_ban = Заблокувати в каналі
chat_stat_watch_time = ЧАС ПЕРЕГЛЯДУ
chat_stat_messages = ПОВІДОМЛЕННЯ
chat_stat_sub = ПІДПИСКА
chat_stat_sub_yes = Так
chat_stat_follow = ФОЛОУ
chat_drawer_shoutout_sent = Shoutout надіслано
chat_drawer_shoutout_failed = Помилка shoutout: { $error }
chat_drawer_whisper_sent = Шепіт надіслано
chat_drawer_whisper_failed = Помилка шепоту: { $error }
chat_drawer_timeout_sent = Глядача заблоковано на 10 хв
chat_drawer_timeout_failed = Помилка таймауту: { $error }
chat_drawer_ban_sent = Глядача забанено
chat_drawer_ban_failed = Помилка бану: { $error }
chat_drawer_block_tts_sent = Глядача заблоковано в TTS
chat_drawer_block_tts_failed = Помилка блокування TTS: { $error }
chat_ctx_timeout_10m = Таймаут 10 хв
chat_ctx_timeout_1h = Таймаут 1 година
chat_ctx_timeout_2w = Таймаут 2 тижні
chat_ctx_ban = Бан
chat_ctx_timeout_sent = Таймаут застосовано
chat_reply = Відповісти
chat_reply_title = Відповідь для @{ $recipient }
chat_reply_placeholder = Введіть відповідь…
chat_reply_sent = Відповідь надіслано
chat_reply_failed = Помилка відповіді: { $error }

## Стрічка подій - заголовок / фільтри

event_feed_filter_all = Всі { $n }
event_feed_filter_chat = Чат { $n }
event_feed_filter_subs = Підписки { $n }
event_feed_filter_bits = Бітси { $n }
event_feed_filter_timers = Таймери { $n }
event_feed_filter_obs = OBS { $n }
event_feed_filter_errors = Помилки { $n }
event_feed_pause = Призупинити
event_feed_resume = Продовжити
event_feed_clear = Очистити
event_feed_export = Експорт
event_feed_export_success = Стрічку подій експортовано до { $path }
event_feed_export_failed = Не вдалося експортувати стрічку подій: { $error }
event_feed_no_events = Подій ще немає - системна активність з'явиться тут в реальному часі.
event_feed_no_filter_match = Жодна подія не відповідає активному фільтру.
event_feed_inspector_title = Інспектор подій
event_feed_inspector_hint = Виберіть подію, щоб переглянути її payload.
event_feed_auto_scroll_on = Авто-прокрутка увімкнена
event_feed_auto_scroll_off = Авто-прокрутка вимкнена
event_feed_buffer = Буфер: { $count } / 10 000
event_feed_rate = { $rate } под/с
event_feed_breadcrumb_automation = Автоматизація
event_feed_breadcrumb_feed = Стрічка подій
event_feed_status_live = НАЖИВО
event_feed_status_paused = ПАУЗА
event_feed_header_count = Подій: { $count }
event_feed_streaming_status = Стрімінг · WebSocket :8081
event_feed_events_live_stream = подій · живий стрім

## Глобальні змінні - заголовок / фільтри

globals_breadcrumb_automation = Автоматизація
globals_breadcrumb_globals = Глобальні
globals_filter_all = Всі
globals_filter_persisted = Збережені
globals_filter_session = Сесійні
globals_search_placeholder = Шукати змінні...
globals_export_btn = Експорт JSON
globals_new_btn = + Нова змінна
globals_loading = Завантаження...
globals_empty_title = Глобальних змінних немає
globals_empty_desc = Змініть фільтр або пошук, або створіть нову за допомогою + Нова змінна.
globals_edit_action = Редагувати значення
globals_delete_action = Видалити
globals_deleted_toast = Видалено '{ $name }'
globals_breadcrumb = Глобальні змінні
globals_stat_total = усього
globals_stat_persisted = збережено
globals_stat_in_memory = у пам'яті
globals_empty_caption = Немає змінних за цим фільтром.
globals_col_modified = ЗМІНЕНО
globals_col_reads_writes = ЧИТАНЬ · ЗАПИСІВ
globals_col_persist = ЗБЕРІГАТИ
globals_col_actions = ДІЇ
globals_rename_taken = Назву '{ $name }' вже зайнято
globals_menu_rename = Перейменувати
globals_menu_persist = Зберігати
globals_menu_session_only = Лише сесія
globals_toast_error = Глобальні змінні: { $message }

## Глобальні змінні - редактор значень

globals_editor_title_create = Нова змінна
globals_editor_title_edit = Редагувати змінну
globals_editor_section_name = НАЗВА
globals_editor_section_type = ТИП
globals_editor_type_locked_hint = Тип фіксується після створення і тут не змінюється
globals_editor_section_persistence = ЗБЕРЕЖЕННЯ
globals_editor_section_value = ЗНАЧЕННЯ
globals_editor_persist_label = Зберігати після перезапуску
globals_editor_persist_desc = Збережені глобальні виживають після закриття; сесійні скидаються при запуску
globals_editor_cancel = Скасувати
globals_editor_save = Зберегти
globals_editor_saving = Збереження...
globals_editor_kbd_hint = ⌘ Enter - зберегти
globals_editor_name_placeholder = my_variable
globals_error_invalid_int = Некоректне ціле число
globals_error_invalid_float = Некоректне число з рухомою комою
globals_error_invalid_datetime = Некоректна дата й час ISO 8601 (напр. 2026-05-18T14:23:00Z)
globals_error_invalid_json_array = Некоректний масив JSON
globals_error_invalid_json_object = Некоректний об'єкт JSON
globals_error_name_required = Потрібно вказати назву
globals_error_name_taken = Глобальна змінна з такою назвою вже існує
globals_delete_confirm_title = Видалити глобальну змінну
globals_delete_confirm_body = Це назавжди видалить змінну та її значення.

## Глобальні - інспектор значення

globals_inspect_subtitle_items = { $kind } · { $count } елем. · лише читання
globals_inspect_subtitle_keys = { $kind } · { $count } ключів · лише читання
globals_inspect_snapshot = Знімок поточного значення · оновлюється при наступному читанні
globals_inspect_close = Закрити
globals_inspect_edit = Редагувати значення

## Редактор скриптів - сторінка / панель інструментів

script_editor_breadcrumb = Редактор скриптів
script_editor_breadcrumb_automation = Автоматизація
script_editor_edited_prefix = змінено
script_editor_run = Тестовий запуск
script_editor_save = Зберегти
script_editor_format = Форматувати
script_editor_api_docs = Документація API
script_editor_debug = Відлагодження
script_editor_debug_tip = Відлагоджувач планується після версії 1.0
script_editor_output_header = Виведення
script_editor_output_clear = Очистити
script_editor_api_reference = Довідка API
script_editor_scripts_label = СКРИПТИ
script_editor_search_placeholder = Пошук скриптів…
script_editor_new_script = Новий скрипт
script_editor_no_scripts = Скриптів ще немає
script_editor_group_action = Скрипти дій
script_editor_group_standalone = Окремі
script_editor_manual_run = ручний запуск
script_editor_rename_action = Перейменувати
script_editor_enable_action = Увімкнути
script_editor_disable_action = Вимкнути
script_editor_delete_action = Видалити
script_editor_new_btn = + Новий
script_editor_empty_title = Виберіть скрипт або натисніть + Новий
script_editor_empty_desc = Скрипти дозволяють запускати код rhai з будь-якої дії.
script_editor_running = Виконується…
script_editor_run_modal_cancel = Скасувати
script_editor_save_blocked = Збереження заблоковано - спершу виправте синтаксичні помилки
script_editor_discard_title = Відхилити незбережені зміни?
script_editor_discard_body = У цьому скрипті є незбережені зміни. Продовжити й втратити їх або залишитися для редагування.
script_editor_discard_confirm = Відхилити
script_editor_discard_cancel = Продовжити редагування
script_editor_discard_esc_hint = щоб продовжити редагування
script_editor_shared = Спільні
script_editor_sandbox_label = Пісочниця:
script_editor_sandbox_enabled = увімкнено
script_editor_problems_tab = Проблеми
script_editor_console_cleared = Консоль очищено.
script_editor_no_problems = Проблем немає.
script_editor_rename_placeholder = Назва скрипта

## Редактор скриптів - модальне вікно запуску

script_editor_health = { $ok }/{ $total } справні
script_editor_type_check_passed = Перевірку типів пройдено
script_editor_type_check_errors = { $count ->
    [one] { $count } помилка
    [few] { $count } помилки
    [many] { $count } помилок
   *[other] { $count } помилок
}
script_editor_run_modal_title = Запустити { $name }
script_editor_run_modal_title_generic = Запустити скрипт
script_editor_run_input_placeholder = Введіть значення { $label }…
script_editor_run_input_error = Введіть значення для { $name }

## Action telemetry - stat column headers

telemetry_stat_last_fired = ОСТАННЯ ДІЯ
telemetry_stat_runs_today = ЗАПУСКИ · СЬОГОДНІ
telemetry_stat_avg_time = СЕР. ЧАС
telemetry_stat_errors_7d = ПОМИЛКИ · 7Д

## Action editor - validation errors

action_editor_error_message_required = Повідомлення є обов'язковим.
action_editor_error_var_required = Назва змінної є обов'язковою.
action_editor_error_delay_invalid = Кількість мілісекунд має бути невід'ємним цілим числом.
action_editor_error_log_required = Текст журналу є обов'язковим.
action_editor_error_clip_required = Оберіть кліп для відтворення.
action_editor_error_speak_required = Текст для озвучення є обов'язковим.
action_editor_error_file_required = Шлях та цільова змінна є обов'язковими.
action_editor_error_random_invalid = min, max (min ≤ max) та цільова змінна є обов'язковими.
action_editor_pill_custom = Власний
action_editor_pill_default = Типовий

## Integration detail - OBS / quick-action

builtin_quick_action_fallback = Швидка дія
builtin_obs_not_connected = OBS не підключено
builtin_obs_not_supported = Не підтримується для OBS
builtin_disconnect_confirm_hint = Вас буде відключено, і потрібно буде підключитися повторно вручну. Живі події від цієї інтеграції перестануть надходити до цього моменту.
integration_disconnect_title = Відключити інтеграцію
integration_settings_coming_soon = Налаштування зʼявляться згодом
integration_quick_action_na = Н/Д
integration_state_connecting_title = Підключення…
integration_state_connecting_detail = Встановлення сеансу з цією інтеграцією.
integration_state_reconnecting_title = Повторне підключення…
integration_state_reconnecting_detail = Сеанс розірвано; forge відновлює його.
integration_state_disconnected_detail = Натисніть «Підключитися» вгорі, щоб зв'язати цю інтеграцію.

## OAuth / authentication errors

auth_error_credentials_missing_youtube = Облікові дані OAuth для YouTube не налаштовані
auth_error_credentials_missing_kick = Облікові дані OAuth для Kick не налаштовані
auth_error_flow_consumed = OAuth-потік уже використано
auth_error_unknown = Невідома помилка

## Widget - key capture

widget_key_capture_placeholder = Натисніть комбінацію…

## Widget - event inspector

widget_event_replay = Відтворити цю подію
widget_event_replaying = Відтворення…
widget_event_payload_header = ДАНІ
widget_event_caused_header = ПРИЧИНА

## Widget - chat row

widget_chat_subscribed = підписався (Рівень { $tier })
widget_chat_cheered = зробив cheer
widget_chat_raiding_with = рейдить з
widget_chat_viewers = { $viewers } глядачів
widget_chat_triggered = Спрацювало: { $action }

## Live chat - event descriptors

chat_event_subscribed = підписався (Рівень { $tier })
chat_event_raided = рейдить з
chat_event_cheered = чірив
chat_event_viewers = { $viewers } глядачів
chat_event_super_chat = надіслав Super Chat ({ $amount } { $currency })
chat_event_new_member = став учасником
chat_event_member_milestone = ювілей участі

## Widget - builtin header actions

widget_header_action_reconnect = Підключитися
widget_header_action_refresh_token = Оновити токен
widget_header_action_disconnect = Відключитися
widget_header_action_settings = Налаштування
widget_header_uptime = час роботи { $duration }
widget_header_uptime_only = час роботи { $duration }
widget_header_capability_limited = Обмежено

## Widget - builtin content

widget_builtin_stream_health = ЯКІСТЬ СТРІМУ
widget_builtin_active_badge = АКТИВНО
widget_builtin_live_badge = НАЖИВО
widget_builtin_active_count = { $count } активних
widget_builtin_event_count =
    { $count ->
        [one] { $count } подія
        [few] { $count } події
       *[other] { $count } подій
    }

## Widget - server file list

widget_file_list_header = Коренева директорія оверлею
widget_file_list_path_label = ШЛЯХ
widget_file_list_files_label = ФАЙЛИ
widget_file_list_url_label = URL ДЖЕРЕЛА В БРАУЗЕРІ
widget_file_list_dir_count =
    { $count ->
        [one] { $count } файл
        [few] { $count } файли
       *[other] { $count } файлів
    }


## Widget - server confirm modal

widget_confirm_what_this_means = ЩО ЦЕ ОЗНАЧАЄ
widget_confirm_type_prefix = Введіть
widget_confirm_type_suffix = для підтвердження:
widget_confirm_esc_to_cancel = щоб скасувати
widget_confirm_cancel = Скасувати

## Widget - destructive confirm modal

widget_confirm_delete_title = Видалити { $kind }?
widget_confirm_delete_hint = Цей елемент буде остаточно видалено. Дію не можна скасувати.
widget_confirm_delete_kind_action = дію
widget_confirm_delete_kind_step = крок
widget_confirm_delete_kind_trigger_link = прив'язку тригера
widget_confirm_delete_kind_global = глобальну змінну
widget_confirm_delete_kind_script = скрипт
widget_confirm_delete_kind_client = клієнта

## Widget - server bearer token

widget_bearer_copy = КОПІЮВАТИ
widget_bearer_regenerate = ПЕРЕГЕНЕРУВАТИ
widget_bearer_regen_warning = Перегенерація відключить всіх клієнтів
widget_bearer_regen_warning_body = Підключені WebSocket-клієнти мають перепідключитися з новим токеном.

## Widget - server bind card

widget_bind_badge_recommended = Рекомендовано
widget_bind_badge_requires_confirmation = Потребує підтвердження

## Widget - picker modal

widget_picker_search_placeholder = Пошук…
widget_picker_loading = Завантаження…
widget_picker_no_results = Нічого не знайдено.

## Widget - output device picker

widget_device_default_suffix = (за замовчуванням)
widget_device_test = Тест

## Widget - quick actions panel

widget_quick_actions_title = Швидкі дії

## Widget - console (script output)

widget_console_no_output = Вихідних даних ще немає

## Settings - audio output

settings_audio_scanning = Сканування пристроїв…
settings_audio_title = Аудіо
settings_audio_output_devices = ПРИСТРОЇ ВИВОДУ
settings_audio_test_section = ТЕСТ
settings_audio_test_tone = Відтворити тестовий тон 440 Гц
settings_audio_test_playing = Відтворення…
settings_audio_test_error = Помилка тестового тону: { $error }
settings_audio_persist_error = Не вдалося зберегти вибір пристрою: { $error }

## Script editor - API docs panel

script_editor_api_no_matches = Збігів не знайдено
script_editor_api_search_placeholder = Пошук модулів…

## Script editor - details panel

script_editor_details_heading = ДЕТАЛІ
script_editor_signature_heading = СИГНАТУРА
script_editor_details_type = Тип
script_editor_details_linked = Пов'язано з
script_editor_type_action = Скрипти дій
script_editor_type_standalone = Окремі
script_editor_open_action = Відкрити дію
script_editor_details_lines = Рядки
script_editor_details_edited = Змінено
script_editor_details_returns = повертає
script_editor_run_stats_heading = СТАТИСТИКА
script_editor_stat_runs = ЗАПУСКИ
script_editor_stat_avg = СЕРЕДНЄ
script_editor_stat_runs_value = { $n } сьогодні
script_editor_stat_avg_value = { $n } мс

## Widget - layout chrome

widget_layout_app_name = Forge
widget_layout_footer_app = forge
widget_layout_connected = { $connected }/{ $total } підключено
widget_layout_uptime_suffix = час роботи

## Widget - volume slider

widget_volume_label = ГУЧН

## Locale-aware formatting - feed time

fmt_feed_time_pattern = %HH%:%MM%:%SS%.%mmm%

## Locale-aware formatting - month abbreviations (uk, genitive short form for date display)

fmt_month_abbr_01 = січ.
fmt_month_abbr_02 = лют.
fmt_month_abbr_03 = бер.
fmt_month_abbr_04 = квіт.
fmt_month_abbr_05 = трав.
fmt_month_abbr_06 = черв.
fmt_month_abbr_07 = лип.
fmt_month_abbr_08 = серп.
fmt_month_abbr_09 = вер.
fmt_month_abbr_10 = жовт.
fmt_month_abbr_11 = лист.
fmt_month_abbr_12 = груд.

## Locale-aware formatting - relative time (uk four-form plural: one/few/many/other)

fmt_relative_never = ніколи
fmt_relative_seconds = { $count ->
    [one] { $count } с тому
    [few] { $count } с тому
    [many] { $count } с тому
   *[other] { $count } с тому
}
fmt_relative_minutes = { $count ->
    [one] { $count } хв тому
    [few] { $count } хв тому
    [many] { $count } хв тому
   *[other] { $count } хв тому
}
fmt_relative_hours = { $count ->
    [one] { $count } год тому
    [few] { $count } год тому
    [many] { $count } год тому
   *[other] { $count } год тому
}
fmt_relative_days = { $count ->
    [one] { $count } д тому
    [few] { $count } д тому
    [many] { $count } д тому
   *[other] { $count } д тому
}

## Storage error screen
storage_error_title = Не вдалося відкрити базу даних
storage_error_data_safe = Ваші дані на диску не було змінено. Застосунок працює на тимчасовому сховищі, тож зміни, зроблені зараз, буде втрачено після перезапуску.
storage_error_report = Про цю помилку варто повідомити.

## Integration seed
iseed_metric_chat = Чат
iseed_metric_messages = Повідомлення
iseed_metric_eventsub = EventSub
iseed_metric_api_budget = Бюджет API
iseed_metric_websocket = WebSocket
iseed_metric_streaming = Трансляція
iseed_metric_mode = Режим
iseed_metric_activity = Активність
iseed_metric_session = Сеанс
iseed_metric_detail = Деталі
iseed_scenes = Сцени
iseed_sources = Джерела
iseed_dropped = Втрачено
iseed_channel = Канал
iseed_status = Статус
iseed_stat_bitrate = Бітрейт
iseed_stat_fps = FPS
iseed_field_viewers = Глядачі
iseed_field_category = Категорія
iseed_field_uptime = Час роботи
iseed_field_latency = Затримка
iseed_field_since = Відколи
iseed_section_eventsub_subs = Підписки EventSub
iseed_section_oauth_scopes = Дозволи OAuth
iseed_section_live_broadcast = Пряма трансляція
iseed_section_stream_stats = Статистика трансляції
iseed_section_overview = Огляд
iseed_section_details = Подробиці
iseed_cta_manage_subscriptions = Керувати підписками
iseed_action_run_ad = Запустити рекламу
iseed_action_create_clip = Створити кліп
iseed_action_commercial = Рекламна пауза
iseed_action_shoutout = Shoutout
iseed_action_switch_scene = Змінити сцену
iseed_action_toggle_source = Перемкнути джерело
iseed_action_record = Запис
iseed_action_toggle_mute = Перемкнути звук
iseed_action_send_message = Надіслати повідомлення
iseed_action_clear_chat = Очистити чат
iseed_action_slow_mode = Повільний режим
iseed_action_ban_user = Заблокувати користувача
iseed_kick_capability = Гібридний транспорт
iseed_kick_banner_title = Гібридний транспорт чату
iseed_kick_banner_body = Отримання чату йде через спільнотний Pusher WebSocket; запис - через офіційний API.
iseed_generic_connect_hint = Підключіться, щоб бачити стан у реальному часі
