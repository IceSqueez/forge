## Boot - splash and data-open failure screens

boot_starting = Запуск...
boot_upgrade_title = Потрібне оновлення
boot_upgrade_body = Ваші дані forge використовують схему версії { $found }, новішу за версію цієї збірки { $expected }. Оновіть forge до останнього випуску, щоб відкрити їх.
boot_upgrade_reassure = Ваші дані у безпеці й недоторкані.
boot_retry = Повторити
boot_failure_title = Не вдалося відкрити ваші дані
boot_failure_reassure = Ваші дані у безпеці. Якщо це повторюється, повідомте про проблему.
boot_pre_baseline_title = Спершу відкрийте дані в ранішій версії
boot_pre_baseline_body = Цю теку даних востаннє використовував forge старіший за { $release } (версія схеми бази { $found }). Відкрийте її один раз у forge { $release }, а потім запустіть цю версію знову.
boot_pre_baseline_reassure = Нічого не змінено. Ваші дані недоторкані.
boot_pre_baseline_open_folder = Відкрити теку даних
boot_pre_baseline_quit = Вийти

## Загальні дії, спільні для всіх екранів

common_cancel = Скасувати
common_save = Зберегти
common_copy = Копіювати

## Навігація - назви екранів (хлібні крихти + бічна панель)

nav_home = Головна
nav_event_feed = Стрічка подій
nav_script_editor = Скрипти

## Навігація - заголовки секцій бічної панелі

nav_section_audience = АУДИТОРІЯ
nav_section_automation = АВТОМАТИЗАЦІЯ

## Навігація - підписи пунктів бічної панелі

nav_item_home = Головна
nav_item_chat = Чат
nav_item_actions = Дії
nav_item_triggers = Тригери
nav_item_queues = Черги
nav_item_event_feed = Стрічка подій
nav_item_globals = Глобальні змінні
nav_item_soundboard = Звукова панель
nav_item_tts = Синтез мовлення
nav_item_overlays = Оверлеї
nav_item_ws_server = WebSocket-сервер
nav_item_hotkey = Гарячі клавіші
nav_item_settings = Налаштування

## Головна - секція привітання

home_hero_tagline = Відкрита автоматизація стрімів, створена для стрімерів
home_hero_import = Імпортувати
home_hero_new_action = Нова дія
home_import_success = Імпортовано дію «{ $name }»
home_import_failed = Не вдалося імпортувати: { $error }

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

## Головна - картка втрат подій і значок у підвалі

event_loss_title = Втрати подій
event_loss_scope_hint = від запуску · скидається після перезапуску
event_loss_none = Від запуску жодної події не втрачено
event_loss_footer_badge = { $count } втрачено
event_loss_tooltip_title = Події, втрачені від запуску
event_loss_tooltip_row = { $consumer } ({ $tier }) - { $detail }
stay_awake_footer_label = Без сну
stay_awake_tooltip_both = Без сну: екран і система не засинають
stay_awake_tooltip_display_only = Без сну: екран не засинає
stay_awake_tooltip_system_only = Без сну: система не засинає
stay_awake_tooltip_starting = Без сну: запуск
stay_awake_tooltip_inactive = Без сну: не активно
stay_awake_tooltip_display_refused = Екран: недоступно - { $reason }
stay_awake_tooltip_system_refused = Система: недоступно - { $reason }
event_loss_tier_critical = без втрат
event_loss_tier_observer = відображення
event_loss_part_priority = { $count ->
    [one] { $count } пріоритетна відкинута
    [few] { $count } пріоритетні відкинуто
    [many] { $count } пріоритетних відкинуто
   *[other] { $count } пріоритетних відкинуто
}
event_loss_part_bulk = { $count ->
    [one] { $count } масова відкинута
    [few] { $count } масові відкинуто
    [many] { $count } масових відкинуто
   *[other] { $count } масових відкинуто
}
event_loss_part_skipped = { $count ->
    [one] { $count } пропущена
    [few] { $count } пропущені
    [many] { $count } пропущених
   *[other] { $count } пропущених
}
event_loss_part_unwritten = { $count ->
    [one] { $count } не збережена
    [few] { $count } не збережені
    [many] { $count } не збережених
   *[other] { $count } не збережених
}
event_loss_consumer_trigger_evaluator = Перевірка тригерів
event_loss_consumer_event_log = Журнал подій
event_loss_consumer_chat_history = Історія чату
event_loss_consumer_viewer_tracker = Облік глядачів
event_loss_consumer_latest_values = Останні значення
event_loss_consumer_latest_overlays = Оверлеї останніх значень
event_loss_consumer_run_history = Історія запусків
event_loss_consumer_event_trail = Слід подій
event_loss_consumer_observer = Інші вікна
event_loss_consumer_ui_events = Вікна застосунку
event_loss_consumer_ui_chat_feed = Стрічка чату
event_loss_consumer_ui_chat_display = Показ стрічки чату

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
settings_appearance_density_label = Щільність інтерфейсу
settings_appearance_density_subtitle = Скільки простору отримує інтерфейс - застосовується миттєво
settings_appearance_density_compact = Компактна
settings_appearance_density_compact_hint = Щільніше розташування, більше рядків на екрані
settings_appearance_density_cozy = Затишна
settings_appearance_density_cozy_hint = Збалансовані відступи (типово)
settings_appearance_density_spacious = Простора
settings_appearance_density_spacious_hint = Більше повітря між елементами
settings_theme_persist_failed = Не вдалося зберегти тему
settings_density_persist_failed = Не вдалося зберегти щільність інтерфейсу
settings_check_updates_failed = Не вдалося відкрити сторінку релізів
settings_appearance_fonts_label = Шрифти
settings_appearance_theme_hint = Який вигляд матиме Forge
settings_appearance_font_interface = ІНТЕРФЕЙС
settings_appearance_font_monospace = МОНОШИРИННИЙ
settings_appearance_font_search = Пошук встановлених шрифтів
settings_appearance_font_default_body = Типовий (Inter)
settings_appearance_font_default_mono = Типовий (JetBrains Mono)
settings_appearance_font_persist_failed = Не вдалося зберегти шрифт
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
settings_nav_general = Загальні
settings_stay_awake_label = Не засинати, поки forge працює
settings_stay_awake_hint = Екран і система не переходять у сон, поки forge працює
settings_nav_language_region = Мова та регіон
settings_nav_shortcuts = Скорочення
settings_nav_audio = Аудіо

## Налаштування → панель діагностики

settings_diagnostics_section_title = Журнали та діагностика
settings_diagnostics_subtitle = Стеж за журналами Forge та експортуй їх для підтримки
settings_diagnostics_open_log_dir = Відкрити теку журналів
settings_diagnostics_export_bundle = Експортувати діагностичний пакет
settings_diagnostics_clear_logs = Очистити журнали
settings_diagnostics_tail_empty = Записів журналу ще немає.
settings_diagnostics_clear_confirm_title = Очистити журнали?
settings_diagnostics_clear_confirm_body = Архівні файли журналів буде видалено, а сьогоднішній - очищено. Це незворотно.
settings_diagnostics_clear_confirm_action = Очистити журнали
settings_diagnostics_cleared = Журнали очищено
settings_diagnostics_clear_failed = Не вдалося очистити журнали: { $error }
settings_diagnostics_exported = Діагностичний пакет збережено до { $path }
settings_diagnostics_export_failed = Не вдалося експортувати діагностичний пакет: { $error }
settings_diagnostics_level_label = Рівень журналювання
settings_diagnostics_level_hint = Застосовується одразу, без перезапуску. Debug і trace швидко нарощують журнал на активному стрімі.
settings_diagnostics_level_env_locked = Цим запуском керує змінна середовища RUST_LOG, тож рівень тут змінити не можна.
settings_diagnostics_level_env_hatch = До завантаження налаштувань журналювання йде на типовому рівні, тож перші миті запуску ніколи не потраплять у журнал на підвищеному рівні. Щоб зловити помилку старту, запусти Forge зі змінною середовища RUST_LOG=debug (або trace).
settings_diagnostics_level_targets = Відтворення на trace лишиться читабельним, якщо підняти рівень одному компоненту, а не всьому: RUST_LOG=info,forge::event=debug для ланцюжка подій, forge::trigger=debug щоб побачити, чому тригер спрацював або ні, forge::command=trace для повного рядка команди разом з тим, що набрали глядачі.
settings_diagnostics_level_apply_failed = Не вдалося перемкнути рівень журналювання для цього запуску.
settings_diagnostics_export_preview_title = Прочитай це перед експортом
settings_diagnostics_export_preview_lead = Цей файл створено, щоб прикріпити його до публічного issue. Відкрий і прочитай його, перш ніж десь публікувати.
settings_diagnostics_export_preview_includes = У ньому є версія Forge, твоя ОС і архітектура, які інтеграції встановлено та чи вони підключені, твої налаштування, останні { $runs } запусків дій і сам журнал.
settings_diagnostics_export_preview_excludes = У ньому немає токенів і ключів доступу, назв акаунтів і каналів, адрес та шляхів до файлів, а також повідомлень з чату. Приховані значення виглядають як { $stamp } - це навмисно, а не обрізане читання.
settings_diagnostics_export_preview_lengths = Налаштування та запуски дій несуть довжини і типи замість значень, тож із них неможливо відновити те, що було набрано.
settings_diagnostics_export_preview_script = { $count } рядків у цьому файлі написав крок журналювання у твоїх власних діях і скриптах. Forge не обирав цей текст - він винесений в окремий розділ, щоб ти міг його прочитати.
settings_diagnostics_export_preview_script_none = У цьому файлі немає рядків від кроку журналювання з твоїх власних дій і скриптів.
settings_diagnostics_export_preview_trace = Журналювання йшло на рівні trace: у цьому файлі є { $count } повних рядків команд разом з усім, що глядачі набрали як аргументи до твоїх команд. Прочитай їх перед публікацією.
settings_diagnostics_export_preview_trace_none = Рівень журналювання - trace. Жодна команда ще не збіглася, але на trace кожна збіжна команда потрапляє в журнал повністю, разом з усім, що глядачі набирають як аргументи.
settings_diagnostics_export_preview_size = Приблизно { $size } КБ.
settings_diagnostics_export_preview_elided = { $size } КБ старіших рядків журналу не вмістилися в обмеження розміру і не потрапили у файл; збережено найновіші рядки.
settings_diagnostics_export_preview_confirm = Обрати файл і експортувати
settings_diagnostics_level_trace = Trace
settings_diagnostics_level_debug = Debug
settings_diagnostics_level_info = Info
settings_diagnostics_level_warn = Warn
settings_diagnostics_level_error = Error

## Налаштування → панель версії

settings_version_title = Версія та оновлення
settings_version_license = Відкритий код · MIT OR Apache-2.0
settings_version_check_updates = Перевірити оновлення
settings_version_notify_label = Сповіщати про нові версії
settings_version_notify_hint = Показує одноразове сповіщення, коли виходить новіший реліз. Ніколи під час трансляції чи запису в OBS.

## Сповіщення про оновлення

update_toast_available = Доступна forge { $version }
update_toast_open_action = Відкрити реліз

## Налаштування → панель сховища

settings_storage_section_title = Сховище та резервні копії
settings_storage_db_path_label = База даних
settings_storage_backup_btn = Резервна копія зараз
settings_storage_backup_hint = Зберігає всю теку даних - базу, ключ облікових даних, медіа, оверлеї, голоси й ресурси - одним архівом у теці резервних копій. Архів містить ключ, що розшифровує збережені входи: тримайте його в таємниці.
settings_storage_backup_btn_busy = Створюю копію...
settings_storage_backup_done = Копію збережено: { $path }
settings_storage_backup_failed = Не вдалося створити копію: { $error }
settings_storage_backup_reveal = Показати
credentials_key_loss_toast =
    { $count ->
        [one] Ключ облікових даних зник - { $count } збережений вхід неможливо прочитати. Увійдіть знову на екранах платформ.
        [few] Ключ облікових даних зник - { $count } збережені входи неможливо прочитати. Увійдіть знову на екранах платформ.
        [many] Ключ облікових даних зник - { $count } збережених входів неможливо прочитати. Увійдіть знову на екранах платформ.
       *[other] Ключ облікових даних зник - { $count } збережених входів неможливо прочитати. Увійдіть знову на екранах платформ.
    }
settings_storage_per_viewer_limit_label = Історія чату на глядача
settings_storage_per_viewer_limit_hint = Скільки останніх повідомлень кожного глядача зберігати в базі даних.
settings_storage_per_viewer_limit_custom = Своє
settings_storage_per_viewer_limit_unlimited = Без ліміту
settings_storage_per_viewer_limit_unlimited_hint = База даних росте без обмежень.
settings_storage_display_limit_label = Показувати при відкритті чату
settings_storage_display_limit_hint = Скільки останніх повідомлень показує чат; старіші зникають. Діє після перезапуску.
settings_storage_retention_label = Зберігання журналу подій
settings_storage_retention_hint = Скільки днів зберігати журнал подій у базі даних.
settings_storage_history_retention_label = Зберігання історії дій
settings_storage_history_retention_hint = Скільки днів зберігати історію запусків дій у базі даних - щонайменше { $min }.

## Налаштування → панель черг

settings_queues_section_title = Черги та потоки
settings_queues_workers_label = Робочі потоки
settings_queues_managed_hint = Ліміти паралелізму та прапорці блокування керуються на екрані Черги.

## Налаштування → панель гарячих клавіш (ярлики)

settings_shortcuts_title = Клавіатурні скорочення
settings_shortcuts_subtitle = Ці скорочення працюють лише тоді, коли вікно forge у фокусі. Системні комбінації - у розділі «Хоткеї».
settings_shortcuts_action_nav_home = Перейти на Хаб
settings_shortcuts_action_nav_live_chat = Відкрити живий чат
settings_shortcuts_action_nav_actions = Відкрити дії
settings_shortcuts_action_nav_triggers = Відкрити тригери
settings_shortcuts_action_nav_twitch = Відкрити Twitch
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
settings_hold_ceiling_section = ГЛОБАЛЬНІ ГАРЯЧІ КЛАВІШІ
settings_hold_ceiling_label = Закривати зависле утримання через
settings_hold_ceiling_placeholder = секунди
settings_hold_ceiling_unit = секунд
settings_hold_ceiling_off = Ніколи не закривати автоматично
settings_hold_ceiling_hint = Якщо система проковтне відпускання клавіші, утримання відпустить себе саме через цей час. Вимкнено - утримання лишається відкритим до справжнього відпускання.
settings_hold_ceiling_invalid = Введи ціле число секунд від { $min } до { $max }.
settings_shortcuts_error_needs_modifier = Поєднайте клавішу з Ctrl, Alt чи Meta або оберіть F-клавішу - звичайні клавіші заважали б набору тексту.
settings_shortcuts_conflict_title = Скорочення вже призначено
settings_shortcuts_conflict_body = { $chord } зараз призначено дії «{ $owner }». Перепризначити? Попереднє скорочення стане непризначеним.
settings_shortcuts_conflict_steal = Перепризначити

## Налаштування → панель WebSocket

settings_ws_title = WebSocket-сервер
settings_ws_subtitle = Налаштуйте підключення оверлеїв та сторонніх інструментів до Forge.
settings_ws_enable_label = Увімкнути сервер
settings_ws_enable_description = Запускається під час старту застосунку, хостить оверлеї, приймає WS-клієнти
settings_ws_bind_section_title = Адреса прив'язки
settings_ws_bind_section_subtitle = Інтерфейс, який слухає сервер
settings_ws_bind_localhost_title = Тільки локальний хост
settings_ws_bind_localhost_description = Підключатися можуть лише застосунки на цьому компʼютері. Browser sources в OBS та локальні плагіни Stream Deck працюють як зазвичай. Безпечний типовий варіант.
settings_ws_bind_lan_title = Усі інтерфейси (LAN)
settings_ws_bind_lan_description = Дозволяє іншим пристроям у вашій мережі (телефон, планшет, другий ПК) підключатися до Forge. Відкриває сервер для будь-кого в тій самій Wi-Fi чи LAN.
settings_ws_badge_recommended = Рекомендовано
settings_ws_badge_requires_confirmation = Потребує підтвердження
settings_ws_origins_section_title = Додаткові дозволені походження
settings_ws_origins_subtitle = По одному в рядку. Сторінка, яку роздає сам Forge, приймається автоматично, зокрема браузерне джерело, спрямоване на IP вашого компʼютера. Потрібно лише для сторінки з іншого джерела - з іншого хоста або за доменним іменем, що веде до Forge через проксі. Без збігу WebSocket-підключення відхиляється.
settings_ws_origins_placeholder = http://192.168.1.5:8081
settings_ws_origins_apply_btn = Застосувати
settings_ws_origins_invalid = Некоректне походження (схема + хост, порт необовʼязковий):
settings_ws_port_section_title = Порт
settings_ws_port_subtitle = За замовчуванням 8081 · діапазон 1024-65535
settings_ws_token_section_title = Bearer-токен
settings_ws_token_clients_send = Клієнти передають його в
settings_ws_auth_section_title = Автентифікація
settings_ws_auth_section_subtitle = Які клієнти мають автентифікуватися
settings_ws_auth_require_ws_label = Вимагати токен для WebSocket-клієнтів
settings_ws_auth_require_ws_sublabel = Відхиляти WS-підключення без дійсного bearer-токена
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

## Дії - заголовок сторінки / хлібні крихти

actions_breadcrumb_automation = Автоматизація
actions_breadcrumb_actions = Дії
actions_filter_all = Всі
actions_filter_chat = Чат
actions_filter_timers = Таймери
actions_filter_points = Поінти
actions_search_placeholder = Пошук дій...
actions_stat_actions = дій
actions_stat_enabled = увімкнено
actions_stat_disabled = вимкнено
actions_empty = Дій ще немає

## Дії - панель деталей

actions_detail_empty_title = Дію не обрано
actions_detail_empty_hint = Оберіть дію зі списку, щоб переглянути деталі.

## Дії - контекстне меню

actions_menu_rename = Перейменувати...
actions_menu_duplicate = Дублювати
actions_menu_enable = Увімкнути
actions_menu_disable = Вимкнути
actions_menu_delete = Видалити...

## Дії - нижній рядок

## Дії - підказка ESC


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
sub_cat_overlay = Оверлеї
sub_cat_util = Утиліти

## Дії - секції конфігурації кроку

## Дії - вибір тригера (бічна панель)

## Дії - назви категорій тригерів (заголовки секцій)

## Дії - підписи типів тригерів

## Дії - опис типів тригерів

## Редактор дій - хлібні крихти / дерево / деталі

action_editor_loading = Завантаження дії...
action_editor_no_description = Без опису
action_editor_test_run = Тестовий запуск
action_editor_duplicate = Дублювати
action_editor_export = Експорт JSON
action_editor_export_done = Дію експортовано до { $path }
action_editor_export_failed = Не вдалося експортувати: { $error }
action_editor_edit = Редагувати
action_editor_menu_delete = Видалити дію
action_editor_add_trigger = Додати тригер
action_editor_add_step = Додати крок
action_editor_health_unknown_var = Використовує %{ $name }%, якої не надає жоден тригер і не створює жоден попередній крок
action_editor_health_produced_later = Використовує %{ $name }%, але її створює лише пізніший крок
action_editor_health_isolated_sibling = Використовує %{ $name }%, яку створює сусідній крок, що виконується ізольовано й не ділиться нею
action_editor_health_some_triggers = Використовує %{ $name }%, яку надають лише деякі тригери цієї дії
action_editor_health_last_run_failed = Останній запуск завершився помилкою: { $message }
action_editor_health_control_flow_concurrent = Переривання, продовження та зупинка не мають ефекту, якщо дія виконує свої кроки паралельно. Переведіть дію в послідовний режим або видаліть цей крок.
action_editor_health_disabled = Інтеграцію вимкнено
action_editor_health_integration_disabled = { $name } вимкнено
action_editor_step_disabled_prefix = { $name } вимкнено - цей крок завершується з причиною{" "}
action_editor_health_failed = Інтеграція не запустилася
action_editor_health_integration_failed = { $name } не вдалося запустити
action_editor_step_failed_notice = { $name } не вдалося запустити - цей крок завершуватиметься помилкою, доки інтеграція не запрацює
action_editor_health_ok = Все гаразд: усі посилання резолвляться, останній запуск успішний
action_editor_health_warn = Статичне попередження
action_editor_health_error = Помилка
action_editor_overlay_order_warning = Рядки оверлею можуть надійти не за порядком
action_editor_overlay_order_warning_hint = Ця дія надсилає вміст в оверлей, чутливий до порядку, з черги, яка виконує кілька дій одночасно. Черга з паралельністю 1 зберігає порядок.
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
action_editor_section_triggers_count = ТРИГЕРИ · { $count }
action_editor_sub_actions_count = { $count } підпунктів
actions_sub_file_browse = Огляд
actions_sub_files_root_label = Тека файлів
actions_sub_files_root_hint = Кроки з файлами приймають лише шляхи відносно цієї теки - абсолютні шляхи та ".." відхиляються.
actions_sub_files_root_open = Відкрити теку
actions_sub_files_root_create_failed = Не вдалося створити теку файлів: { $reason }
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

action_editor_run_history = Історія запусків...
action_editor_run_history_title = Історія запусків
action_editor_run_history_loading = Завантаження історії запусків...
action_editor_run_history_failed = Не вдалося завантажити історію запусків: { $error }
action_editor_run_history_empty_title = Запусків ще немає
action_editor_run_history_empty_hint = Ця дія ще не виконувалася
action_editor_run_history_duration_ms = { $count } мс
action_editor_run_history_outcome_success = Успіх
action_editor_run_history_outcome_failed = Помилка
action_editor_run_history_outcome_cancelled = Скасовано
action_editor_run_history_step_ok = ok
action_editor_run_history_step_failed = помилка
action_editor_run_history_integration_disabled = інтеграцію вимкнено · { $name }
action_editor_run_history_open_integration = Відкрити { $name }
action_editor_run_history_step_skipped = пропущено
action_editor_run_history_step_nested = ↳
action_editor_run_history_trigger_fallback = Тригер
action_editor_run_history_scheduled_by = Заплановано дією { $name }
action_editor_run_history_scheduled_by_removed = Заплановано видаленою дією
action_editor_run_history_scheduled = Запланований запуск
action_editor_run_history_step_args_in = @in
action_editor_run_history_step_produced = @out

## Редактор дій - меню кроку

action_editor_step_menu_edit = Редагувати крок...
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
action_editor_test_run_status_running = виконується...
action_editor_test_run_status_failed = помилка
action_editor_test_run_status_skipped = пропущено
action_editor_test_run_status_ms = { $ms } мс
action_editor_test_run_failed_banner = Збій на кроці { $step } · { $name }
action_editor_test_run_completed = Виконано кроків: { $count } · помилок: { $errors }
action_editor_test_run_notstarted = Дію не запущено · можливо, чергу призупинено
action_editor_test_run_foot_simulating = Моделювання...
action_editor_test_run_foot_finished = Запуск завершено
action_editor_test_run_foot_halted = Зупинено через помилку
action_editor_test_run_foot_notstarted = Не запущено
action_editor_test_run_again = Запустити знову
action_editor_test_run_close = Закрити

## Редактор дій - вибір підпункту / тригера

action_editor_this_action = цю дію
action_editor_recent_triggers = Нещодавні
picker_favorites = Обране
picker_favorites_empty = Позначте зірочкою, щоб закріпити тут для швидкого доступу
action_editor_picker_add_sub_title = Додати підпункт
action_editor_picker_inserting_into = Вставлення у
action_editor_picker_sub_count = · { $count } підпунктів
action_editor_picker_footer_hint = Додається з розумними типовими значеннями - редагуйте на місці
action_editor_picker_search = Пошук серед { $count } підпунктів...
action_editor_picker_fires = Запускає
action_editor_picker_available_count = · { $count } доступно
action_editor_trigger_picker_footer_hint = Створює новий тригер обраного типу та прив'язує його

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
triggers_search_placeholder = Пошук тригерів...
triggers_filter_hotkey = Гарячі клавіші
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
triggers_no_results_title = Нічого не знайдено
triggers_no_results_hint = Змініть або скиньте фільтри, щоб знайти тригери.
triggers_clear_filters = Скинути фільтри
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

triggers_menu_rename = Перейменувати...
triggers_menu_template = Використати як шаблон
triggers_menu_delete = Видалити...
triggers_template_copy_name = { $name } копія

## Реєстр тригерів - деталі у бічній панелі

triggers_sheet_section_configuration = КОНФІГУРАЦІЯ
triggers_sheet_config_overridden = { $count } перевизначено
triggers_sheet_config_all_defaults = усе за замовчуванням
triggers_sheet_no_config = Налаштованих полів немає
triggers_sheet_section_cooldown = ПЕРЕЗАРЯДКА
triggers_sheet_cooldown_caption = секунди · 0 = вимк
triggers_sheet_cooldown_value = перезарядка
triggers_sheet_cooldown_scope = Глобальна перезарядка
triggers_cooldown_suffix_global = { " · перезарядка=" }{ $secs }{ "с глобально" }
triggers_cooldown_suffix_per_user = { " · перезарядка=" }{ $secs }{ "с на глядача" }
triggers_sheet_section_permission = ДОСТУП
triggers_sheet_permission_value = мінімальний щабель
triggers_sheet_permission_helper = Вищі щаблі проходять завжди. Якщо платформа не може виразити щабель, він підвищується - суворіше, ніколи не слабше.
triggers_permission_meaning_everyone = Проходить будь-хто в чаті, з бейджем чи без.
triggers_permission_meaning_twitch_subscriber = Twitch: бейдж підписника або засновника
triggers_permission_meaning_twitch_vip = Twitch: бейдж VIP
triggers_permission_meaning_twitch_moderator = Twitch: бейдж модератора
triggers_permission_meaning_twitch_broadcaster = Twitch: лише власник каналу
triggers_permission_meaning_youtube_subscriber = YouTube: спонсори каналу (звичайні підписники не враховуються)
triggers_permission_meaning_youtube_vip = YouTube: ролі VIP немає - проходять лише модератори та власник каналу
triggers_permission_meaning_youtube_moderator = YouTube: модератори каналу
triggers_permission_meaning_youtube_broadcaster = YouTube: лише власник каналу
triggers_permission_meaning_kick_subscriber = Kick: бейдж підписника або засновника
triggers_permission_meaning_kick_vip = Kick: бейдж VIP
triggers_permission_meaning_kick_moderator = Kick: бейдж модератора
triggers_permission_meaning_kick_broadcaster = Kick: лише власник каналу
triggers_permission_rung_everyone = усі
triggers_permission_rung_subscriber = підписники+
triggers_permission_rung_vip = vip+
triggers_permission_rung_moderator = моди+
triggers_permission_rung_broadcaster = ведучий
triggers_permission_suffix = { " · доступ=" }{ $rung }
triggers_timer_every = кожні { $minutes } хв
triggers_timer_live_only = лише під час ефіру
triggers_timer_min_messages = ≥{ $count } { $count ->
    [one] повідомлення
    [few] повідомлення
    [many] повідомлень
   *[other] повідомлення
}
triggers_timer_hint_interval = Інтервал (хвилини): як часто спрацьовує, у цілих хвилинах (1-1440). Перше спрацювання - через один інтервал після збереження або після початку ефіру.
triggers_timer_hint_live = Лише в ефірі: пропускати спрацювання, поки жодна платформа і жоден вихід трансляції OBS не повідомляє про ефір.
triggers_timer_hint_messages = Мін. повідомлень у чаті: пропускати спрацювання, доки від останнього не надійде щонайменше стільки повідомлень глядачів. 0 = вимк. Боти й власні повідомлення forge не рахуються.
triggers_sheet_section_used_in = ВИКОРИСТОВУЄТЬСЯ В
triggers_sheet_delete_btn = Видалити
triggers_sheet_save_btn = Зберегти
triggers_detail_loading = Завантаження тригера...
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

## Реєстр тригерів - сповіщення про скасування видалення

triggers_toast_deleted = Видалено '{ $name }'

## Форма створення тригера - вибір типу

triggers_create_cancel = Скасувати
triggers_create_type_count = типів тригерів: { $count }
triggers_create_search_types = Пошук серед { $count } типів тригерів...
triggers_create_footer_hint = Оберіть джерело подій - налаштуйте далі
triggers_create_cat_server = Сервер
triggers_create_cat_timer = Таймер

## Форма створення тригера - заповнення

triggers_create_back = Назад
triggers_create_new_instance = Новий екземпляр: { $kind }
triggers_create_section_name = НАЗВА
triggers_create_name_placeholder = Назва екземпляра (обов'язково)
triggers_create_section_config = КОНФІГУРАЦІЯ
triggers_create_btn = Створити

## Налаштування → панель скриптів

settings_scripting_title = Скрипти (Rhai)
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
settings_scripting_core_allow_local_label = Дозволити localhost / приватні IP (HTTP-дія)
settings_scripting_core_allow_local_description = Вимикає захист від SSRF для дії HTTP-запиту. Вмикайте лише для локальної розробки.
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
trigger_cat_donations = Донати
trigger_cat_goals = Цілі
trigger_cat_clips = Кліпи
trigger_cat_streams = Трансляції
trigger_cat_users = Користувачі
trigger_cat_core = Ядро
trigger_cat_other = Інше

## Actions modals - placeholder literals

actions_name_placeholder = Моя автоматизація
actions_group_placeholder = Приклади
actions_description_placeholder = Відтворює звук, показує сповіщення...

## Actions - list states, toasts, delete confirm

actions_tree_loading = Завантаження дій...
actions_loading_queues = Завантаження черг...
actions_no_queue = Немає доступної черги
actions_toast_error = Дії: { $message }
actions_rename_taken = Назву '{ $name }' вже зайнято
actions_deleted_toast = Видалено '{ $name }'
actions_delete_title = Видалити дію?
actions_delete_body = Це видалить дію та всі її піддії й тригери.
actions_delete_scheduled_runs = { $count ->
    [one] Також буде скасовано { $count } її запланований запуск.
    [few] Також буде скасовано { $count } її заплановані запуски.
    [many] Також буде скасовано { $count } її запланованих запусків.
   *[other] Також буде скасовано { $count } її запланованих запусків.
}
actions_delete_scheduled_runs_unknown = Також буде скасовано всі її заплановані запуски, що очікують.

## Triggers registry - error messages

triggers_delete_reference_block = Вилучіть цей тригер з усіх дій перед видаленням.

## TTS - підписи вкладок

tts_tab_dashboard = Панель
tts_tab_engines = Рушії
tts_tab_aliases = Псевдоніми голосів
tts_tab_filters = Фільтри

## TTS - хлібні крихти

tts_breadcrumb_tts = Text-to-Speech

## TTS Dashboard - смуга керування

tts_dash_pause_btn = Пауза черги
tts_dash_resume_btn = Продовжити
tts_dash_skip_btn = Пропустити
tts_dash_stop_all_btn = Зупинити все
tts_dash_voice_gate_held = Утримано голосовим гейтом
tts_dash_stop_all_confirm_name = Зупинити всю озвучку
tts_dash_stop_all_confirm_hint = Повідомлення, що зараз озвучується, буде обірвано, а всі повідомлення в черзі - скинуто. Рушії залишаться готові обробляти нові повідомлення.
tts_dash_test_placeholder = Введіть текст для тестування...
tts_dash_speak_btn = Озвучити
tts_dash_test_speaker_name = Тест

## TTS Dashboard - зараз говорить

tts_dash_now_speaking_header = ЗАРАЗ ГОВОРИТЬ
tts_dash_no_speaking = -
tts_dash_last_drop = Останній запит відхилено: { $reason }

## TTS Dashboard - черга

tts_dash_queue_header = НАСТУПНИЙ
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
tts_engines_toggle_failed = Не вдалося змінити стан рушія
tts_engines_persist_disabled_failed = Не вдалося зберегти стан рушія

## TTS Filters - колонка пайплайну

tts_filters_pipeline_intro = Конвеєр, який текст проходить перед озвученням.

## TTS Filters - нумеровані картки етапів

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
tts_filters_blocklist_censor_meta = замінити на [beep]
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
tts_filters_output_strip_twitch_emotes = Прибирати емоції Twitch
tts_filters_output_strip_twitch_emotes_meta = видаляти назви емоцій Twitch перед озвученням
tts_filters_output_strip_reward_emotes = Прибирати емоції нагород
tts_filters_output_strip_reward_emotes_meta = видаляти назви емоцій із нагород за бали каналу
tts_filters_output_max_duration_meta = після { $secs }с
tts_filters_output_lang_meta = обирати голос за мовою повідомлення

## TTS Filters - список правил

tts_filters_badge_text = ТЕКСТ
tts_filters_badge_regex = REGEX
tts_filters_badge_text_inside_words = ТЕКСТ · ВСЕРЕДИНІ СЛІВ
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
tts_filters_preset_output_maxdur_label = МАКС. СЕКУНД
tts_filters_preset_output_maxdur_placeholder = 30
tts_filters_preset_output_maxdur_range = Дозволено: { $min }-{ $max } секунд
tts_filters_preset_output_sanitize_hint = "!!!" → "!"

tts_filters_modal_blocklist_words_label = СЛОВА АБО ФРАЗИ
tts_filters_modal_blocklist_words_placeholder = по одному на рядок або через кому...
tts_filters_modal_blocklist_note = Збіг не чутливий до регістру та шукає лише цілі слова; розділові знаки навколо слова ігноруються; багатослівні фрази не збігаються як одне ціле.
tts_filters_modal_blocklist_when_matched_label = ПРИ ЗБІГУ
tts_filters_modal_blocklist_censor_row = Цензурувати слово
tts_filters_modal_blocklist_censor_row_hint = замінити на [beep]
tts_filters_modal_blocklist_skip_row = Пропустити все повідомлення
tts_filters_modal_blocklist_skip_row_hint = нічого не озвучується

tts_filters_modal_replace_text_tab = ТЕКСТ
tts_filters_modal_replace_regex_tab = REGEX
tts_filters_modal_replace_find_label = ЗНАЙТИ
tts_filters_modal_replace_match_label = ШАБЛОН ЗБІГУ
tts_filters_modal_replace_find_placeholder = POG
tts_filters_modal_replace_replace_label = ЗАМІНИТИ НА
tts_filters_modal_replace_replace_text_placeholder = повага
tts_filters_modal_replace_inside_words_label = Шукати всередині слів
tts_filters_modal_replace_inside_words_hint = Вимкнено: GG замінює GG і GG!, але не POGGERS. Регістр не враховується.
tts_filters_modal_replace_note = Залиште заміну порожньою, щоб прибрати збіг.

## TTS Filters - налаштування конвеєра

## TTS Filters - колонка попереднього перегляду

tts_filters_preview_header = Живий перегляд
tts_filters_preview_input_label = ВХІДНЕ ПОВІДОМЛЕННЯ
tts_filters_preview_input_placeholder = Введіть повідомлення для перегляду...
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

## Хмарні рушії - кнопки картки

tts_cloud_test_connection_btn = Перевірити підключення
tts_cloud_testing_btn = Перевірка...
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

tts_aliases_search_placeholder = Пошук глядачів...
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
tts_aliases_loading = Завантаження псевдонімів голосів...

## Псевдоніми голосів - заблокований рядок

tts_aliases_never_speak = Ніколи не озвучувати

## TTS Псевдоніми голосів - бейджі ролей

tts_aliases_role_blocked = БЛОК

## Голосові аліаси - модалка призначення/редагування

tts_aliases_form_title_assign = Призначити голос
tts_aliases_form_title_edit = Редагувати голосовий аліас
tts_aliases_form_platform_label = ПЛАТФОРМА
tts_aliases_platform_any = Будь-яка
tts_aliases_form_viewer_label = ГЛЯДАЧ
tts_aliases_form_viewer_placeholder = Імʼя глядача
tts_aliases_form_engine_label = ДВИГУН
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
tts_aliases_viewer_taken = Цей глядач уже має голосовий псевдонім - редагуйте його.
common_delete = Видалити
common_undo = Відмінити

## Голосові аліаси - попереднє прослуховування

tts_aliases_preview_text = Це попереднє прослуховування голосу.

## Голосові аліаси - підпис під таблицею

tts_aliases_footer_caption = Показано { $shown } з { $total } ручних аліасів · { $auto } глядачів озвучуються автоматично

## Звукова панель - хлібні крихти

soundboard_breadcrumb_soundboard = Звукова панель

## Звукова панель - заголовок / модальне вікно

soundboard_loading = Завантаження кліпів...
soundboard_empty_title = Кліпів ще немає
soundboard_playback_error_prefix = Помилка відтворення: { $error }
soundboard_error_source_missing = Вихідний файл для { $name } зник.
soundboard_error_clip_gone = Цього звуку вже немає у звуковій панелі.
soundboard_import_unsupported = { $file } - не той формат звуку, який forge вміє програвати.
soundboard_import_type_mismatch = { $file } має розширення { $named }, але вміст файлу - { $detected }.
soundboard_import_too_large = { $file } важить { $size }, а ліміт для звуків - { $limit }.

## Звукова панель - модальне вікно

soundboard_modal_title_add = Додати кліп
soundboard_modal_title_edit = Редагувати кліп
soundboard_modal_no_file = Файл не обрано
soundboard_modal_browse_btn = Огляд
soundboard_modal_name_placeholder = Назва кліпу
soundboard_modal_save_btn = Зберегти
soundboard_modal_cancel_btn = Скасувати
soundboard_modal_validation_error = Назва та аудіофайл обов'язкові.
soundboard_delete_title = Видалити кліп?
soundboard_delete_body = Це назавжди видалить кліп зі звукової панелі.

## Звукова панель - назви секцій модального вікна

soundboard_modal_section_file = ФАЙЛ
soundboard_modal_section_name = НАЗВА
soundboard_device_system_default = Системний за замовчуванням

## Звукова панель - помилка завантаження пристроїв

## Звукова панель - помилка аудіоплеєра

## Звукова панель - зворотний зв'язок

## Soundboard - redesigned screen

soundboard_search_placeholder = Пошук звуків...
soundboard_header_summary = Вихід { $device } · { $count } звуків
soundboard_hero_title = Звукова панель
soundboard_hero_blurb = Запускайте звукові кліпи з падів, гарячих клавіш або дій. Спрямовується на віртуальний вихід, який може захопити OBS.
soundboard_hero_enabled = Увімкнено
soundboard_hero_disabled = Вимкнено
soundboard_category_all = Усі { $count }
soundboard_category_memes = Меми
soundboard_category_alerts = Сповіщення
soundboard_category_music = Стингери
soundboard_category_voice = Голос
soundboard_stop_all = Зупинити все
soundboard_pad_playing = грає...
soundboard_pad_source_missing = файл зник
soundboard_pad_source_missing_hint = Вихідний файл зник. Відредагуй цей звук і вкажи файл ще раз.
soundboard_pad_adopt_blocked = не копіюється
soundboard_pad_not_in_library = ще не в бібліотеці
soundboard_toast_clip_failed = { $clip }: { $error }
soundboard_toast_clip_failed_unnamed = Збій звуку: { $error }
soundboard_persist_failed = Не вдалося зберегти налаштування саундборду. Після перезапуску воно скинеться.
soundboard_no_matches = Немає звуків за фільтром
soundboard_library_section = Бібліотека
soundboard_library_import = Імпорт
soundboard_add_sound = Додати звук
soundboard_modal_section_category = КАТЕГОРІЯ
soundboard_modal_section_playback = ВІДТВОРЕННЯ
soundboard_modal_loop_label = Циклічне відтворення
soundboard_modal_loop_hint = Повторювати кліп, доки ви його не зупините.
soundboard_modal_ready = Готово до додавання
soundboard_modal_ready_edit = Готово до збереження
soundboard_modal_fill_required = Заповніть обовʼязкові поля
soundboard_modal_section_key = ГЛОБАЛЬНА КЛАВІША
soundboard_modal_key_none = Немає
soundboard_modal_key_set = Призначити
soundboard_modal_key_change = Змінити
soundboard_modal_key_clear = Очистити
soundboard_modal_key_hint = Вмикає й вимикає кліп звідусіль, навіть коли forge у фоні. Esc скасовує.
soundboard_modal_key_disabled = { $name } вимкнено - ця клавіша не спрацює.
soundboard_hotkeys_disabled = { $name } вимкнено - гарячі клавіші кліпів не спрацюють. Пади й дії й надалі відтворюють кліпи.
soundboard_toast_key_not_registered = { $name } збережено, але { $combo } не вдалося зареєструвати як глобальну клавішу. Прив'язка лишається збереженою, повторна спроба буде під час наступної зміни.
soundboard_toast_key_still_shared = { $name } збережено, але { $combo } не вдалося забрати в { $holder }. Клавіша лишається привʼязаною до обох - одне натискання запускає обидва.
soundboard_pad_key_not_live = { $combo } збережено, але вона не активна: система відмовила або клавішу утримує інша програма. Повторна спроба буде під час наступної зміни.
soundboard_pad_key_engine_unavailable = Глобальні клавіші недоступні в цій системі, тому { $combo } не спрацьовує.
soundboard_pad_key_shared = { $combo } також прив'язано до { $holder }. Одне натискання запускає обидва.
soundboard_routing_section = Маршрутизація виходу
soundboard_routing_device = ПРИСТРІЙ
soundboard_routing_hint = Додайте цей пристрій як Audio Input Capture в OBS.
soundboard_routing_volume = ГОЛОСНІСТЬ · { $pct }%
soundboard_routing_headphones = Також відтворювати в навушниках
soundboard_footer_left = { $sounds } звуків · { $categories } категорій · { $size }
soundboard_footer_unadopted =
    { $count ->
        [one] { $count } звук досі грає зі своєї початкової теки
        [few] { $count } звуки досі грають зі своєї початкової теки
        [many] { $count } звуків досі грають зі своєї початкової теки
       *[other] { $count } звуків досі грають зі своєї початкової теки
    }
soundboard_footer_adopt_action =
    { $count ->
        [one] Скопіювати { $count } кліп у бібліотеку
        [few] Скопіювати { $count } кліпи у бібліотеку
        [many] Скопіювати { $count } кліпів у бібліотеку
       *[other] Скопіювати { $count } кліпів у бібліотеку
    }
soundboard_footer_adopt_busy = Копіюю...
soundboard_adopt_copied = скопійовано: { $count }
soundboard_adopt_refused = відхилено: { $count }
soundboard_adopt_missing = зникло: { $count }
soundboard_adopt_unfinished = незавершено: { $count }
soundboard_adopt_nothing = Нічого не змінилось
soundboard_output_ready = Пристрій виходу готовий
soundboard_output_missing = Пристрій виходу відсутній

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
queues_loading = Завантаження черг...
queues_search_placeholder = Пошук черг...
queues_no_filter_match = Жодна черга не відповідає активному фільтру.
queues_filter_all = Усі
queues_filter_running = Активні
queues_filter_paused = На паузі
queues_filter_parallel = Паралельна
queues_filter_sequential = Послідовна
queues_configure_btn = Налаштувати
queues_drain_btn = Спустошити
queues_pause_btn = Пауза
queues_hold_btn = Утримати
queues_free_btn = Звільнити

## Черги - керування режимом

queues_mode_pause_tooltip = Зупинити обробку та пропускати всі нові дії
queues_mode_drain_tooltip = Продовжувати обробку накопиченого, пропускати всі нові дії
queues_mode_hold_tooltip = Зупинити обробку, але й далі приймати нові дії в чергу
queues_mode_active_tooltip = Повернути до виконання
queues_free_tooltip = Прибрати все, що чекає - дія, яка вже виконується, завершиться, а режим лишиться той самий
queues_free_feedback = Прибрано { $count } дій з очікування в черзі «{ $name }».
queues_free_failed = Не вдалося звільнити чергу: { $error }
queues_mode_change_failed = Не вдалося змінити режим черги: { $error }
queues_pause_all_failed = Не вдалося поставити на паузу всі черги: { $error }
queues_delete_failed = Не вдалося видалити чергу: { $error }
queues_load_failed = Не вдалося завантажити черги: { $error }
queues_mode_running_caption = Виконується - дії обробляються щойно надходять
queues_mode_drain_caption = Спустошення - накопичене завершується, нові дії пропускаються
queues_mode_hold_caption = Утримано - обробку зупинено, нові дії далі стають у чергу
queues_mode_pause_caption = Пауза - обробку зупинено, нові дії пропускаються
queues_strip_counts = { $pending } в очікуванні · { $in_flight } виконується
queues_overflow_badge = { $count } пропущено понад ліміт { $cap }

## Черги - меню картки

queues_menu_configure = Налаштувати...
queues_menu_pause = Пауза
queues_menu_resume = Продовжити
queues_menu_free = Звільнити чергу
queues_menu_delete = Видалити...
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
queues_concurrency_serial = Послідовна - лише одна дія за раз
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

## Черги - панель запущених дій

queues_running_now_header = ВИКОНУЄТЬСЯ ЗАРАЗ
queues_no_actions_running = Дій не виконується
queues_running_count = { $count } дій виконується
queues_running_label = виконується

## Черги - бейдж статусу

queues_status_paused = ПАУЗА
queues_status_running = ВИКОНУЄТЬСЯ
queues_status_draining = СПУСТОШЕННЯ
queues_status_held = УТРИМАНО

## Черги - розбіжність живого членства

queues_not_live_badge = НЕ В РОБОТІ · ЗАСТОСУВАТИ
queues_not_live_tooltip = Планувальник не працює з останніми налаштуваннями цієї черги. Натисни, щоб застосувати їх зараз.
queues_apply_live_done = Налаштування черги застосовано наживо.
queues_apply_live_failed = Не вдалося застосувати чергу наживо ({ $error }). Перезапусти forge, щоб підхопити зміну.
queues_apply_live_missing = черги більше не існує
queues_apply_live_not_found = планувальник не знає цієї черги

## Черги - чіп переповнення

queues_overflow_more = +{ $count } ще

## Черги - заплановані запуски

queues_scheduled_title = Заплановано
queues_scheduled_blurb = Запуски дій, відкладені кроком «Schedule Action» або скриптом
queues_scheduled_count = { $count } в очікуванні
queues_scheduled_loading = Завантаження запланованих запусків...
queues_scheduled_empty = Нічого не заплановано - додайте до дії крок «Schedule Action»
queues_scheduled_load_failed = Не вдалося завантажити заплановані запуски: { $error }
queues_scheduled_due_now = час настав
queues_scheduled_due_in = через { $span }
queues_scheduled_waiting_for_queue = чекає на свою чергу
queues_scheduled_from = від
queues_scheduled_from_removed = від видаленої дії
queues_scheduled_from_unknown = заплановано поза запуском дії
queues_scheduled_run_now = Запустити зараз
queues_scheduled_cancel = Скасувати
queues_scheduled_run_now_started = «{ $name }» запущено зараз.
queues_scheduled_run_now_refused = «{ $name }» не запустилася: { $reason }
queues_scheduled_run_now_failed = Не вдалося запустити зараз: { $error }
queues_scheduled_cancel_failed = Не вдалося скасувати запланований запуск: { $error }
queues_scheduled_not_pending = Цей запуск уже відбувся або скасований.
queues_scheduled_recent = НЕЩОДАВНІ ({ $count })
queues_scheduled_outcome_ran = ЗАПУЩЕНО
queues_scheduled_outcome_cancelled = СКАСОВАНО
queues_scheduled_outcome_skipped = ПРОПУЩЕНО
queues_scheduled_outcome_failed = ПОМИЛКА
queues_scheduled_reason_missed = Пропущено - forge не працював у призначений час
queues_scheduled_reason_missed_by = Пропущено на { $span } - forge не працював у призначений час
queues_scheduled_reason_superseded = Замінено новішим запуском з тим самим ключем
queues_scheduled_reason_cancelled = Скасовано до настання часу
queues_scheduled_reason_action_removed = Дію було видалено
queues_scheduled_reason_action_disabled = Дію було вимкнено
queues_scheduled_reason_action_not_found = Дії більше не існує
queues_scheduled_reason_actions_unavailable = Не вдалося прочитати дії
queues_scheduled_reason_queues_closed = Черги зупинялися
queues_scheduled_reason_unreadable = Не вдалося прочитати збережений запуск
queues_scheduled_reason_queue_paused = Її черга була на паузі
queues_scheduled_reason_queue_draining = Її черга спустошувалася
queues_scheduled_reason_queue_overflow = Її черга була переповнена
queues_scheduled_reason_queue_not_found = Її черги більше не існує
scheduled_span_minutes = { $minutes } хв
scheduled_span_hours = { $hours } год { $minutes } хв
scheduled_span_days = { $days } д { $hours } год

## Черги - описи вбудованих черг

## TTS дашборд - підписи карток рушіїв / бейдж пріоритету

tts_dash_priority_high = ВИСОК.
tts_dash_priority_bits = БІТСИ { $amount }

## TTS рушії - запасний підпис невідомого рушія

## Хмарний TTS - підписи полів форми

tts_cloud_field_api_key = API-ключ
tts_cloud_field_region = Регіон
tts_cloud_field_access_key_id = Ідентифікатор ключа доступу
tts_cloud_field_secret_key = Секретний ключ
tts_cloud_field_placeholder_subscription_key = Ключ підписки

## Звукова панель - фільтр файлового діалогу

soundboard_file_filter_audio = Аудіо

## Огляд платформ




## Платформи - чіпи можливостей


## Панель Twitch

twitch_reauth_title = Токен Twitch не має необхідних областей
twitch_reauth_detail = EventSub відхилив підписку на чат. Виконайте повторну авторизацію, щоб оновити токен із поточними областями.
twitch_reauth_btn = Повторна авторизація

## Панель OBS

## Вбудована деталь

builtin_picker_scene = Оберіть сцену
builtin_picker_source = Оберіть джерело
builtin_picker_audio_input = Оберіть аудіовхід
builtin_picker_hotkey = Оберіть гарячу клавішу
builtin_picker_expression = Оберіть вираз
builtin_picker_midi_port = Оберіть порт MIDI
builtin_picker_transition = Оберіть перехід
builtin_picker_profile = Оберіть профіль
builtin_picker_scene_collection = Оберіть колекцію сцен
builtin_picker_model = Оберіть модель
builtin_picker_item = Оберіть предмет
builtin_picker_item_instance = Оберіть завантажений предмет

## OAuth / локальний callback-потік

oauth_connect_explainer_prefix = Forge використовує
oauth_connect_explainer_emphasis = { " " }OAuth 2.0 Authorization Code з PKCE
oauth_connect_explainer_suffix = . Ваш браузер відкриває { $name }, а відповідь повертається на локальну loopback-адресу - жоден секрет ніколи не зберігається.
oauth_status_authorizing = Авторизація
oauth_status_authorized = Авторизовано
oauth_step_open_title = Відкрийте { $name } у браузері
oauth_step_approve_title = Надайте доступ на платформі
oauth_step_approve_caption = Forge очікує код авторизації на loopback-перенаправленні.
oauth_step_exchange_title = Обмін коду на токен (+ verifier)
oauth_step_exchange_caption = PKCE code_verifier підтверджує запит; токени зберігаються зашифрованими на цьому пристрої.
oauth_step_connected_title = Підключено
oauth_progress_launching = Запуск браузера...
oauth_progress_waiting = Очікування підтвердження на { $name }...
oauth_progress_subline = loopback :{ $port } · scopes: { $scopes }
oauth_done_authorized = Авторизовано - завершуємо...
oauth_footer_choose_different = Обрати іншу платформу
oauth_footer_signin = Увійти через { $name }
oauth_btn_retry = Повторити
oauth_btn_cancel = Скасувати
oauth_failed_title = Авторизація не вдалася
oauth_kick_hybrid_title = Гібридний транспорт чату

## Потік Twitch device code

twitch_device_explainer_prefix = Twitch використовує
twitch_device_explainer_emphasis = { " " }OAuth 2.0 Device Code
twitch_device_explainer_suffix = { " " }grant. Відкрийте посилання, введіть код, і Forge автоматично визначить, коли ви завершите - жоден секрет ніколи не зберігається.
twitch_device_open_title = Відкрийте цей URL у будь-якому браузері
twitch_device_open_btn = Відкрити
twitch_device_enter_title = Введіть цей код на сторінці
twitch_device_copy = Копіювати
twitch_device_copied = Скопійовано
twitch_device_expires_in = Спливає через
twitch_device_get_new_code = Отримати новий код
twitch_device_waiting = Очікування підтвердження на { $name }...
twitch_device_requesting = Запитуємо код у { $name }...
twitch_device_polling_subline = опитування кожні { $interval }с · scopes: { $scopes }
twitch_device_do_later = Зроблю це пізніше
twitch_device_expired_title = Цей код сплив, перш ніж ви завершили
twitch_device_denied_title = Авторизацію відхилено
twitch_device_failed_title = Не вдалося почати авторизацію
twitch_device_denied_detail = Ви відмовили в доступі на Twitch. Повторіть, щоб отримати новий код.

## Екран сервера

server_breadcrumb_server = WebSocket-сервер
server_status_listening = Слухає · { $clients ->
    [one] { $clients } клієнт
    [few] { $clients } клієнти
    [many] { $clients } клієнтів
   *[other] { $clients } клієнтів
  }
server_status_stopped = Зупинено
server_not_running = Не запущено
server_console_disabled_hint = Сервер вимкнено
server_unavailable_banner = Сервер не вдалося запустити: { $reason }. Увімкнення не допоможе, доки це не виправлено; перевір лог і перезапусти forge.
server_console_disabled_hint_link = Увімкнути в Налаштуваннях -> WebSocket
server_bind_address = АДРЕСА ПРИВ'ЯЗКИ
server_bind_address_loading = Читаємо адресу...
server_bearer_token = BEARER-ТОКЕН
server_btn_restart = Перезапуск
server_btn_restarting = Перезапуск...
server_stat_clients = КЛІЄНТИ
server_stat_clients_sub = +{ $count } за { $minutes } хв
server_stat_events_rate = ПОДІЙ / С
server_stat_events_sub = сер. за { $seconds }с
server_stat_http = HTTP-ЗАПИТИ
server_stat_http_sub = оверлеїв віддано
server_stat_bandwidth = ПРОПУСКНА ЗДАТНІСТЬ
server_stat_bandwidth_value = { $rate } КБ/с
server_stat_bandwidth_sub = ↑ назовні
server_clients_header = Підключені клієнти
server_clients_empty = Немає підключених клієнтів
server_col_client = КЛІЄНТ
server_col_subscriptions = ПІДПИСКИ
server_col_evs = ПОД/С
server_col_uptime = АПТАЙМ
server_overlay_files_empty = Файли оверлею не знайдено
server_overlay_dir_files = { $count ->
    [one] { $count } файл
    [few] { $count } файли
    [many] { $count } файлів
   *[other] { $count } файлів
  }
server_disconnect_confirm_hint = Клієнта { $info } буде відключено від WebSocket-сервера. Інші клієнти не постраждають.
server_disconnect_tooltip = Відключити цього клієнта
server_btn_regenerate = Оновити
server_regen_warning_title = Оновлення відключить усіх клієнтів
server_throughput_title = Пропускна здатність
server_throughput_meta = останні { $seconds }с · макс { $max } под/с
server_overlay_host_title = Хостинг оверлеїв
server_overlay_browser_source_url = URL ДЛЯ БРАУЗЕРНОГО ДЖЕРЕЛА
server_open_overlay_folder = Відкрити теку оверлеїв
server_open_overlay_folder_failed = Не вдалося відкрити теку оверлеїв
server_clients_live = наживо
server_footer_totals = Надіслано всього: { $sent } · Подій всього: { $events }
server_footer_health_ok = Справно
server_footer_health_degraded = Погіршено (втрачено { $dropped })
server_footer_endpoint_accepting = WebSocket :{ $port } · приймає підключення
server_footer_endpoint_stopped = WebSocket · не запущено
server_disconnect_confirm_title = Відключити клієнта?
server_disconnect_esc_hint = щоб скасувати
server_btn_disconnect = Відключити

## Загальні бейджі статусу (використовуються на сторінках деталей платформ)

common_status_not_connected = Не підключено
common_status_connected = Підключено

## Деталі платформи YouTube

twitch_description = Чат, підписки, біти, рейди, нагороди каналу та EventSub.
youtube_description = Живий чат, супер-чати, членство в каналі, підписники.

## Деталі платформи Kick

kick_description = Чат, підписки, хости - гібрид: офіційний OAuth API для відправки, Pusher WS спільноти для отримання. Не афілійовано з Kick.com.

## Деталі VTube Studio

## Огляд стрим-додатків


## Живий чат - заголовок / фільтри

chat_breadcrumb_audience = Аудиторія
chat_breadcrumb_chat = Чат
chat_filter_all = Всі
chat_filter_events = Лише події
chat_filter_hide_bots = Сховати ботів
chat_viewers_unit = глядачів
chat_no_filter_matches = Жодне повідомлення не відповідає цим фільтрам.
chat_send_placeholder_connected = Надіслати в чат...
chat_send_placeholder_to = Надіслати в чат {$platform}...
chat_send_placeholder_offline = Жодна чат-платформа не підключена
chat_send_failed = Не вдалося надіслати в { $platform }: { $error }
chat_send_no_response = { $platform } не підтвердив повідомлення
chat_send_no_platform_enabled = Жодна чат-платформа не увімкнена
chat_send_no_platform_connected = Жодна увімкнена чат-платформа не підключена
chat_send_too_long = Повідомлення задовге: { $count } з { $limit } символів
chat_send_unavailable = Надсилання в чат недоступне
chat_search_placeholder = Пошук повідомлень...
chat_new_message = 1 нове повідомлення
chat_new_messages = { $count ->
    [one] { $count } нове повідомлення
    [few] { $count } нових повідомлення
    [many] { $count } нових повідомлень
   *[other] { $count } нових повідомлень
}
chat_gap_messages = { $count ->
    [one] Тут пропущено { $count } повідомлення
    [few] Тут пропущено { $count } повідомлення
    [many] Тут пропущено { $count } повідомлень
   *[other] Тут пропущено { $count } повідомлень
}
chat_gap_events = { $count ->
    [one] Тут пропущено { $count } подію - серед них могли бути повідомлення
    [few] Тут пропущено { $count } події - серед них могли бути повідомлення
    [many] Тут пропущено { $count } подій - серед них могли бути повідомлення
   *[other] Тут пропущено { $count } подій - серед них могли бути повідомлення
}
chat_gap_both = Тут пропущено повідомлень: { $messages }, інших подій: { $events }
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
chat_drawer_whisper_placeholder = Введіть повідомлення...
chat_drawer_whisper_send = Надіслати
chat_drawer_whisper_cancel = Скасувати
chat_drawer_set_tts_voice = Встановити голос TTS...
chat_drawer_block_tts = Заблокувати TTS
chat_drawer_timeout = Таймаут 10 хв
chat_drawer_ban = Заблокувати в каналі
chat_stat_messages = ПОВІДОМЛЕННЯ
chat_stat_sub = ПІДПИСКА
chat_stat_sub_yes = Так
chat_stat_follow = ФОЛОУ
chat_follow_not_following = Не фоловить
chat_follow_hidden = Приховано
chat_drawer_history_loading = Завантаження повідомлень...
chat_drawer_history_empty = Немає збережених повідомлень
chat_drawer_history_failed = Не вдалося завантажити історію повідомлень
chat_drawer_history_older_failed = Не вдалося завантажити старіші повідомлення
chat_drawer_history_retry = Повторити
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
chat_drawer_tts_voice_saved = Голос TTS збережено
chat_drawer_tts_voice_failed = Не вдалося встановити голос TTS: { $error }
chat_ctx_timeout_10m = Таймаут 10 хв
chat_ctx_timeout_1h = Таймаут 1 година
chat_ctx_timeout_2w = Таймаут 2 тижні
chat_ctx_ban = Бан
chat_ctx_timeout_sent = Таймаут застосовано
chat_reply = Відповісти
chat_reply_title = Відповідь для @{ $recipient }
chat_reply_placeholder = Введіть відповідь...
chat_reply_sent = Відповідь надіслано
chat_reply_failed = Помилка відповіді: { $error }
chat_tab_feed = Стрічка
chat_tab_banned = Заблоковані
chat_banned_col_viewer = Глядач
chat_banned_col_reason = Причина
chat_banned_col_moderator = Модератор
chat_banned_col_banned = Бан
chat_banned_col_expires = Діє до
chat_banned_permanent = Назавжди
chat_banned_until = до { $time }
chat_banned_unban = Розбанити
chat_banned_unban_failed = Не вдалося розбанити: { $error }
chat_banned_outside_forge = Забанено поза forge - зніміть бан у YouTube Studio
chat_banned_loading = Завантаження банів...
chat_banned_empty = Активних банів немає
chat_banned_load_more = Завантажити ще
chat_banned_retry = Повторити
chat_banned_no_platform = Жодна підключена платформа не надає список банів
chat_banned_not_connected = Не підключено
chat_banned_missing_permission = Бракує дозволу - підключіться знову, щоб надати його
chat_banned_try_later = Не вдалося завантажити бани - спробуйте пізніше
chat_banned_seen_by_forge = Лише бани, які forge видав або побачив під час підключення - не повний список платформи
chat_dispatch_cancelled = Дію скасовано до завершення

## Стрічка подій - заголовок / фільтри

event_feed_filter_all = Всі { $n }
event_feed_filter_chat = Чат { $n }
event_feed_filter_subs = Підписки { $n }
event_feed_filter_bits = Бітси { $n }
event_feed_filter_timers = Таймери { $n }
event_feed_filter_obs = OBS { $n }
event_feed_filter_errors = Помилки { $n }
event_feed_export_success = Стрічку подій експортовано до { $path }
event_feed_export_failed = Не вдалося експортувати стрічку подій: { $error }
event_feed_replay_success = Подію відтворено
event_feed_replay_not_found = Цієї події вже немає в журналі подій, її не можна відтворити.
event_feed_replay_failed = Не вдалося відтворити подію: { $error }
event_feed_no_events = Подій ще немає - системна активність з'явиться тут в реальному часі.
event_feed_no_filter_match = Жодна подія не відповідає активному фільтру.
event_feed_inspector_title = Інспектор подій
event_feed_auto_scroll_on = Авто-прокрутка увімкнена
event_feed_auto_scroll_off = Авто-прокрутка вимкнена
event_feed_breadcrumb_automation = Автоматизація
event_feed_events_live_stream = подій · живий стрім
event_feed_status_live = Наживо
event_feed_status_paused = Пауза
event_feed_search_placeholder = Пошук подій...

## Глобальні змінні - заголовок / фільтри

globals_breadcrumb_automation = Автоматизація
globals_breadcrumb_globals = Глобальні
globals_filter_all = Всі
globals_filter_persisted = Збережені
globals_filter_session = Сесійні
globals_search_placeholder = Шукати змінні...
globals_loading = Завантаження...
globals_deleted_toast = Видалено '{ $name }'
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
globals_export_done = Глобальні змінні експортовано до { $path }
globals_export_failed = Не вдалося експортувати глобальні змінні: { $error }

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

script_editor_breadcrumb_automation = Автоматизація
script_editor_edited_prefix = змінено
script_editor_run = Тестовий запуск
script_editor_save = Зберегти
script_editor_format = Форматувати
script_editor_api_docs = Документація API
script_editor_debug = Відлагодження
script_editor_output_header = Виведення
script_editor_api_reference = Довідка API
script_editor_search_placeholder = Пошук скриптів...
script_editor_new_script = Новий скрипт
script_editor_no_scripts = Скриптів ще немає
script_editor_group_action = Скрипти дій
script_editor_group_standalone = Окремі
script_editor_manual_run = ручний запуск
script_editor_rename_action = Перейменувати
script_editor_enable_action = Увімкнути
script_editor_disable_action = Вимкнути
script_editor_delete_action = Видалити
script_editor_running = Виконується...
script_editor_run_modal_cancel = Скасувати
script_editor_save_blocked = Збереження заблоковано - спершу виправте синтаксичні помилки
script_editor_discard_title = Відхилити незбережені зміни?
script_editor_discard_body = У цьому скрипті є незбережені зміни. Продовжити й втратити їх або залишитися для редагування.
script_editor_discard_confirm = Відхилити
script_editor_discard_cancel = Продовжити редагування
script_editor_discard_esc_hint = щоб продовжити редагування
script_editor_sandbox_label = Пісочниця:
script_editor_sandbox_enabled = увімкнено
script_editor_problems_tab = Проблеми
script_editor_console_cleared = Консоль очищено.
script_editor_no_problems = Проблем немає.
script_editor_inert_annotation = Ця анотація більше нічого не оголошує - входи редагуються на панелі СИГНАТУРА.

## Редактор скриптів - модальне вікно запуску

script_editor_health = { $ok }/{ $total } справні
script_editor_type_check_passed = Перевірку типів пройдено
script_editor_type_check_failed = Перевірку типів не пройдено
script_editor_run_modal_title = Запустити { $name }
script_editor_run_input_placeholder = Введіть значення { $label }...
script_editor_run_input_placeholder_example = Введіть значення { $label }, напр. { $example }

## Action telemetry - stat column headers

## Action editor - validation errors

## Integration detail - OBS / quick-action

builtin_disconnect_confirm_hint = Вас буде відключено, і потрібно буде підключитися повторно вручну. Живі події від цієї інтеграції перестануть надходити до цього моменту.
integration_disconnect_title = Відключити інтеграцію
integration_settings_coming_soon = Налаштування зʼявляться згодом
integration_control_failed = Команда керування не виконалася
integration_quick_action_failed = Швидка дія не виконалася
integration_row_action_failed = Не вдалося виконати дію рядка: { $error }
integration_open_url_failed = Не вдалося відкрити посилання в браузері
integration_quick_action_na = Н/Д
integration_quick_action_not_live = Недоступно в поточному стані
integration_status_authenticated = Автентифіковано
integration_token_expires_in = термін токена спливає через { $time }
integration_viewers_delta = { $delta } за останні { $window }
integration_qa_filter_placeholder = Фільтрувати дії...
integration_qa_no_matches = Немає дій за фільтром
integration_quick_action_ran = Виконано "{ $label }"
integration_run_history = Історія запусків
integration_run_history_failed = Не вдалося завантажити історію запусків
integration_qa_modal_subtitle = Перевірте це, не створюючи дію
integration_qa_modal_confirm_body = Виконати "{ $label }" зараз із поточним станом каналу?
integration_qa_modal_footer_immediate = Виконується одразу - дію не створено
integration_qa_modal_footer_destructive = Це одразу впливає на ваш живий канал
integration_qa_modal_run = Виконати
integration_qa_modal_confirm = Підтвердити
integration_qa_field_loading = Завантаження варіантів...
integration_qa_field_retry = Повторити
integration_qa_field_select = Оберіть...
integration_qa_field_unavailable = Варіанти недоступні, поки цю інтеграцію відключено
integration_qa_field_no_scene = Немає активної сцени, щоб зчитати джерела
integration_qa_scene_current_hint = Зараз: { $scene } (В ЕФІРІ)
integration_qa_toggle_on = Увімк.
integration_qa_toggle_off = Вимк.
integration_qa_source_visible = видиме
integration_qa_source_hidden = приховане
integration_qa_field_range = Дозволено: { $min }-{ $max }
integration_qa_field_range_open = Дозволено: { $min } або більше
collection_modal_subtitle = Зміни застосовуються одразу - без створення дії
collection_form_creating = Новий елемент
collection_form_editing = Редагування "{ $title }"
collection_form_footer = Зберігається безпосередньо в інтеграції
collection_new = Створити
collection_save = Зберегти
collection_close = Закрити
collection_loading = Завантаження...
collection_empty = Тут поки нічого немає
collection_read_only = ЛИШЕ ЧИТАННЯ
collection_count = Елементів: { $count }
collection_count_of = Використано { $count } з { $capacity }
collection_count_full = Ліміт вичерпано - { $capacity } з { $capacity }
collection_count_unknown = -
collection_saved = Збережено "{ $title }"
collection_deleted = Видалено "{ $title }"
collection_delete_title = Видалити елемент?
collection_delete_body = Його буде остаточно видалено з інтеграції.
collection_delete_confirm = Видалити
collection_field_unsupported = Це поле не можна редагувати тут
collection_field_max_chars = До { $max } символів
collection_failure_not_connected = Немає з'єднання - спершу підключіть інтеграцію
collection_failure_unauthorized = Авторизація застаріла - увійдіть знову
collection_failure_not_owned = Створено поза forge - тут його змінити не можна
collection_failure_capacity = Ліміт вичерпано - спершу видаліть елемент
collection_failure_not_eligible = Цей канал не відповідає вимогам для цієї функції - нагороди за бали каналу Twitch доступні лише партнерам і компаньйонам (Affiliate / Partner)
collection_failure_rate_limited = Забагато запитів - спробуйте трохи згодом
collection_failure_transport = Помилка з'єднання - спробуйте ще раз
collection_text_channel_point_rewards = Нагороди за бали каналу
collection_text_title = Назва
collection_text_cost = Вартість (бали каналу)
collection_text_prompt = Підказка
collection_text_optional = необов'язково
collection_text_require_input = Вимагати введення від глядача
collection_text_background_color = Колір фону (#RRGGBB)
collection_text_max_per_stream = Максимум за стрім (0 = без обмежень)
collection_text_max_per_viewer = Максимум на глядача за стрім (0 = без обмежень)
collection_text_global_cooldown = Загальна затримка, секунди (0 = вимк.)
collection_text_enabled = Увімкнено
collection_text_paused = Призупинено
collection_text_created_outside = Створено поза forge - редагуйте в панелі Twitch
collection_text_title_required = Назва обов'язкова
collection_text_color_format = Колір фону має бути порожнім або #RRGGBB
collection_text_zero_or_more = Має бути 0 або більше
collection_text_duplicate_title = Нагорода з такою назвою вже існує
collection_text_automod_rejected = Twitch AutoMod відхилив текст нагороди
collection_text_settings_rejected = Twitch відхилив налаштування нагороди
collection_text_reward_gone = Цієї нагороди більше не існує
collection_text_rejected_per_viewer = Twitch відхилив ліміт на глядача
collection_text_rejected_per_stream = Twitch відхилив ліміт на стрім
collection_text_rejected_cooldown = Twitch відхилив перезарядку
collection_text_rejected_color = Twitch відхилив колір тла
collection_text_rejected_prompt = Twitch відхилив підказку
collection_text_rejected_cost = Twitch відхилив вартість
collection_text_rejected_title = Twitch відхилив назву
collection_text_unknown_collection = Невідома колекція
collection_text_unknown_toggle = Невідомий перемикач нагороди
collection_text_title_max = Назва має містити не більше { $max } символів
collection_text_prompt_max = Підказка має містити не більше { $max } символів
collection_text_cost_min = Вартість має бути не меншою за { $min }
integration_state_connecting_title = Підключення...
integration_state_connecting_detail = Встановлення сеансу з цією інтеграцією.
integration_state_reconnecting_title = Повторне підключення...
integration_state_reconnecting_detail = Сеанс розірвано; forge відновлює його.
integration_state_disconnected_detail = Натисніть «Підключитися» вгорі, щоб зв'язати цю інтеграцію.
obs_state_password_rejected_title = OBS відхилив збережений пароль
obs_state_password_rejected_detail = Введіть актуальний пароль WebSocket у налаштуваннях підключення OBS і підключіться знову.

## OAuth / authentication errors

auth_error_credentials_missing_twitch = Інтеграцію Twitch не налаштовано. Встановіть FORGE_TWITCH_CLIENT_ID із client_id вашого зареєстрованого додатка та перезапустіть.
auth_error_credentials_missing_youtube = Облікові дані OAuth для YouTube не налаштовані
auth_error_credentials_missing_kick = Інтеграцію Kick не налаштовано. Встановіть FORGE_KICK_CLIENT_ID та FORGE_KICK_CLIENT_SECRET із облікових даних вашого зареєстрованого додатка та перезапустіть.

## Widget - event inspector

widget_event_replay = Відтворити цю подію
widget_event_payload_header = ДАНІ
widget_event_caused_header = ПРИЧИНА

## Widget - chat row

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
widget_builtin_chip_grid_empty = Поки немає елементів

## Widget - server file list

## Widget - server confirm modal

widget_confirm_type_prefix = Введіть
widget_confirm_type_suffix = для підтвердження:
widget_confirm_esc_to_cancel = щоб скасувати
widget_confirm_cancel = Скасувати

## Widget - destructive confirm modal

widget_confirm_delete_title = Видалити { $kind }?
widget_confirm_delete_hint = Цей елемент буде остаточно видалено. Дію не можна скасувати.
widget_confirm_delete_kind_script = скрипт

## Widget - date time picker

widget_date_month_january = Січень
widget_date_month_february = Лютий
widget_date_month_march = Березень
widget_date_month_april = Квітень
widget_date_month_may = Травень
widget_date_month_june = Червень
widget_date_month_july = Липень
widget_date_month_august = Серпень
widget_date_month_september = Вересень
widget_date_month_october = Жовтень
widget_date_month_november = Листопад
widget_date_month_december = Грудень
widget_date_weekday_sunday = Нд
widget_date_weekday_monday = Пн
widget_date_weekday_tuesday = Вт
widget_date_weekday_wednesday = Ср
widget_date_weekday_thursday = Чт
widget_date_weekday_friday = Пт
widget_date_weekday_saturday = Сб

## Widget - save indicator

widget_copied_toast = Скопійовано в буфер обміну
widget_save_all_saved = Усі зміни збережено
widget_save_saving = Збереження...
widget_save_unsaved = Незбережені зміни
widget_save_failed = Помилка збереження: { $error }

## Widget - server bearer token

## Widget - server bind card

## Widget - picker modal

widget_picker_search_placeholder = Пошук...
widget_picker_loading = Завантаження...
widget_picker_no_results = Нічого не знайдено.

## Widget - output device picker

widget_device_default_suffix = (за замовчуванням)
widget_device_test = Тест

## Widget - quick actions panel

widget_quick_actions_title = Швидкі дії

## Widget - console (script output)

## Settings - audio output

settings_audio_scanning = Сканування пристроїв...
settings_audio_title = Аудіо
settings_audio_output_devices = ПРИСТРОЇ ВИВОДУ
settings_audio_test_section = ТЕСТ
settings_audio_test_tone = Відтворити тестовий тон 440 Гц
settings_audio_test_playing = Відтворення...
settings_audio_test_error = Помилка тестового тону: { $error }
settings_audio_persist_error = Не вдалося зберегти вибір пристрою: { $error }

## Settings - маршрутизація аудіо

settings_audio_routing_title = Маршрутизація
settings_audio_routing_loading = Читання налаштувань маршрутизації
settings_audio_routing_load_error = Не вдалося прочитати налаштування маршрутизації: { $error }
settings_audio_routing_speech_label = Мовлення відтворюється на
settings_audio_routing_speech_hint = Де чути синтезоване мовлення.
settings_audio_routing_clips_label = Звукові кліпи відтворюються на
settings_audio_routing_clips_hint = Де чути кліпи саундборду.
settings_audio_route_local = Локальний пристрій
settings_audio_route_overlay = Аудіооверлей
settings_audio_route_both = Обидва
settings_audio_routing_destination = АУДІООВЕРЛЕЙ
settings_audio_routing_destination_none = Немає
settings_audio_routing_no_overlays = Спочатку створи оверлей на екрані оверлеїв.
settings_audio_routing_test = Тест на оверлеї
settings_audio_routing_testing = Відтворення
settings_audio_routing_test_error = Тест оверлея не вдався: { $error }
settings_audio_routing_persist_error = Не вдалося зберегти маршрутизацію: { $error }
settings_audio_routing_in_effect = ЗАРАЗ ДІЄ
settings_audio_routing_in_effect_fallback = { $route } - { $reason }
settings_audio_routing_fallback_unchosen = не вибрано аудіооверлей
settings_audio_routing_fallback_server_off = сервер, який передає звук оверлея, не запущено
settings_audio_routing_fallback_unreadable = не вдалося знайти вибраний аудіооверлей
settings_audio_routing_fallback_missing = вибраного аудіооверлея більше не існує
settings_audio_routing_pages_none = Жодної сторінки не підключено
settings_audio_routing_pages_connected =
    { $count ->
        [one] Підключено { $count } сторінку
        [few] Підключено { $count } сторінки
        [many] Підключено { $count } сторінок
       *[other] Підключено { $count } сторінок
    }
settings_audio_routing_pages_duplicate =
    { $count ->
        [one] Кожна підключена сторінка відтворює той самий звук, тож кліп буде чути { $count } раз у трансляції.
        [few] Кожна підключена сторінка відтворює той самий звук, тож кліп буде чути { $count } рази в трансляції.
        [many] Кожна підключена сторінка відтворює той самий звук, тож кліп буде чути { $count } разів у трансляції.
       *[other] Кожна підключена сторінка відтворює той самий звук, тож кліп буде чути { $count } разів у трансляції.
    }

## Settings - voice gate

settings_voice_gate_title = Голосовий гейт
settings_voice_gate_enable_label = Утримувати озвучення, поки ти говориш
settings_voice_gate_enable_hint = Призупиняє чергу озвучення, щойно мікрофон вловлює звук вище порогу.
settings_voice_gate_input_devices = ПРИСТРІЙ ВВОДУ
settings_voice_gate_scanning = Сканування пристроїв...
settings_voice_gate_threshold_label = Поріг
settings_voice_gate_threshold_hint = Піковий рівень входу, який вважається мовленням. Індикатор показує поточний рівень.
settings_voice_gate_hold_label = Утримання
settings_voice_gate_hold_hint = Скільки черга лишається утриманою після завершення мовлення, у мілісекундах.
settings_voice_gate_hold_placeholder = 800
settings_voice_gate_hold_range = Дозволено: { $min }-{ $max } мс
settings_voice_gate_state_off = Вимкнено
settings_voice_gate_state_inactive = Слухає - мовлення не виявлено
settings_voice_gate_state_active = Виявлено мовлення - чергу утримано
settings_voice_gate_state_unavailable = Мікрофон недоступний: { $error }
settings_voice_gate_retry = Повторити
settings_voice_gate_persist_error = Не вдалося зберегти налаштування голосового гейта: { $error }

## Script editor - API docs panel

script_editor_api_no_matches = Збігів не знайдено
script_editor_api_search_placeholder = Пошук модулів...

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
script_editor_signature_add_input = Додати вхід
script_editor_signature_no_inputs = Вхідних параметрів немає
script_editor_signature_no_return = не оголошено
script_editor_input_name_empty = Назва входу не може бути порожньою
script_editor_input_name_invalid = «{ $name }» не є коректним ідентифікатором
script_editor_input_name_reserved = «{ $name }» зарезервовано рушієм
script_editor_input_name_duplicate = «{ $name }» вже оголошено
script_editor_run_stats_heading = СТАТИСТИКА
script_editor_stat_runs = ЗАПУСКИ
script_editor_stat_avg = СЕРЕДНЄ
script_editor_stat_runs_value = { $n } сьогодні
script_editor_stat_avg_value = { $n } мс

## Widget - layout chrome

## Widget - volume slider

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

## Недоступна інтеграція
builtin_unavailable_metric_status = Статус
builtin_unavailable_metric_uptime = Час роботи
builtin_unavailable_metric_endpoint = Адреса
builtin_unavailable_metric_version = Версія

obs_connect_title = OBS Studio
obs_connect_subtitle = Підключіться, щоб керувати сценами, джерелами, звуком, фільтрами та записом
obs_connect_guide_title = Перед початком
obs_connect_guide_intro = В OBS Studio увімкніть вбудований сервер WebSocket, а потім перенесіть налаштування сюди.
obs_connect_step_menu_prefix = В OBS:
obs_connect_step_menu_path = Tools → WebSocket Server Settings
obs_connect_step_enable_prefix = Позначте
obs_connect_step_enable_option = Enable WebSocket server
obs_connect_step_port_prefix = Занотуйте
obs_connect_step_port_field = порт
obs_connect_step_port_default_prefix = (типово
obs_connect_step_reveal_prefix = Натисніть
obs_connect_step_reveal_button = Show Connect Info
obs_connect_step_reveal_suffix = для показу пароля
obs_connect_settings_title = Налаштування підключення
obs_connect_field_host = ХОСТ
obs_connect_field_port = ПОРТ
obs_connect_field_password = ПАРОЛЬ
obs_connect_password_note = зберігається зашифровано
obs_connect_auto_reconnect_label = Автоматичне перепідключення
obs_connect_auto_reconnect_hint = Повтори з експоненційною затримкою
obs_connect_on_launch_label = Підключатися під час запуску
obs_connect_on_launch_hint = Починати підключення разом із запуском Forge
obs_connect_btn_test = Перевірити підключення
obs_connect_btn_connect = Підключити
obs_connect_btn_save_reconnect = Зберегти та перепідключитися
obs_connect_testing = Перевірка підключення
obs_connect_connecting = Підключення
obs_connect_test_successful = Перевірка успішна
obs_connect_test_detail = obs-websocket v{ $version } · сцен: { $scenes } · RTT { $rtt } мс
obs_connect_test_failed = Перевірка не вдалася
obs_connect_failed = Не вдалося підключитися
obs_connect_error_title = Хибні налаштування
obs_connect_error_unreachable = На цьому хості та порту ніхто не відповів. Чи запущено OBS з увімкненим WebSocket-сервером?
obs_connect_error_auth = OBS відхилив пароль
obs_connect_error_timeout = OBS не відповів вчасно
obs_connect_error_unknown = OBS перервав спробу підключення
obs_connect_error_invalid_port = Порт має бути числом від 1 до 65535
obs_connect_settings_save_failed = Не вдалося зберегти налаштування підключення OBS

vtube_connect_title = VTube Studio
vtube_connect_subtitle = Підключіться, щоб керувати аватаром: вирази, гарячі клавіші та кинуті предмети
vtube_connect_guide_title = Перед початком
vtube_connect_guide_intro = VTube Studio має бути запущений з увімкненим API. Forge потрібно підтвердити як плагін усередині програми.
vtube_connect_step_open_prefix = У VTube Studio відкрийте
vtube_connect_step_open_path = Settings → API
vtube_connect_step_enable_prefix = Увімкніть
vtube_connect_step_enable_option = Start API (allow plugins)
vtube_connect_step_port_prefix = Занотуйте
vtube_connect_step_port_field = порт
vtube_connect_step_port_default_prefix = (типово
vtube_connect_step_approve_prefix = Натисніть
vtube_connect_step_approve_button = Connect
vtube_connect_step_approve_suffix = тут, а потім Allow у вікні VTube Studio
vtube_connect_settings_title = Налаштування підключення
vtube_connect_field_host = ХОСТ
vtube_connect_field_port = ПОРТ
vtube_connect_field_plugin = НАЗВА ПЛАГІНА
vtube_connect_auto_reconnect_label = Автоматичне перепідключення
vtube_connect_auto_reconnect_hint = Повтори з експоненційною затримкою
vtube_connect_on_launch_label = Підключатися під час запуску
vtube_connect_on_launch_hint = Починати підключення разом із запуском Forge
vtube_connect_idle_hint = Ще не підключено - запустіть VTube Studio, потім підключіться.
vtube_connect_awaiting_title = Очікування підтвердження у VTube Studio...
vtube_connect_awaiting_hint = натисніть "Allow" у вікні плагіна
vtube_connect_btn_test = Перевірити підключення
vtube_connect_btn_connect = Підключити
vtube_connect_btn_authorizing = Авторизація...
vtube_connect_testing = Перевірка підключення
vtube_connect_connecting = Підключення
vtube_connect_test_successful = Перевірка успішна
vtube_connect_test_detail = VTube Studio { $version } · RTT { $rtt } мс · { $auth }
vtube_connect_test_authorized = уже авторизовано
vtube_connect_test_unauthorized = потрібне підтвердження
vtube_connect_test_failed = Перевірка не вдалася
vtube_connect_failed = Не вдалося підключитися
vtube_connect_error_title = Хибні налаштування
vtube_connect_error_invalid_port = Порт має бути числом від 1 до 65535
vtube_connect_error_unreachable = На цьому хості та порту ніхто не відповів. Чи запущено VTube Studio з увімкненим API?
vtube_connect_error_token_rejected = Збережений токен плагіна відхилено. Підтвердьте Forge ще раз.
vtube_connect_error_denied = Підтвердження відхилено у VTube Studio
vtube_connect_error_timeout = На вікно підтвердження не відповіли вчасно
vtube_connect_error_auth = Не вдалося автентифікувати плагін
vtube_connect_error_subscribe = Авторизовано, але не вдалося підписатися на події VTube Studio
vtube_connect_error_socket_closed = VTube Studio закрив з'єднання
vtube_connect_error_unresponsive = VTube Studio перестав відповідати на запити
vtube_connect_error_unknown = VTube Studio перервав спробу підключення
vtube_connect_settings_save_failed = Не вдалося зберегти налаштування підключення VTube Studio
obs_settings_disconnect_hint = Сеанс WebSocket буде закрито. Перемикання сцен і джерел не працюватиме, доки ти не підключишся знову.

## Common - row actions

common_duplicate = Дублювати

## MIDI - breadcrumb + header

midi_header_summary = { $devices ->
    [one] { $devices } пристрій
    [few] { $devices } пристрої
   *[other] { $devices } пристроїв
    } · { $mappings ->
    [one] { $mappings } зіставлення
    [few] { $mappings } зіставлення
   *[other] { $mappings } зіставлень
    }

## MIDI - hero

midi_hero_blurb = Прив'язуй ноти, контрольні зміни та фейдери з будь-якого MIDI-контролера до дій Forge.
midi_hero_enabled = Увімкнено
midi_hero_disabled = Вимкнено
midi_toggle_failed = Не вдалося змінити стан входу MIDI

## MIDI - devices

midi_section_devices = Пристрої
midi_device_port = вхід · порт { $index }
midi_device_offline = офлайн
midi_device_maps = { $count } зіставлень
midi_devices_empty = MIDI-пристроїв ще не виявлено
midi_rescan_ports = Пересканувати порти

## MIDI - input monitor

midi_section_monitor = Монітор входу
midi_monitor_empty = Очікування MIDI-сигналу...
midi_monitor_disabled = Вхід MIDI вимкнено - увімкни його вище, щоб бачити сигнали

## MIDI - mappings

midi_section_mappings = Зіставлення
midi_bindings_count = { $count ->
    [one] { $count } прив'язка
    [few] { $count } прив'язки
   *[other] { $count } прив'язок
    }
midi_value_any = будь-яке
midi_unassigned = Не призначено - натисни, щоб обрати дію
midi_mappings_empty = Зіставлень MIDI ще немає
midi_mappings_empty_device = Для цього пристрою зіставлень ще немає
midi_mapping_any_device = усі пристрої
midi_menu_edit = Редагувати зіставлення...
midi_add_learn = Навчання MIDI
midi_add_learn_kbd = слухати
midi_learn_prompt = Поворуши регулятором або натисни клавішу...
midi_learn_input_disabled = Вхід MIDI вимкнено - увімкни його вище, щоб захопити сигнал
midi_toast_error = MIDI: { $message }

## MIDI - вікно зіставлення

midi_modal_title_add = Призначити зіставлення MIDI
midi_modal_title_edit = Редагувати зіставлення MIDI
midi_modal_subtitle_captured = Сигнал захоплено - обери, що він має робити
midi_modal_section_signal = Сигнал
midi_modal_section_action = Дія для запуску
midi_modal_input = ВХІД
midi_modal_listening = слухаю...
midi_modal_type = ТИП
midi_modal_channel = КАНАЛ
midi_modal_channel_any = будь-який
midi_modal_device = ПРИСТРІЙ
midi_modal_device_any = Будь-який пристрій
midi_modal_relearn = Перенавчити
midi_modal_input_disabled = Вхід MIDI вимкнено - перенавчання недоступне, доки ти його не увімкнеш.
midi_modal_filter_actions = Фільтр дій...
midi_modal_actions_loading = Завантаження дій...
midi_modal_actions_empty = Жодна дія не відповідає фільтру
midi_modal_actions_none = Дій ще немає - спершу створи хоча б одну
midi_modal_hint_ready = Готово до збереження
midi_modal_hint_pick_action = Обери дію, щоб продовжити
midi_modal_add_mapping = Додати зіставлення
midi_modal_save_changes = Зберегти зміни

## MIDI - footer

midi_footer_left = { $devices ->
    [one] { $devices } пристрій підключено
    [few] { $devices } пристрої підключено
   *[other] { $devices } пристроїв підключено
    } · { $mappings ->
    [one] { $mappings } активне зіставлення
    [few] { $mappings } активні зіставлення
   *[other] { $mappings } активних зіставлень
    }
midi_engine_running = Рушій MIDI працює
midi_engine_stopped = Рушій MIDI зупинено

## Hotkeys - hero

hotkeys_hero_title = Гарячі клавіші
hotkeys_hero_blurb = Загальносистемні комбінації клавіш, які запускають дії, навіть коли Forge працює у фоні.
hotkeys_hero_enabled = Увімкнено
hotkeys_hero_disabled = Вимкнено
hotkeys_header_summary = { $count } активних · глобально
hotkeys_toggle_failed = Не вдалося змінити стан рушія гарячих клавіш
hotkeys_toggle_binding_failed = Не вдалося змінити стан прив'язки
hotkeys_toast_error = Операція з гарячою клавішею не вдалася: { $message }
hotkeys_toast_enable_partial = { $count } комбінацій не вдалося перереєструвати в системі
hotkeys_toast_bare_key_global = { $combo } без модифікатора перехоплюється в усій системі: поки forge працює, інші програми не отримують цю клавішу

## Hotkeys - stats

hotkeys_stat_bindings = Прив'язки
hotkeys_stat_bindings_hint = { $count } увімкнено
hotkeys_stat_global = Глобальні
hotkeys_stat_global_hint = на рівні ОС
hotkeys_stat_conflicts = Конфлікти
hotkeys_stat_conflicts_none = без збігів
hotkeys_stat_conflicts_hint = від запуску
hotkeys_stat_last_fired = Остання спрацювала
hotkeys_stat_last_fired_none = ще нічого

## Hotkeys - bindings

hotkeys_section_bindings = Прив'язки
hotkeys_section_hint = подвійний клік по комбінації, щоб перепризначити
hotkeys_unassigned = Не призначено - обери дію
hotkeys_scope_global = Глобальна
hotkeys_scope_app = Застосунок
hotkeys_scope_unregistered = Не зареєстровано
hotkeys_menu_edit = Редагувати...
hotkeys_menu_edit_press = Редагувати натискання...
hotkeys_menu_edit_release = Редагувати відпускання...
hotkeys_menu_add_press = Додати натискання...
hotkeys_menu_add_release = Додати відпускання...
hotkeys_menu_remove_press = Прибрати натискання
hotkeys_menu_remove_release = Прибрати відпускання
hotkeys_badge_hold = УТРИМАННЯ
hotkeys_badge_release_edge = ВІДПУСКАННЯ
hotkeys_row_stop = стоп: { $action }
hotkeys_row_stop_unassigned = стоп: не призначено
hotkeys_menu_unbind = Відв'язати
hotkeys_menu_reset_default = Скинути до типової
hotkeys_add_binding = Додати гарячу клавішу
hotkeys_add_binding_kbd = захопити
hotkeys_capture_prompt = Натисни комбінацію клавіш...

## Hotkeys - conflict

hotkeys_confirm_delete_title = Видалити гарячу клавішу?
hotkeys_confirm_delete_body = { $action } більше не запускатиметься цією комбінацією. Це не можна скасувати.
hotkeys_confirm_delete_body_unassigned = Ця комбінація не пов'язана з жодною дією. Це не можна скасувати.
hotkeys_confirm_delete_body_pair = Буде видалено обидві половини цієї комбінації - і натискання, і відпускання. Це не можна скасувати.
hotkeys_confirm_delete_press_half = Буде видалено лише половину натискання; половина відпускання лишить комбінацію зареєстрованою.
hotkeys_confirm_delete_release_half = Буде видалено лише половину відпускання, тож утримання починатиметься, але не спинятиметься.

hotkeys_conflict_title = Комбінацію вже прив'язано
hotkeys_conflict_body = вже прив'язана до { $holder }. Замінити наявну прив'язку чи скасувати?
hotkeys_conflict_holder_unassigned = непризначеної прив'язки
hotkeys_conflict_holder_clip = Звукова панель: { $name }
hotkeys_conflict_replace = Замінити

## Hotkeys - modal

hotkeys_modal_title_add = Прив'язати гарячу клавішу
hotkeys_modal_title_edit = Редагувати гарячу клавішу
hotkeys_app_modal_subtitle = Скорочення в застосунку
hotkeys_modal_subtitle_captured = Захоплена комбінація
hotkeys_modal_section_combo = Комбінація
hotkeys_modal_section_action = Дія
hotkeys_modal_section_edge = Спрацьовує на
hotkeys_modal_edge_press = Натискання
hotkeys_modal_edge_release = Відпускання
hotkeys_modal_edge_locked_press = Половину натискання для цієї комбінації вже прив'язано.
hotkeys_modal_edge_locked_release = Половину відпускання для цієї комбінації вже прив'язано.
hotkeys_modal_edge_queue_hint = Розведи половини по різних чергах - повільна дія натискання блокує відпускання в спільній послідовній черзі.
hotkeys_modal_recapture = Захопити знову
hotkeys_modal_filter_actions = Фільтр дій...
hotkeys_modal_actions_loading = Завантаження дій...
hotkeys_modal_actions_empty = Жодна дія не відповідає фільтру
hotkeys_modal_actions_none = Дій ще немає - спершу створи хоча б одну
hotkeys_modal_hint_ready = Готово до збереження
hotkeys_modal_hint_pick_action = Обери дію, щоб продовжити
hotkeys_modal_add_binding = Додати гарячу клавішу
hotkeys_modal_save_changes = Зберегти зміни

## Hotkeys - footer

hotkeys_footer_bindings = { $count } прив'язок
hotkeys_footer_listening = глобальний слухач активний
hotkeys_footer_stopped = глобальний слухач зупинено
hotkeys_footer_no_conflicts = Немає конфліктів
hotkeys_footer_conflicts = { $count } конфліктів
hotkeys_footer_hold_closed = утримання примусово закрито { $when }

## Discord - breadcrumb + hero

discord_hero_title = Discord
discord_hero_blurb = Публікація в канали сервера через вхідні вебхуки. Кожна прив'язка - це одна кінцева точка каналу, куди може писати дія.
discord_hero_webhooks = { $count } вебхуків
discord_hero_webhooks_sub = налаштовано
discord_header_summary = { $webhooks } вебхуків · { $actions } дій
discord_toast_error = Операція з вебхуком не вдалася: { $message }
discord_toast_saved = Вебхук збережено
discord_toast_deleted = Вебхук видалено
discord_toast_name_taken = Вебхук з назвою { $name } уже існує
discord_toast_test_sent = Тестове повідомлення надіслано
discord_toast_test_failed = Тестова публікація не вдалася: { $message }
discord_test_content = Тестове повідомлення з forge.

## Discord - stats

discord_stat_latency = Затримка вебхука
discord_stat_latency_value = { $ms } мс
discord_stat_latency_hint = p50 · останні { $count }
discord_stat_no_sends = ще нічого не надіслано
discord_stat_budget = Ліміт запитів
discord_stat_budget_value = { $used } / { $total }
discord_stat_budget_hint = використано бюджету
discord_stat_budget_unknown = ще немає даних бюджету
discord_stat_send = Остання відправка
discord_stat_send_ok = Доставлено
discord_stat_send_failed = Помилка
discord_stat_send_none = Відправок ще не було
discord_stat_errors = Помилки
discord_stat_errors_hint = за останні 60 хв

## Discord - channel bindings

discord_section_bindings = Прив'язки каналів
discord_section_bindings_count = { $count } прив'язано
discord_bindings_empty = Жодного каналу ще не прив'язано.
discord_binding_no_actions = сюди не пише жодна дія
discord_binding_action_count = сюди пише дій: { $count }
discord_add_binding = Прив'язати канал
discord_menu_edit = Редагувати...
discord_menu_test = Тест

## Discord - recent posts

discord_section_posts = Останні публікації
discord_posts_empty = Ще нічого не опубліковано.
discord_posts_never = ніколи
discord_post_kind_embed = вкладення
discord_post_kind_message = повідомлення

## Discord - webhook modal

discord_modal_title_add = Прив'язати канал
discord_modal_title_edit = Редагувати прив'язку
discord_modal_subtitle = Адреси вебхуків зберігаються зашифрованими і ніколи не потрапляють у логи.
discord_modal_name_label = Назва
discord_modal_name_placeholder = go-live
discord_modal_name_locked = Дії посилаються на цей вебхук за назвою, тому перейменувати його тут не можна.
discord_modal_url_label = URL вебхука
discord_modal_url_placeholder = https://discord.com/api/webhooks/...
discord_modal_url_hint = Скопіюй його з налаштувань каналу - Інтеграції - Вебхуки.
discord_modal_url_invalid = Це не URL вебхука Discord.
discord_modal_test = Тестова публікація
discord_modal_testing = Публікація...
discord_modal_add = Прив'язати канал
discord_modal_save_changes = Зберегти зміни

## Discord - delete + footer

discord_confirm_delete_title = Видалити вебхук?
discord_confirm_delete_body = Кінцеву точку буде вилучено зі сховища. Це не можна скасувати.
discord_confirm_delete_body_linked = До цього вебхука пише дій: { $count } - вони почнуть завершуватися помилкою. Це не можна скасувати.
discord_footer_webhooks = { $count } вебхуків
discord_footer_linked = { $count } пов'язаних дій
discord_footer_healthy = Надсилання працює
discord_footer_failing = Остання відправка не вдалася
discord_footer_idle = Ще нічого не надіслано

## Overlays - registry

overlays_breadcrumb_overlays = Оверлеї
overlays_header_summary = { $enabled }/{ $total } активних - віддається на :{ $port }
overlays_header_summary_stopped = { $enabled }/{ $total } активних - сервер зупинено
overlays_pane_title = Оверлеї
overlays_pane_loading = Завантаження оверлеїв...
overlays_pane_empty = Оверлеїв ще немає.
overlays_add_overlay = Новий оверлей
overlays_menu_rename = Перейменувати...
overlays_menu_copy_url = Копіювати URL
overlays_type_unavailable = Недоступний тип
overlays_type_unavailable_notice = Цей оверлей має тип { $kind }, якого немає в цій збірці. Запис лишається незмінним.

## Overlays - selection + URL

overlays_url_copy = Копіювати
overlays_url_not_served = Сервер зупинено, тому адреси в цього оверлея поки немає.
overlays_stage_select = Виберіть оверлей, щоб працювати з ним.
overlays_stage_empty = Створіть оверлей, щоб отримати адресу для браузерного джерела OBS.

## Overlays - add / rename form

overlays_form_title_create = Новий оверлей
overlays_form_title_rename = Перейменувати оверлей
overlays_form_subtitle_create = Адресу для OBS буде створено з цієї назви один раз, і надалі вона не змінюється.
overlays_form_subtitle_rename = Змінюється лише видима назва - адреса у ваших сценах OBS лишається тією самою.
overlays_form_name_label = Назва
overlays_form_name_placeholder = Сповіщення про підписки
overlays_form_type_label = Тип оверлея
overlays_form_type_locked = Тип оверлея фіксується в момент створення.
overlays_form_create = Створити оверлей
overlays_form_save = Зберегти назву

## Overlays - toasts + delete

overlays_toast_created = Оверлей створено
overlays_toast_renamed = Оверлей перейменовано
overlays_toast_deleted = Оверлей видалено
overlays_toast_missing = Цього оверлея вже немає у сховищі.
overlays_toast_unknown_type = У цій збірці немає такого типу оверлея.
overlays_toast_url_unavailable = Сервер зупинено, тому копіювати нічого.
overlays_toast_build_failed_pages =
    { $count ->
        [one] Не вдалося перебудувати { $count } сторінку оверлея - браузерне джерело може показувати стару версію. Подробиці в журналі.
        [few] Не вдалося перебудувати { $count } сторінки оверлеїв - браузерні джерела можуть показувати стару версію. Подробиці в журналі.
        [many] Не вдалося перебудувати { $count } сторінок оверлеїв - браузерні джерела можуть показувати стару версію. Подробиці в журналі.
       *[other] Не вдалося перебудувати { $count } сторінки оверлеїв - браузерні джерела можуть показувати стару версію. Подробиці в журналі.
    }
overlays_toast_build_failed_pass = Не вдалося перебудувати сторінки оверлеїв: { $reason }
overlays_toast_speech_failed = { $overlay } показано без мовлення: { $error }. Оберіть встановлений голос у полі Голос.
overlays_confirm_delete_title = Видалити оверлей?
overlays_confirm_delete_body = Будь-яке браузерне джерело OBS, що вказує на цей оверлей, перестане завантажуватися. Це не можна скасувати.

overlays_mode_design = Дизайн
overlays_mode_code = Код
overlays_code_placeholder = Цей файл ще не записано.
overlays_code_loading = Читаємо файл із теки оверлея...
overlays_code_meta = рядків: { $lines } - { $state }
overlays_code_state_saved = збережено
overlays_code_state_unsaved = не збережено
overlays_code_state_saving = зберігаємо
overlays_code_save = Зберегти
overlays_code_revert = Повернути
overlays_code_restore = Відновити
overlays_code_hint_generated = forge пише { $file } за вас. Збережіть правку - і файл стане вашим: його перестане перегенеровувати, а контролі дизайну, яким потрібна згенерована розмітка чи стилі, більше до нього не дійдуть.
overlays_code_hint_owned = Файл { $file } тепер ваш. forge його не переписує, тож контролі дизайну, яким потрібна згенерована розмітка чи стилі, більше до нього не доходять.
overlays_code_hint_predates_box = Файл { $file } ваш і створений до нових правил розміру браузерного джерела. Його старі правила позиції чи відступів можуть і далі діяти; Повернути відновить згенерований файл.
overlays_code_hint_missing = Запис вважає { $file } вашим, але файла немає в теці оверлея. Поверніть його, щоб forge записав файл знову.
overlays_code_footer_reload = Збереження перезавантажує кожне браузерне джерело із цим оверлеєм. Віддається за адресою
overlays_code_schema_notice = Тип цього оверлея змінився в новішій збірці. Ваші файли лишилися точно такими, як ви їх написали: { $files }. Конфігурація й далі доходить до тих прив'язок, які ви в них лишили.
overlays_code_revert_title = Повернути цей файл?
overlays_code_revert_body = forge запише постачену версію поверх вашої копії, і браузерні джерела перезавантажаться на неї. Ваші правки зникнуть.
overlays_code_revert_confirm = Повернути файл
overlays_code_discard_title = Відкинути незбережені правки?
overlays_code_discard_body = У цьому файлі є правки, які так і не збережено. Продовжити й втратити їх або лишитися та редагувати далі.
overlays_code_discard_confirm = Відкинути
overlays_code_discard_cancel = Редагувати далі
overlays_preview_label = Живий перегляд - { $width }x{ $height }
overlays_preview_scale_true = Реальний масштаб
overlays_preview_scale_zoom = Збільшено
overlays_preview_open = Відкрити в браузері
overlays_preview_open_failed = forge не зміг відкрити сторінку цього оверлея у браузері.
overlays_preview_browser_counts = Вкладка перегляду все одно отримує зразок, але вона не рахується як браузерне джерело.
overlays_preview_canvas_note = прозоро - браузерне джерело OBS
overlays_preview_approximate = Приблизний перегляд. Справжній оверлей малює браузерне джерело OBS на прозорому тлі.
overlays_preview_unavailable = У цій збірці немає типу оверлея для цього запису, тож переглядати нічого.
overlays_preview_audio_silent = Нічого не малює в OBS - лише відтворює звук.
overlays_preview_audio_needs_source = Додайте його як браузерне джерело OBS - звичайна вкладка блокує автовідтворення.
overlays_test_send = Переглянути вигляд
overlays_test_send_hint = Лише малює вигляд у перегляді та на підключених сторінках - без черги, звуку й озвучення.
overlays_test_sending = Готуємо зразок для прив'язаної події...
overlays_test_delivered = { $count ->
    [one] Зразок доставлено в { $count } браузерне джерело
    [few] Зразок доставлено в { $count } браузерні джерела
    [many] Зразок доставлено в { $count } браузерних джерел
   *[other] Зразок доставлено в { $count } браузерних джерел
}
overlays_test_preview_only = { $count ->
    [one] Підключена лише вкладка перегляду - відкрийте URL оверлея в OBS, щоб побачити його там
    [few] Підключено лише { $count } вкладки перегляду - відкрийте URL оверлея в OBS, щоб побачити його там
    [many] Підключено лише { $count } вкладок перегляду - відкрийте URL оверлея в OBS, щоб побачити його там
   *[other] Підключено лише { $count } вкладок перегляду - відкрийте URL оверлея в OBS, щоб побачити його там
}
overlays_test_undelivered = Сервер зупинено, тому жодне браузерне джерело не отримало цей зразок. Перегляд вище відпрацював локально.
overlays_test_no_browser_source = Сервер працює, але до цього оверлея не підключене жодне браузерне джерело. Перевірте URL оверлея в OBS і оновіть джерело.
overlays_bindings_heading = Прив'язки даних
overlays_bindings_intro = Позначте елемент { $marker }, і { $call } запише в нього текст. Колбек content отримує поля нижче за назвою - передайте кожне в { $writer }, щоб заповнити його елемент.
overlays_bindings_api_heading = API сторінки
overlays_bindings_fn_ready = Виконується один раз після завантаження сторінки, з налаштуваннями цього оверлея.
overlays_bindings_fn_content = Виконується на кожну доставку з полями вмісту. durationMs задано лише тоді, коли доставка змінює тривалість.
overlays_bindings_fn_set = Записує текст у кожен елемент, чий data-bind дорівнює name.
overlays_bindings_fn_show = Показує елементи, що відповідають target. З ms ховає їх знову через ms або після завершення мовлення, якщо воно довше.
overlays_bindings_fn_sound = Відтворює файл із теки цього оверлея.
overlays_bindings_content_heading = Поля вмісту
overlays_bindings_content_empty = Цей вигляд не оголошує полів вмісту. Колбек content однаково виконується на кожну доставку.
overlays_bindings_runtime_heading = Застосовує forge
overlays_bindings_runtime_accent = Колір акценту
overlays_bindings_runtime_font = Шрифт
overlays_bindings_runtime_text_size = Розмір тексту в px, без одиниці
overlays_bindings_runtime_position = Позиція: top або bottom
overlays_bindings_config_heading = Налаштування вигляду
overlays_bindings_config_empty = Цей вигляд не має власних налаштувань.
overlays_bindings_look_unavailable = У цій збірці немає типу оверлея для цього запису.
overlays_bindings_field_headline = Заголовок
overlays_bindings_field_subline = Підрядок
overlays_bindings_field_author = Автор
overlays_bindings_field_author_color = Колір імені
overlays_bindings_field_badges = Значки
overlays_bindings_field_message = Повідомлення
overlays_bindings_field_label = Підпис
overlays_bindings_field_value = Поточне значення
overlays_bindings_field_target = Ціль
overlays_bindings_field_placeholder = Текст-заглушка
overlays_bindings_field_icon = Іконка
overlays_bindings_field_slot = Слот
overlays_bindings_field_platform = Платформа
overlays_panel_section_content = Вміст
overlays_panel_section_style = Стиль
overlays_panel_section_source = Браузерне джерело
overlays_source_width = Ширина
overlays_source_height = Висота
overlays_source_size_hint = Пікселі. Задайте браузерному джерелу OBS саме такий розмір.
overlays_source_margin_top = Відступ згори
overlays_source_margin_right = Відступ справа
overlays_source_margin_bottom = Відступ знизу
overlays_source_margin_left = Відступ зліва
overlays_source_margin_hint = Місце для анімації, у відсотках від рамки. Вміст лишається всередині відступів; рух може їх використовувати.
overlays_sizing_notice_title = Розміри змінилися
overlays_sizing_notice_body = Цей оверлей тепер заповнює своє браузерне джерело OBS. Позицію прибрано, а старі відступи стали відсотковими.
overlays_sizing_notice_size = Задайте браузерному джерелу OBS розмір { $width } x { $height }.
overlays_sizing_notice_ack = Зрозуміло
overlays_obs_title = Браузерне джерело OBS
overlays_obs_url = URL
overlays_obs_width = Ширина
overlays_obs_height = Висота
overlays_obs_shutdown = Вимикати джерело, коли не видно
overlays_obs_refresh = Оновлювати браузер, коли сцена стає активною
overlays_obs_custom_css = Власний CSS
overlays_obs_custom_css_value = Типовий OBS
overlays_obs_audio = Керувати звуком через OBS
overlays_obs_on = Увімк.
overlays_obs_off = Вимк.
overlays_panel_section_behavior = Поведінка
overlays_panel_section_look = Вигляд
overlays_panel_section_audio = Звук
overlays_panel_section_display = Показ
overlays_panel_section_receiver = Приймач
overlays_look_hint = Зміна вигляду зберігає налаштування звуку, показу, стилю та браузерного джерела.
overlays_base_sound = Звук
overlays_base_speech = Текст для озвучення
overlays_base_speech_hint = Читається вголос, коли з'являється показ. Тут працюють змінні події, як-от %message%.
overlays_base_voice = Голос
overlays_base_voice_hint = Встановлений голос рушія; Типовий голос - псевдонім глядача або типовий голос.
overlays_voice_default = Типовий голос
overlays_voice_not_installed = { $voice } - не встановлено
overlays_base_duration = Тривалість
overlays_base_duration_hint = Залишається на екрані стільки часу або до кінця озвучення, якщо воно довше.
overlays_display_draws_nothing = Нічого не показує на екрані - лише відтворює свій звук і озвучення.
overlays_display_replace = Залишається на екрані, доки наступне оновлення не замінить його.
overlays_display_append = Кожне оновлення додається до того, що вже на екрані.
overlays_display_own_hide = Власний overlay.js ховає показ за своїм таймером, тож тривалість і утримання на час озвучення можуть не діяти.
overlays_motion_entrance = Поява
overlays_motion_text = Текст
overlays_motion_on_screen = На екрані
overlays_motion_exit = Зникнення
overlays_motion_total = Увесь показ
overlays_motion_ms = { $value } мс
overlays_motion_secs = { $value } с
overlays_motion_unit_letter = по літерах
overlays_motion_unit_word = по словах
overlays_motion_text_by = { $effect } { $unit }
overlays_motion_text_staggered = { $effect } { $unit }, крок { $stagger }
overlays_motion_replay = Повтор у браузері
overlays_motion_field_entrance = Поява
overlays_motion_field_entrance_ms = Тривалість появи
overlays_motion_field_text_effect = Ефект тексту
overlays_motion_field_text_effect_hint = Грає на тексті під час появи.
overlays_motion_field_text_unit = Ефект тексту по
overlays_motion_field_stagger_custom = Власний крок тексту
overlays_motion_field_stagger = Крок тексту
overlays_motion_field_exit = Зникнення
overlays_motion_field_exit_ms = Тривалість зникнення
overlays_motion_field_exit_hint = Грає після завершення тривалості; наступний показ чекає на нього.
overlays_motion_field_intensity = Інтенсивність руху
overlays_motion_opt_none = Немає
overlays_motion_opt_fade = Згасання
overlays_motion_opt_slide_up = Виїзд знизу
overlays_motion_opt_slide_down = Виїзд зверху
overlays_motion_opt_slide_left = Виїзд справа
overlays_motion_opt_slide_right = Виїзд зліва
overlays_motion_opt_pop = Виринання
overlays_motion_opt_wipe = Витирання
overlays_motion_opt_sparks = Іскри
overlays_motion_opt_assemble = Збирання
overlays_motion_opt_glow_burst = Спалах сяйва
overlays_motion_opt_dissolve = Розчинення
overlays_motion_opt_smoke = Дим
overlays_motion_opt_shatter = Розбиття
overlays_motion_opt_dust = Розсипання в пил
overlays_motion_opt_burst = Вибух
overlays_motion_opt_typewriter = Друкарська машинка
overlays_motion_opt_fly_in = Влітання
overlays_motion_opt_spin = Обертання
overlays_motion_opt_wave = Хвиля
overlays_motion_opt_bounce = Підстрибування
overlays_motion_opt_letter = За літерами
overlays_motion_opt_word = За словами
overlays_motion_opt_low = Низька
overlays_motion_opt_medium = Середня
overlays_motion_opt_high = Висока
overlays_motion_reveal_target_missing = Ваш { $file } не має елемента з id "{ $target }" - цей пресет не відтвориться.
overlays_motion_text_targets_missing = Ваш { $file } не має тексту з data-bind - ефект тексту не відтвориться.
overlays_motion_reveal_call_missing = Ваш { $file } ніколи не викликає forge.show - пресети не відтворяться.
overlays_motion_retired_rules = Ваш { $file } досі стилізує data-animation - ці правила більше не діють.
overlays_queue_label = Черга показів
overlays_queue_waiting = У черзі: { $count }
overlays_queue_clear = Очистити чергу
overlays_show_fire = Повний тестовий показ
overlays_show_fire_hint = Ставить у чергу зразковий показ зі звуком і озвученням, як це зробила б дія.
overlays_show_fire_hint_arrival = Надсилає зразковий вміст зі звуком і озвученням, як це зробила б дія.
overlays_show_fire_hint_latest = Показує зразкові значення 5 секунд, потім оверлей повертається до поточного значення.
overlays_latest_section = Поточне значення
overlays_latest_from = з { $platform }
overlays_latest_empty = Поки нічого
overlays_latest_empty_hint = Оверлей показує заглушку, доки не надійде значення.
overlays_latest_reset = Скинути
overlays_latest_resetting = Скидання...
overlays_latest_reset_hint = Очищає цей слот для всіх платформ. Оверлей показує заглушку до наступного значення.
overlays_latest_confirm_title = Скинути останнє значення?
overlays_latest_confirm_body = Збережене значення буде видалено для всіх платформ, і кожен оверлей цього слота покаже заглушку. Це не можна скасувати.
latest_slot_donation = Останній донат
latest_slot_now_playing = Зараз грає
latest_platform_all = Усі платформи
overlays_show_sending = Надсилаємо показ...
overlays_show_delivered = { $count ->
    [one] Показ надіслано в { $count } браузерне джерело
    [few] Показ надіслано в { $count } браузерні джерела
    [many] Показ надіслано в { $count } браузерних джерел
   *[other] Показ надіслано в { $count } браузерних джерел
}
overlays_show_preview_only = { $count ->
    [one] Показ надіслано, але підключена лише { $count } вкладка перегляду
    [few] Показ надіслано, але підключені лише { $count } вкладки перегляду
    [many] Показ надіслано, але підключено лише { $count } вкладок перегляду
   *[other] Показ надіслано, але підключено лише { $count } вкладок перегляду
}
overlays_show_no_page = Показ надіслано, але жодна сторінка не підключена - нічого не видно й не чутно.
overlays_show_ended = Показ завершився.
overlays_show_cleared = Показ прибрано з черги до того, як він відбувся.
overlays_show_withdrawn = Показ відкликано до того, як він відбувся.
overlays_receiver_toggle = Приймає TTS і звуки саундборду forge
overlays_receiver_hint_on = Тут звучать озвучення й саундборд, спрямовані на оверлей. Увімкнення на іншому оверлеї вимикає його тут.
overlays_receiver_hint_off = Увімкнення надсилає сюди озвучення й саундборд forge і перемикає Налаштування - Аудіо - Маршрутизація на оверлей. Звук приймає лише один оверлей.
overlays_receiver_save_failed = Не вдалося зберегти приймач звуку: { $reason }
overlays_panel_unavailable = У цій збірці немає типу оверлея для цього запису, тож налаштовувати нічого.
overlays_panel_choice_empty = Немає доступних варіантів.
overlays_sound_none = Без звуку
overlays_sound_custom_file = Власний файл: { $name }
overlays_sound_adopting = Копіюю звук до бібліотеки...
overlays_sound_issue_unknown_clip = Обраного звуку вже немає у звуковій панелі, тож цей оверлей нічого не програє.
overlays_sound_issue_outside_library = Обраний звук ще не скопійовано до бібліотеки, тож цей оверлей нічого не програє. Відкрий Звукову панель і додай його до бібліотеки.
overlays_sound_issue_bytes_missing = Копія обраного звуку зникла з бібліотеки, тож цей оверлей нічого не програє.
overlays_sound_issue_kind_mismatch = Обраний звук збережено як { $format }, а це не аудіо, тож цей оверлей нічого не програє.
overlays_sound_issue_lookup_failed = forge не зміг прочитати те, на що вказує це поле: { $reason }
overlays_icon_issue_unknown_glyph = Обраної іконки немає в наборі цієї збірки, тож цей оверлей не покаже іконку.
overlays_icon_issue_unknown_image = Обраного зображення вже немає в бібліотеці, тож цей оверлей не покаже іконку.
overlays_icon_issue_bytes_missing = Копія обраного зображення зникла з бібліотеки, тож цей оверлей не покаже іконку.
overlays_icon_issue_kind_mismatch = Обране зображення збережено як { $format }, а це не зображення, тож цей оверлей не покаже іконку.
overlays_icon_none = Без іконки
overlays_icon_none_desc = Сповіщення покаже лише текст.
overlays_icon_group_none = Без іконки
overlays_icon_group_imported = Імпортовані
overlays_icon_import = Імпортувати зображення...
overlays_icon_import_desc = Обери файл на цьому комп'ютері ({ $formats }).
overlays_icon_import_not_image = { $file } збережено як { $format }, а це не зображення, тож воно не може бути іконкою.
overlays_icon_importing = Імпортую зображення до бібліотеки...
overlays_icon_file_filter = Зображення
overlays_icon_picker_title = Обери іконку
overlays_icon_picker_subtitle = Добірний гліф бере акцент оверлея; імпорт показується як є.
overlays_icon_picker_hint = Друкуй для пошуку, стрілки - рух, Enter - вибір, Esc - закрити.
overlays_icon_picker_search = Пошук іконок
overlays_icon_category_support = Підтримка
overlays_icon_category_celebration = Святкування
overlays_icon_category_community = Спільнота
overlays_icon_category_stream = Стрім
overlays_icon_category_play = Ігри
overlays_icon_category_goals = Цілі
overlays_icon_category_nature = Природа
overlays_panel_override_notice = Ці файли тепер ваші, тож вони ніколи не перегенеровуються: { $files }. Зміни дизайну, яким потрібна згенерована розмітка чи стилі, більше до них не доходять.
config_form_choice_placeholder = Не задано
config_form_auto_placeholder = авто
config_form_choice_any = Будь-яке
config_form_choice_search = Пошук або власне значення...
config_form_choice_empty = Нема з чого обрати - введіть значення
config_form_choice_custom = використати введене
config_form_duration_unit_ms = мілісекунди
config_form_duration_unit_seconds = секунди
config_form_duration_unit_minutes = хвилини
config_form_duration_unit_hours = години
config_field_label_allow_everyone_pings = Дозволити @everyone / @here
config_field_label_allow_role_pings = Дозволити пінг ролей
config_field_label_ban_type = Тип бану
config_field_label_case_sensitive = З урахуванням регістру
config_field_label_channel = Канал
config_field_label_channel_filter = Фільтр за каналом
config_field_label_command_phrase = Фраза команди
config_field_label_controller = Контролер
config_field_label_controller_filter = Фільтр за контролером
config_field_label_decision_status = Статус рішення
config_field_label_device = Пристрій
config_field_label_device_filter = Фільтр за пристроєм
config_field_label_donation_service = Донат-сервіс
config_field_label_donation_service_filter = Фільтр за донат-сервісом
config_field_label_direction = Напрямок
config_field_label_direction_filter = Фільтр за напрямком
config_field_label_event_name = Назва події
config_field_label_from_user = Від кого (логін)
config_field_label_guest_state = Стан гостя (порожнє = усі)
config_field_label_hotkey_combo = Комбінація клавіш
config_field_label_interval_minutes = Інтервал (хвилини)
config_field_label_match_text = Текст для збігу
config_field_label_max_bits = Макс. бітсів (-1 = без меж)
config_field_label_min_bits = Мін. бітсів
config_field_label_min_chat_messages = Мін. повідомлень у чаті
config_field_label_min_donation_cents = Мін. донат (центи)
config_field_label_min_level = Мін. рівень (1-5)
config_field_label_note = Нота
config_field_label_note_filter = Фільтр за нотою
config_field_label_only_first_time_chatters = Лише ті, хто пише вперше
config_field_label_only_while_live = Лише в ефірі
config_field_label_program = Програма
config_field_label_program_filter = Фільтр за програмою
config_field_label_redemption_status = Статус викупу
config_field_label_reward = Нагорода
config_field_label_reward_id = Нагорода (порожнє = усі)
config_field_label_reward_title = Назва нагороди (якщо нагороду не вибрано)
config_field_label_scene = Сцена
config_field_label_scene_filter = Фільтр за сценою
config_field_label_source_name = Назва джерела
config_field_label_source_filter = Фільтр за джерелом
config_field_label_use_regex = Регулярний вираз

## Оверлеї - привʼязка до події

overlays_wire_open = Показати на подію...
overlays_wire_empty = До цього оверлея ще ніщо не надсилає.
overlays_wire_picker_title = Показати цей оверлей на подію
overlays_wire_picker_lead = для
overlays_wire_picker_count = { $count ->
    [one] { $count } подія
    [few] { $count } події
    [many] { $count } подій
   *[other] { $count } подій
}
overlays_wire_picker_search = Пошук подій
overlays_wire_picker_hint = Введіть текст для пошуку - стрілки для переходу - Enter для вибору
overlays_wire_scope_suggested = Рекомендовані
overlays_wire_confirm_title = Показувати { $overlay } на цю подію
overlays_wire_checking = Перевіряємо, що вже надсилає до цього оверлея...
overlays_wire_section_creates = Буде створено
overlays_wire_section_wording = У трансляції це читається як
overlays_wire_record_action = Дія
overlays_wire_record_trigger = Тригер
overlays_wire_record_queue = Черга
overlays_wire_wording_empty = Цей тип оверлея не має тексту, який можна заповнити.
overlays_wire_create = Створити
overlays_wire_back = Назад
overlays_wire_open_action = Відкрити дію
overlays_wire_already_body = Цей оверлей уже показується на цю подію. Відкрийте дію замість створення другої.
overlays_wire_refused_overlay = Цей тип оверлея не приймає привʼязку до події.
overlays_wire_refused_trigger = Ця подія не оголошує нічого, що оверлей міг би показати.
overlays_wire_toast_created = Оверлей привʼязано до події
overlays_wire_toast_already = Цей оверлей уже показується на цю подію.
overlays_wire_toast_incomplete = Привʼязка обірвалася на півдорозі. Записано: { $landed }.
overlays_wire_toast_missing = У цій збірці більше немає цього типу оверлея або цієї події.
overlays_wire_landed_action = дію
overlays_wire_landed_trigger = тригер
overlays_wire_landed_link = звʼязок між ними
overlays_wire_landed_nothing = нічого
overlays_wire_queue_not_live = Черга { $queue } ще не працює. Перезапустіть forge, щоб це сповіщення програлося.
overlays_wire_fires_on = Спрацьовує на { $triggers }
overlays_wire_fires_untriggered = Дії надсилають до цього оверлея, але жодна подія їх ще не запускає.
overlays_wire_action_disabled = вимкнено
overlays_wire_overlay_off = Цей оверлей вимкнено, тож поки він вимкнений, нічого не показується.
overlays_confirm_delete_feeds = { $count ->
    [one] До нього надсилає { $count } дія.
    [few] До нього надсилають { $count } дії.
    [many] До нього надсилають { $count } дій.
   *[other] До нього надсилають { $count } дій.
}
nav_item_integrations = Інтеграції
integrations_breadcrumb = Інтеграції
integrations_title = Інтеграції
integrations_subtitle = Увімкніть те, чим користуєтеся. Вимкнена інтеграція не запускається взагалі - жодних з'єднань, жодної фонової роботи. Входи й налаштування зберігаються.
integration_enabled_of_total = {" "}з { $total } увімкнено
integration_category_streaming = Стрімінг
integration_category_streaming_blurb = Чат і події з платформ, на яких ви стрімите
integration_category_donations = Донати
integration_category_donations_blurb = Підтримка від глядачів через донат-сервіси
integration_category_apps = Застосунки
integration_category_apps_blurb = Настільні застосунки, якими Forge керує через локальний WebSocket
integration_category_community = Спільнота
integration_category_community_blurb = Публікації для вашої спільноти поза стрімом
integration_category_audio = Аудіо
integration_category_audio_blurb = Мовлення і звук, вбудовані у Forge
integration_category_controls = Керування
integration_category_controls_blurb = Фізичні та системні входи, що запускають тригери
integration_category_tools = Інструменти
integration_category_tools_blurb = Браузерні джерела та локальний API, вбудовані у Forge
integration_category_on_count = { $on } з { $total } увімкнено
integration_twitch_description = Чат, EventSub, бали каналу, біти, рейди
integration_youtube_description = Живий чат, суперчати, спонсорства
integration_kick_description = Чат, підписки, події каналу
integration_donatello_description = Донати через API панелі Donatello
integration_monobank_description = Поповнення банки monobank
integration_obs_description = Сцени, джерела, аудіо, запис
integration_vtube_description = Гарячі клавіші моделі, вирази, предмети
integration_discord_description = Анонси початку стріму та дзеркала подій через вебхуки
integration_midi_description = Ноти, CC і фейдери з контролерів
integration_hotkey_description = Загальносистемні комбінації, навіть коли Forge приховано
integration_core_tts_description = Голоси, псевдоніми, черга мовлення
integration_core_soundboard_description = Кліпи з падів, гарячих клавіш, дій
integration_core_overlays_description = Браузерні джерела для сповіщень і віджетів
integration_core_server_description = Локальний API для оверлеїв та інструментів
integration_core_open = Відкрити
integration_core_always_on = Завжди увімкнено - частина Forge
integration_status_disabled = Вимкнено
integration_status_starting = Запуск
integration_status_stopping = Зупинка
integration_status_failed = Не вдалося запустити
integration_status_active = Активно
integration_status_connected = Підключено
integration_status_not_connected = Не підключено
integration_status_built_in = Вбудовано
integration_refs_none = Ще не використовується
integration_refs_actions = { $count ->
    [one] { $count } дія
    [few] { $count } дії
    [many] { $count } дій
   *[other] { $count } дії
}
integration_refs_triggers = { $count ->
    [one] { $count } тригер
    [few] { $count } тригери
    [many] { $count } тригерів
   *[other] { $count } тригера
}
integration_refs_used_by = використовують { $parts }
integration_retry = Повторити
integration_retry_failed = Не вдалося перезапустити { $name }
integration_connect_failed = Не вдалося підключити { $name }
integration_disconnect_failed = Не вдалося відключити { $name }
integration_failed_title = { $name } не вдалося запустити
integration_failed_lead = Повторна спроба запустить її знову. Вхід і налаштування зберігаються.
integration_failed_retrying = Запускається знову
integration_failed_sign_in = Увійти знову
integration_connect = Підключити
integration_disconnect = Відключити
integration_settings_tooltip = Налаштування { $name }
integration_kick_disclaimer_short = Чат через неофіційний WebSocket - може зламатися
integration_kick_disclaimer = Чат читається через неофіційний WebSocket Kick і може перестати працювати без попередження.
integration_activity_discord = { $count ->
    [one] { $count } вебхук готовий
    [few] { $count } вебхуки готові
    [many] { $count } вебхуків готово
   *[other] { $count } вебхука готово
}
integration_activity_midi = { $count ->
    [one] { $count } пристрій
    [few] { $count } пристрої
    [many] { $count } пристроїв
   *[other] { $count } пристрою
}
integration_activity_hotkey = { $count ->
    [one] { $count } прив'язка
    [few] { $count } прив'язки
    [many] { $count } прив'язок
   *[other] { $count } прив'язки
}
integration_disable_title = Вимкнути { $name }?
integration_disable_subtitle = Зупиняє зараз. Вхід і налаштування зберігаються.
integration_disable_lead_prefix = { $name } використовують{" "}
integration_disable_lead_and = {" "}і{" "}
integration_disable_lead_suffix = . Поки інтеграцію вимкнено:
integration_disable_fails_prefix = Кроки { $name } завершуються помилкою{" "}
integration_disable_reason = інтеграцію вимкнено
integration_disable_fails_suffix = {" "}в історії запусків
integration_disable_triggers_inactive = Її тригери неактивні й не спрацюють
integration_disable_kept = Ваш вхід і налаштування залишаються - увімкніть знову будь-коли
integration_disable_used_by = Використовують
integration_disable_more = +{ $count } ще
integration_disable_confirm = Вимкнути { $name }
integration_disable_esc_hint = Esc - скасувати
integration_welcome_title = Ласкаво просимо до Forge
integration_welcome_lead = Увімкніть те, з чим ви стрімите. Усе вимкнене лишається вимкненим - без з'єднань і фонової роботи.
integration_welcome_lead_later = Змінити це можна будь-коли в Інтеграціях.
integration_welcome_nothing_on = Ще нічого не увімкнено
integration_welcome_some_on = Увімкнено { $count } - { $names }
integration_welcome_progress = { $on } / { $total }
integration_welcome_built_in = Синтез мовлення, Звукова панель, Оверлеї та WebSocket-сервер вбудовані - завжди доступні.
integration_welcome_off = Вимкнено - увімкніть, щоб налаштувати
integration_welcome_on = Увімкнено
integration_welcome_skip = Пропустити
integration_welcome_enter = Увійти до Forge
integration_welcome_turn_one_on = Увімкніть щось, щоб продовжити
integration_disabled_title = { $name } вимкнено
integration_disabled_lead = Не працює - без з'єднання і фонової роботи. Вхід і налаштування зберігаються.
integration_disabled_all = Усі інтеграції
integration_disabled_enable = Увімкнути { $name }
integration_disabled_affected = Зачеплено, поки вимкнено
integration_disabled_unused = { $name } ще ніде не використовується.
integration_disabled_steps_fail = Кроки { $name } завершаться помилкою
integration_disabled_triggers_idle = { $count ->
    [one] { $count } тригер не спрацює
    [few] { $count } тригери не спрацюють
    [many] { $count } тригерів не спрацюють
   *[other] { $count } тригера не спрацюють
}
integration_disabled_kept = Збережено до повторного увімкнення
integration_disabled_nothing_kept = Ще нічого не налаштовано.
integration_kept_sign_in = Вхід збережено
integration_kept_credentials = Облікові дані збережено
integration_inactive_short = неактивний
integration_inactive_badge = неактивний · { $name } вимкнено
integration_inactive_category_badge = неактивний · жодної інтеграції з розділу { $category } не увімкнено
donation_anonymous_donor = Анонім
donation_settings_title = Токен { $service }
donation_token_label = API-токен
donation_token_placeholder = Вставте токен сюди
donation_token_status_saved = Токен збережено
donation_token_status_missing = Токен не збережено
donation_token_status_unknown = Перевіряємо збережений токен
donation_token_hint_new = Скопіюйте токен зі свого акаунта { $service } і вставте сюди.
donation_token_hint_replace = Токен збережено. Вставте новий, щоб замінити його.
donation_token_page_link = Відкрити сторінку токена
donation_token_encrypted_note = Зберігається зашифровано на цьому комп'ютері
donation_token_check = Перевірити токен
donation_token_checking = Перевіряємо...
donation_token_save = Зберегти токен
donation_token_saving = Зберігаємо...
donation_token_remove = Видалити токен
donation_token_remove_title = Видалити токен?
donation_token_remove_body = Донати перестануть надходити, доки ви знову не вставите токен.
donation_token_remove_body_monobank = Донати перестануть надходити, а вибір банки скинеться, доки ви знову не вставите токен.
donation_token_works = Токен працює.
donation_token_works_as = Токен працює. Акаунт: { $name }
donation_token_saved = Токен збережено. Стежимо за донатами.
donation_token_saved_as = Токен збережено для { $name }. Стежимо за донатами.
donation_token_removed = Токен видалено.
donation_jar_label = Банка
donation_jar_hint = Завантажте свої банки й оберіть ту, що приймає донати.
donation_jar_loading = Завантажуємо банки...
donation_jar_load = Завантажити банки
donation_jar_loading_button = Завантаження...
donation_jar_save = Обрати цю банку
donation_jar_saved = Збережено. Стежимо за банкою { $jar }.
donation_jar_none = На цьому акаунті немає банок. Спершу створіть банку в застосунку monobank.
donation_jar_untitled = Банка без назви
donation_jar_goal = ціль { $goal }
donation_poll_label = Перевіряти нові донати кожні
donation_poll_hint = forge питає Donatello про нові донати кожні { $seconds } с.
donation_poll_seconds = { $seconds } с
donation_poll_minutes = { $minutes } хв
donation_token_problem_missing = Спершу вставте токен.
donation_token_problem_malformed = Це не схоже на токен. Скопіюйте його ще раз без зайвих символів.
donation_token_problem_rejected = { $service } не прийняв токен. Створіть новий і вставте сюди.
donation_token_problem_profile_incomplete = Профіль Donatello налаштовано не до кінця. Завершіть налаштування в панелі Donatello і спробуйте ще раз.
donation_token_problem_no_jar = Спершу оберіть банку.
donation_token_problem_jar_not_found = Цієї банки немає серед банок цього токена. Завантажте банки ще раз.
donation_token_problem_cooling_down = { $service } дозволяє один запит на хвилину. Спробуйте ще раз через { $seconds } с.
donation_token_problem_cooling_down_soon = { $service } просить зачекати. Спробуйте ще раз за хвилину.
donation_token_problem_unreachable = Не вдалося зв'язатися з { $service }. Перевірте з'єднання з інтернетом.
donation_token_problem_service_error = { $service } відповів помилкою (HTTP { $status }). Спробуйте пізніше.
donation_token_problem_unreadable = { $service } надіслав відповідь, яку forge не зміг прочитати. Спробуйте пізніше.
donation_token_problem_storage = Не вдалося зберегти токен на цьому комп'ютері.
donation_token_problem_local = Щось пішло не так усередині forge. Перезапустіть його і спробуйте ще раз.

## Shell - leaving a screen with unsaved changes
shell_unsaved_leave_title = Піти з незбереженими змінами?
shell_unsaved_leave_body = На цьому екрані є правки, які так і не збережено. Збережіть їх перед виходом, відкиньте або залишіться й редагуйте далі.
shell_unsaved_leave_cancel = Редагувати далі
shell_unsaved_leave_save = Зберегти й вийти
shell_unsaved_leave_discard = Відкинути
