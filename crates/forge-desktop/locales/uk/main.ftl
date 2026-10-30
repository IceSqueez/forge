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
home_hero_export = Експортувати
home_export_full = Повна резервна копія
home_export_pack = Пакет...
pack_export_title = Експорт пакета
pack_export_subtitle = Поділіться діями разом з усім, що їм потрібно
pack_export_search_placeholder = Фільтр дій
pack_export_actions_label = Дії
pack_export_selected_count = Вибрано: { $count }
pack_export_loading = Завантаження дій...
pack_export_load_failed = Не вдалося завантажити дії: { $error }
pack_export_no_actions = Поки що немає дій для експорту
pack_export_no_matches = Жодна дія не відповідає фільтру
pack_export_name_label = Назва
pack_export_name_placeholder = Назва пакета
pack_export_name_required = Обов'язково
pack_export_author_label = Автор
pack_export_author_placeholder = Необов'язково
pack_export_description_label = Опис
pack_export_description_placeholder = Необов'язково
pack_export_summary_label = Вміст пакета
pack_export_summary_empty = Виберіть хоча б одну дію
pack_export_summary_resolving = Збираємо залежності...
pack_export_summary_failed = Не вдалося зібрати залежності: { $error }
pack_export_kind_actions = Дії
pack_export_kind_triggers = Тригери
pack_export_kind_queues = Черги
pack_export_kind_scripts = Скрипти
pack_export_kind_globals = Глобальні змінні
pack_export_kind_clips = Звукові кліпи
pack_export_kind_overlays = Оверлеї
pack_export_kind_media = Медіафайли
pack_export_integrations_label = Потрібні інтеграції
pack_export_integrations_none = Немає
pack_export_unresolved = Посилань на відсутні елементи: { $count }
pack_export_names_not_followed = Назви всередині скриптів і текстових полів не відстежуються, тож пакет може не містити того, на що вони посилаються
pack_export_confirm_btn = Експортувати
pack_export_dialog_filter = Пакети forge
pack_export_done = Пакет збережено до { $path }
pack_export_failed = Не вдалося експортувати пакет: { $error }
pack_import_title = Імпорт пакета
pack_import_subtitle = Перегляньте, що додасть пакет, перш ніж щось буде записано
pack_import_items_label = Вміст пакета
pack_import_item_count = Елементів: { $count }
pack_import_empty = У цьому пакеті немає елементів
pack_import_pack_label = Пакет
pack_import_author = автор: { $author }
pack_import_author_unknown = Автора не вказано
pack_import_conflict_none = новий
pack_import_conflict_id = той самий id
pack_import_conflict_name = та сама назва
pack_import_conflict_archived = в архіві
pack_import_resolution_skip = Пропустити
pack_import_resolution_replace = Замінити
pack_import_resolution_keep_both = Залишити обидва
pack_import_warnings_label = Попередження
pack_import_warning_integration = Потрібна інтеграція { $integration }, якої немає в цьому forge
pack_import_warning_unknown_kind = Використовує «{ $kind }», якого цей forge не знає
pack_import_warning_unresolved = Посилається на { $namespace } «{ $value }», якого немає в пакеті
pack_import_warning_external = Посилається на { $namespace } «{ $value }» поза пакетом
pack_import_warning_clip_silent = Звуковий кліп «{ $clip }» не містить звукового файлу
pack_import_runs_code_notice = Пакет містить вміст, що виконує код. Перегляньте його перед імпортом:
pack_import_runs_script = Скрипт «{ $name }»
pack_import_runs_http = HTTP-крок у дії «{ $action }»
pack_import_runs_script_step = Крок скрипту в дії «{ $action }»
pack_import_runs_overlay = Оверлей «{ $overlay }»: { $file }
pack_import_limit_names = Назви всередині скриптів і текстових полів не відстежуються, тож вони можуть посилатися на те, чого в пакеті немає
pack_import_snapshot_note = Деякі елементи замінять наявні, тому спершу буде створено страхувальну резервну копію ваших даних
pack_import_confirm_btn = Імпортувати
pack_import_applying = Імпорт...
pack_import_done = Пакет імпортовано: записано { $written }, пропущено { $skipped }
pack_import_done_restart = Пакет імпортовано: записано { $written }, пропущено { $skipped }. Перезапустіть forge, щоб застосувати решту
pack_import_reveal_snapshot = Показати копію
pack_import_stale = Дані forge змінилися після побудови перегляду. Перегляньте оновлений список та імпортуйте ще раз
pack_import_failed = Нічого не імпортовано: { $error }
pack_import_rollback_incomplete = Імпорт не вдався, і його не вдалося повністю скасувати: { $error }. Відновіть страхувальну резервну копію з { $folder }
pack_import_rollback_incomplete_no_snapshot = Імпорт не вдався, і його не вдалося повністю скасувати: { $error }
home_export_setup = Конфігурація...
setup_export_title = Експорт конфігурації
setup_export_subtitle = Перенесіть свою конфігурацію на інший комп'ютер
setup_export_carries = Файл конфігурації містить усе, що ви налаштували: дії, тригери, черги, скрипти, глобальні змінні, звукові кліпи, оверлеї, медіа, параметри мовлення, голосові псевдоніми, гарячі клавіші та налаштування.
setup_export_never_logins = Він ніколи не містить входів. Підключіть облікові записи знову на новому комп'ютері.
setup_export_never_history = Він ніколи не містить історії: події, запуски, чат, глядачі, донати й бани лишаються тут.
setup_export_never_machine = Він ніколи не містить налаштувань цього комп'ютера: аудіопристроїв, тек, розташування вікна, мережевого сервера та довіри HTTP. Задайте їх знову на новому комп'ютері.
setup_export_continue = Вибрати файл...
setup_export_dialog_filter = Конфігурації forge
setup_export_done = Конфігурацію збережено до { $path }
setup_export_failed = Не вдалося експортувати конфігурацію: { $error }
setup_confirm_title = Імпортувати цю конфігурацію?
setup_confirm_subtitle = Поточну конфігурацію буде замінено конфігурацією з файлу
setup_confirm_part_voice_aliases = Голосові псевдоніми
setup_confirm_part_filter_rules = Правила фільтрів мовлення
setup_confirm_part_clip_hotkeys = Гарячі клавіші кліпів
setup_confirm_part_settings = Налаштування
setup_confirm_replaces = УСЮ поточну конфігурацію буде замінено. Те, чого немає у файлі, буде видалено.
setup_confirm_keeps = Входи, історія та налаштування цього комп'ютера лишаються без змін.
setup_confirm_snapshot = Спершу буде створено страхувальну резервну копію поточних даних.
setup_confirm_btn = Замінити конфігурацію
setup_confirm_btn_busy = Імпорт...
setup_import_failed = Нічого не змінено: { $error }
setup_import_done = Конфігурацію імпортовано
setup_import_done_restart = Конфігурацію імпортовано. Перезапустіть forge, щоб застосувати все
setup_import_restart = Перезапустити
setup_import_cleanup_failed = Конфігурацію імпортовано, але { $count } залишкових файлів не вдалося видалити. Подробиці в журналі
setup_import_settings_skipped = Не імпортовано налаштувань: { $count } (невідомих: { $undeclared }, локальних для цього пристрою: { $machine_local }, з нечитабельним значенням: { $undecodable }). Подробиці в журналі
setup_import_rollback_incomplete = Імпорт конфігурації не вдався, і його не вдалося повністю скасувати: { $error }. Відновіть страхувальну резервну копію { $snapshot }
home_import_dialog_filter = Файли forge
home_hero_new_action = Нова дія
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
footer_connected = { $connected }/{ $total } підключено
footer_uptime = працює { $uptime }
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
settings_appearance_accent_label = Колір акценту
settings_appearance_accent_hint = Основне підсвічування в усьому інтерфейсі
settings_accent_persist_failed = Не вдалося зберегти колір акценту
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

settings_language_subtitle = Оберіть, якою мовою Forge говоритиме з вами
settings_region_region = Регіон
settings_region_date_format = Формат дати
settings_region_time_format = Формат часу
settings_region_first_day = Перший день тижня
settings_region_default = Як у регіоні ({ $value })
settings_region_persist_failed = Не вдалося зберегти налаштування регіону
message_catalog_conflict = Частину вбудованих текстів не вдалося завантажити: { $problems }

## Налаштування → бічна навігація

settings_page_title = Налаштування
settings_nav_group_preferences = ПЕРЕВАГИ
settings_nav_group_engine = РУШІЙ
settings_nav_group_about = ПРО ПРОГРАМУ
settings_nav_appearance = Зовнішній вигляд
settings_nav_general = Загальні
settings_stay_awake_label = Не засинати, поки forge працює
settings_stay_awake_hint = Екран і система не переходять у сон, поки forge працює
settings_stay_awake_persist_failed = Не вдалося застосувати налаштування «Не засинати»
settings_nav_language_region = Мова та регіон
settings_nav_shortcuts = Скорочення
settings_nav_notifications = Сповіщення
settings_notifications_subtitle = Про що Forge вам повідомляє
settings_notifications_action_failure_label = Показувати системні сповіщення про помилки
settings_notifications_action_failure_hint = Сповіщення ОС, коли дія не виконалася
settings_notifications_connection_loss_label = Сигнал при розриві з'єднання
settings_notifications_connection_loss_hint = Системний звук, коли платформа чи застосунок втрачає з'єднання
settings_notifications_scheduled_backup_failure_label = Сповіщати, коли запланована копія не вдалася
settings_notifications_scheduled_backup_failure_hint = Сповіщення ОС при першій невдачі запланованої резервної копії
settings_notifications_persist_failed = Не вдалося зберегти налаштування сповіщень
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

settings_about_title = Про програму
settings_about_system_title = Інформація про систему
settings_about_system_hint = Вставте це у звіт про помилку. Тут ніколи немає імен комп'ютера чи користувача, шляхів або мережевих адрес.
settings_about_collecting = Збираємо інформацію про систему...
settings_about_unknown = невідомо
settings_about_field_forge = forge
settings_about_field_build = Збірка
settings_about_field_os = ОС
settings_about_field_kernel = Ядро
settings_about_field_cpu = Процесор
settings_about_field_memory = Пам'ять
settings_about_field_gpu = Відеокарта
settings_about_field_gpu_backend = Графічний бекенд
settings_about_field_displays = Дисплеї
settings_about_field_audio_outputs = Аудіовиходи
settings_about_field_data_dir = Розмір теки даних
settings_about_report_problem = Повідомити про проблему
crash_notice = Минулого разу forge несподівано закрилася.
crash_notice_report = Повідомити
crash_notice_dismiss = Закрити
problem_report_title = Повідомити про проблему
problem_report_crash_title = Повідомити про збій
problem_report_lead = forge збереже діагностичний пакет у теку й відкриє в браузері нову задачу на GitHub із заповненими версією forge та відомостями про систему. Нічого не надсилається, доки ви самі не опублікуєте задачу.
problem_report_crash_lead = forge збереже подробиці збою та діагностичний пакет у теку й відкриє в браузері нову задачу на GitHub із заповненим описом збою. Нічого не надсилається, доки ви самі не опублікуєте задачу.
problem_report_files_note = Тека відкриється, коли ви продовжите, - перетягніть її файли в задачу. Про діагностичний пакет у ній:
problem_report_issue_note = Сторінка задачі відкриється з таким заповненим текстом:
problem_report_confirm = Зберегти файли й відкрити GitHub
problem_report_saved = Файли звіту збережено. Перетягніть їх із відкритої теки в задачу на GitHub.
problem_report_prepare_failed = Не вдалося підготувати звіт: { $error }
problem_report_save_failed = Не вдалося зберегти файли звіту: { $error }
settings_version_license = Відкритий код · MIT OR Apache-2.0
settings_version_check_updates = Перевірити оновлення
settings_version_checking = Перевірка оновлень...
settings_version_up_to_date = У вас остання версія forge - перевірено о { $time }
settings_version_update_found = Знайдено оновлення - v{ $version }
settings_version_update_downloading = Знайдено оновлення - v{ $version }, завантажується
settings_version_check_failed = Не вдалося перевірити оновлення
settings_version_open_releases = Відкрити сторінку релізів

## Сповіщення про оновлення

app_menu_quit = Вийти з forge
update_toast_available = Доступна forge { $version }
update_toast_open_action = Завантажити
update_toast_ready = forge { $version } готова. Вона встановиться, коли ви закриєте програму.
update_toast_restart_action = Перезапустити й оновити
update_toast_preparing = Підготовка оновлення до forge { $version }...
update_toast_waiting_for_transfer = Очікування завершення резервної копії чи перенесення - forge { $version } встановиться після цього. Закрийте ще раз, щоб вийти без оновлення.
update_toast_restarting = Перезапуск для встановлення forge { $version }
update_toast_download_failed = Не вдалося завантажити forge { $version }.
update_toast_unverified = Не вдалося перевірити forge { $version }, тому її не встановлено.
update_toast_handoff_failed = Не вдалося підготувати forge { $version } до встановлення.
update_toast_retry_action = Повторити

system_notice_action_failed_title = Дія не виконалася
system_notice_action_failed_body = { $action } не завершилася.
system_notice_action_failed_body_reason = { $action } - { $reason }
system_notice_backup_failed_title = Запланована копія не вдалася
system_notice_backup_failed_body = forge не зміг записати заплановану копію: { $reason }.
system_notice_backup_failed_open = Відкрити налаштування
system_notice_actions_folded_title = Не виконалися й інші дії
system_notice_actions_folded_body = { $count ->
    [one] Ще { $count } дія не виконалася. Перегляньте журнал подій.
    [few] Ще { $count } дії не виконалися. Перегляньте журнал подій.
    [many] Ще { $count } дій не виконалися. Перегляньте журнал подій.
   *[other] Ще { $count } дій не виконалися. Перегляньте журнал подій.
}
update_leftover_install = Інша копія forge встановлена для всіх користувачів. Видаліть її, щоб ваші дані використовувала лише одна копія.
update_leftover_open_action = Відкрити встановлені програми
script_retired_ids_notice = Скрипти досі використовують перейменовані ідентифікатори - оновіть їх: { $uses }
script_retired_ids_entry = { $script } ({ $ids })
script_retired_ids_pair = { $retired } -> { $current }
script_retired_ids_open = Відкрити скрипти
update_report_updated = forge оновлено до { $version }
update_report_rolled_back = forge { $failed } { $reason }. Повернуто { $restored }.
update_report_rolled_back_data_restored = forge { $failed } { $reason }. Повернуто { $restored } і дані.
update_report_rolled_back_data_failed = forge { $failed } { $reason }. Повернуто { $restored }, дані не відновлено.
update_report_rollback_failed = forge { $failed } { $reason }, і повернути попередню версію не вдалося. Перевстановіть forge.
update_report_not_applied = forge { $version } не встановлено: { $reason }.
update_reason_crashed = аварійно закрилася під час запуску
update_reason_no_health_signal = не запустилася вчасно
update_reason_launch_failed = не запустилася
update_reason_parent_still_running = forge не закрилася вчасно
update_reason_installer_failed = інсталятор повідомив про помилку
update_reason_installer_timed_out = інсталятор не завершився вчасно
update_reason_malformed_payload = завантажене оновлення пошкоджене
update_reason_file_operation = не вдалося замінити файли forge
update_reason_interrupted = оновлення було перервано

## Налаштування → панель сховища

settings_storage_section_title = Сховище та резервні копії
settings_storage_db_path_label = База даних
settings_storage_backup_label = Резервна копія
settings_storage_backup_btn = Резервна копія зараз
settings_storage_backup_hint = Зберігає всю теку даних - базу, ключ облікових даних, медіа, оверлеї, голоси й ресурси - одним файлом, зашифрованим паролем, у теці резервних копій.
settings_storage_transfer_busy_hint = Триває інший експорт, імпорт або відновлення. Спершу завершіть або скасуйте його.
settings_storage_backup_btn_busy = Створюю копію...
settings_storage_backup_done = Копію збережено: { $path }
settings_storage_backup_failed = Не вдалося створити копію: { $error }
settings_storage_backup_reveal = Показати
settings_storage_passphrase_title = Шифрування копії
settings_storage_passphrase_subtitle = Пароль запитується для кожної копії окремо
settings_storage_passphrase_warning = forge ніколи не зберігає цей пароль. Якщо ви його забудете, ніхто не зможе його відновити, і ця копія стане непридатною.
settings_storage_passphrase_label = Пароль
settings_storage_passphrase_placeholder = Введіть пароль
settings_storage_passphrase_hint = Щонайменше { $min } символів
settings_storage_passphrase_too_short = Потрібно щонайменше { $min } символів
settings_storage_passphrase_confirm_label = Повторіть пароль
settings_storage_passphrase_confirm_placeholder = Введіть його ще раз
settings_storage_passphrase_mismatch = Паролі не збігаються
settings_storage_passphrase_confirm_btn = Створити копію
settings_storage_restore_btn = Відновити...
settings_storage_restore_btn_busy = Підготовка...
settings_storage_backup_scheduled_hint = Одразу створює заплановану копію з її вмістом і ключем пароля розкладу - пароль не запитується.
backup_schedule_label = Розклад резервних копій
backup_schedule_change_btn = Змінити пароль
backup_schedule_run_done = Резервну копію збережено в теці резервних копій
backup_schedule_status_off = Вимкнено - остання копія: { $last }
backup_schedule_status_paused = Призупинено, доки очікує відновлення
backup_schedule_status_running = Створюю копію...
backup_schedule_status_scheduled = Остання копія: { $last } - наступна о { $next }
backup_schedule_status_last = Остання копія: { $last }
backup_schedule_status_failing = { $attempts ->
    [one] Збій з { $since }: { $reason }
    [few] Збій з { $since }, спроб: { $attempts }: { $reason }
   *[other] Збій з { $since }, спроб: { $attempts }: { $reason }
}
backup_schedule_failure_destination_missing = теки резервних копій немає
backup_schedule_failure_destination_not_writable = до теки резервних копій не можна записати
backup_schedule_failure_disk_full = диск заповнено
backup_schedule_failure_inside_scope = тека резервних копій лежить у теці даних
backup_schedule_failure_key_missing = немає ключа пароля розкладу
backup_schedule_failure_key_unreadable = збережений ключ пароля розкладу пошкоджено, задайте пароль знову
backup_schedule_failure_snapshot_failed = не вдалося записати копію
backup_schedule_failure_state_not_saved = копію записано, але не вдалося зберегти її стан
backup_schedule_failure_interrupted = створення копії перервано
backup_schedule_storage_used_label = Зайняте місце
backup_schedule_storage_used_hint = Розмір на диску кожної частини теки даних.
backup_schedule_storage_used_unknown = -
backup_schedule_storage_used_database = База даних
backup_schedule_storage_used_media = Медіа
backup_schedule_storage_used_voices = Голоси
backup_schedule_storage_used_overlays = Файли оверлеїв
backup_schedule_storage_used_assets = Файли дій
backup_schedule_storage_used_logs = Журнали
backup_schedule_storage_used_backups = Резервні копії
backup_schedule_storage_used_destination = Тека резервних копій
backup_schedule_interval_label = Інтервал
backup_schedule_interval_hint = Хвилин між запланованими копіями, { $min }-{ $max }.
backup_schedule_keep_newest_label = Зберігання копій
backup_schedule_keep_newest_hint = Скільки найновіших запланованих копій зберігати, { $min }-{ $max }.
backup_schedule_keep_daily_label = Щоденні копії
backup_schedule_keep_daily_hint = Додатково зберігати по одній запланованій копії на день стільки днів, { $min }-{ $max }; 0 вимикає.
backup_schedule_range_error = Введіть ціле число від { $min } до { $max }
backup_schedule_unit_minutes = { $count ->
    [one] хвилина
    [few] хвилини
   *[other] хвилин
}
backup_schedule_unit_backups = { $count ->
    [one] копія
    [few] копії
   *[other] копій
}
backup_schedule_unit_days = { $count ->
    [one] день
    [few] дні
   *[other] днів
}
backup_schedule_contents_label = Вміст
backup_schedule_contents_hint = Невибрані частини не потрапляють у заплановані копії; під час відновлення вони лишаються такими, як на цьому комп'ютері.
backup_schedule_category_database = База даних і ключ облікових даних
backup_schedule_category_database_reason = завжди включено
backup_schedule_category_media = Медіа
backup_schedule_category_voices = Голоси
backup_schedule_category_overlays = Файли оверлеїв
backup_schedule_category_assets = Файли дій
backup_schedule_folder_label = Тека резервних копій
backup_schedule_folder_hint = Сюди записуються заплановані копії. Якщо теки немає, створюється лише тека за замовчуванням.
backup_schedule_folder_choose = Вибрати...
backup_schedule_folder_reset = За замовчуванням
backup_schedule_folder_failed = Не вдалося відкрити вибір теки: { $error }
backup_schedule_persist_failed_reason = Не вдалося зберегти налаштування розкладу копій: { $reason }
backup_schedule_load_failed = Не вдалося завантажити налаштування розкладу копій, тому редагування вимкнено, щоб їх зберегти: { $reason }
backup_schedule_key_failed = Не вдалося встановити пароль розкладу: { $error }
backup_schedule_setup_title = Захист запланованих копій
backup_schedule_setup_subtitle = Запитується один раз - далі заплановані копії створюються самі
backup_schedule_setup_warning = forge забуває цей пароль і зберігає лише заблокований ключ відновлення. Відновлення запланованої копії запитає його; якщо ви його забудете, ніхто не зможе відновити ці копії.
backup_schedule_setup_confirm_btn = Увімкнути розклад
backup_schedule_change_title = Зміна пароля розкладу
backup_schedule_change_subtitle = Нові заплановані копії використовуватимуть новий пароль
backup_schedule_change_warning = Копії, створені до цієї зміни, і далі відкриваються лише старим паролем. Зберігайте його, доки ці копії потрібні.
backup_schedule_change_confirm_btn = Змінити пароль
backup_schedule_change_current_label = Поточний пароль
backup_schedule_change_current_placeholder = Введіть поточний пароль
backup_schedule_change_new_label = Новий пароль
backup_schedule_change_wrong_current = Поточний пароль неправильний
backup_schedule_change_done = Пароль розкладу змінено
safety_copies_label = Страхувальні копії
safety_copies_hint = Копії, які forge робить сам перед відновленням, імпортом пакета чи налаштувань і оновленням, щоб можна було повернутися назад.
safety_copies_mode_keep_newest = Зберігати найновіші
safety_copies_mode_delete_after_days = Видаляти через
safety_copies_mode_never = Ніколи не видаляти
safety_copies_unit_copies = { $count ->
    [one] копію
    [few] копії
   *[other] копій
}
safety_copies_unit_days = { $count ->
    [one] день
    [few] дні
   *[other] днів
}
safety_copies_usage_label = Збережені страхувальні копії
safety_copies_usage = { $count ->
    [one] { $count } копія - { $size }
    [few] { $count } копії - { $size }
   *[other] { $count } копій - { $size }
}
safety_copies_usage_unknown = -
safety_copies_clean_up_btn = Очистити зараз
safety_copies_clean_up_btn_busy = Очищую...
safety_copies_clean_up_confirm_title = Видалити всі страхувальні копії?
safety_copies_clean_up_confirm_body = Усі страхувальні копії в теці даних буде видалено, крім тих, що ще потрібні оновленню, яке триває, або незавершеному відновленню. Резервні копії не зачіпаються. Повернутися до стану перед попереднім відновленням, імпортом чи оновленням уже не вийде.
safety_copies_clean_up_confirm_action = Видалити страхувальні копії
safety_copies_saved = Налаштування страхувальних копій збережено
safety_copies_nothing_to_remove = Страхувальних копій для видалення немає - копії, що ще потрібні оновленню, яке триває, або незавершеному відновленню, зберігаються
safety_copies_freed = { $count ->
    [one] Видалено { $count } страхувальну копію - звільнено { $size }
    [few] Видалено { $count } страхувальні копії - звільнено { $size }
   *[other] Видалено { $count } страхувальних копій - звільнено { $size }
}
safety_copies_not_deleted = { $count ->
    [one] Не вдалося видалити страхувальну копію ({ $name }): { $reason }
    [few] Не вдалося видалити { $count } страхувальні копії, перша { $name }: { $reason }
   *[other] Не вдалося видалити { $count } страхувальних копій, перша { $name }: { $reason }
}
safety_copies_change_deferred = Налаштування збережено - триває резервне копіювання чи перенесення, тож страхувальні копії буде скорочено під час наступного запуску forge.
safety_copies_clean_up_deferred = Триває резервне копіювання чи відновлення - спробуйте ще раз, коли воно завершиться.
safety_copies_change_failed = Не вдалося застосувати налаштування страхувальних копій: { $error }
safety_copies_clean_up_failed = Не вдалося очистити страхувальні копії: { $error }
restore_dialog_filter = Резервні копії forge
restore_passphrase_title = Розблокування копії
restore_passphrase_subtitle = Введіть пароль, з яким створено цю копію
restore_passphrase_label = Пароль
restore_passphrase_placeholder = Пароль копії
restore_passphrase_hint = Нічого не зміниться, доки копію не розблоковано і ви не підтвердите відновлення.
restore_passphrase_submit = Розблокувати
restore_passphrase_submit_busy = Розблокування...
restore_confirm_title = Відновити цю копію?
restore_confirm_subtitle = Усі поточні дані буде замінено даними з копії
restore_confirm_created = Створено
restore_confirm_forge_version = Версія forge
restore_confirm_schema = Схема бази даних
restore_confirm_snapshot_kept = Поточні дані збережуться як знімок перед відновленням у теці резервних копій.
restore_confirm_closes = forge зараз закриється і сам запуститься знову, щоб завершити відновлення.
restore_confirm_relogin = Облікові записи можуть попросити увійти знову, якщо їхні входи оновлювалися після створення цієї копії.
restore_confirm_one_machine = Ніколи не використовуйте ті самі відновлені входи на двох комп'ютерах одночасно - це зламає ланцюжок входу на обох.
restore_confirm_btn = Відновити і перезапустити
restore_confirm_btn_busy = Планування...
restore_confirm_subtitle_scoped = Частини, що є в цій копії, буде замінено
restore_confirm_scope_replaced = Замінюється з копії: { $parts }
restore_confirm_scope_kept = Немає в цій копії, залишаються як є: { $parts }
restore_close_cancelled = Відновлення скасовано - поточні дані не змінено.
restore_close_yielded_to_update = Встановлюється оновлення, тому відновлення скасовано. Перезапустіть forge, щоб завершити оновлення, а потім відновіть копію.
restore_close_cancel_failed = Відновлення скасовано, але його файли не вдалося прибрати: { $error }
restore_problem_not_a_bundle = Цей файл не є резервною копією forge.
restore_problem_unknown_kind = Файл містить невідомий цій версії тип даних forge.
restore_problem_newer_format = Копію створено у форматі { $found }; ця версія forge читає формати до { $supported }. Спершу оновіть forge.
restore_problem_newer_schema = Копія містить схему бази даних { $found }; ця версія forge підтримує схеми до { $supported }. Спершу оновіть forge.
restore_problem_schema_too_old = База даних копії (стара схема { $found }) застара для відновлення; спершу відкрийте її у forge { $release }.
restore_problem_unrecognized_database = Базу даних цієї копії forge не може відкрити.
restore_problem_unknown_encryption = Копію зашифровано невідомим цій версії forge способом. Спершу оновіть forge.
restore_problem_passphrase_required = Ця копія зашифрована. Введіть її пароль, щоб відновити.
restore_problem_passphrase_too_short = Пароль має містити щонайменше { $min } символів.
restore_problem_wrong_passphrase = Неправильний пароль
restore_problem_damaged = Файл пошкоджений або неповний, його неможливо відновити.
restore_problem_missing_key = Копія містить входи, але не містить ключа для їх розшифрування, тож її неможливо відновити.
restore_problem_update_in_progress = Оновлення готове або встановлюється. Перезапустіть forge, щоб завершити оновлення, а потім відновіть копію.
restore_problem_already_pending = Відновлення вже очікує наступного запуску.
restore_problem_unavailable = Зараз неможливо запланувати відновлення. Спробуйте ще раз за мить.
restore_problem_failed = Щось пішло не так: { $error }
restore_problem_kind_mismatch = Тип файлу: { $found }; очікувався: { $expected }.
restore_problem_newer_documents = Файл використовує формат документів { $found }; ця версія forge читає формати до { $supported }. Спершу оновіть forge.
bundle_kind_full = повна резервна копія
bundle_kind_pack = пакет
bundle_kind_setup = налаштування
import_busy_pack_title = Читання пакета
import_busy_pack_detail = Перевірка пакета щодо ваших даних forge.
import_busy_setup_title = Читання налаштувань
import_busy_setup_detail = Розпакування налаштувань, щоб показати, що вони замінять.
import_busy_restore_title = Читання копії
import_busy_restore_detail = Перевірка резервної копії перед відновленням.
import_busy_already = Інший імпорт ще триває. Дочекайтеся його завершення й спробуйте ще раз.
transfer_busy_already = Ще триває інша резервна копія, експорт, імпорт або відновлення. Дочекайтеся завершення і спробуйте знову.
restore_boot_applied = Копію відновлено. Попередні дані збережено в { $path }
restore_boot_rolled_back = Відновлення не застосовано, дані залишилися без змін: { $reason }
restore_boot_deferred = Відновлення ще очікує і не було застосоване: { $reason }
restore_rollback_interrupted = його було перервано
restore_rollback_staging_missing = підготовлені файли копії зникли
restore_rollback_marker_unreadable = його службовий файл не вдалося прочитати
restore_deferral_update = спершу встановлюється оновлення. Відновлення застосується під час одного з наступних запусків.
restore_deferral_instance_lock = можливо, запущено ще один forge. Закрийте всі вікна forge і запустіть його знову.
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


## Налаштування → панель гарячих клавіш (ярлики)

settings_shortcuts_subtitle = Ці скорочення працюють лише тоді, коли вікно forge у фокусі. Системні комбінації - у розділі «Хоткеї».
settings_shortcuts_action_nav_home = Перейти на Хаб
settings_shortcuts_action_nav_live_chat = Відкрити живий чат
settings_shortcuts_action_nav_actions = Відкрити дії
settings_shortcuts_action_nav_triggers = Відкрити тригери
settings_shortcuts_action_nav_twitch = Відкрити Twitch
settings_shortcuts_action_nav_settings = Відкрити налаштування
settings_shortcuts_action_save = Зберегти
settings_shortcuts_action_new_action = Нова дія
settings_shortcuts_action_toggle_live_chat = Перемкнути живий чат
settings_shortcuts_action_toggle_event_feed = Перемкнути стрічку подій
settings_shortcuts_action_run_script = Запустити скрипт
settings_shortcuts_unbound = Не призначено
settings_shortcuts_load_failed = Не вдалося завантажити збережені скорочення, тому редагування вимкнено, щоб їх зберегти: { $reason }
settings_shortcuts_capture_prompt = Натисніть скорочення... Esc для скасування
settings_shortcuts_rebind = Змінити
settings_shortcuts_reset = Скинути
settings_shortcuts_reset_all = Скинути все до типових
settings_shortcuts_fixed_section = Незмінні клавіші
settings_shortcuts_fixed_enter = Підтвердити форму чи діалог
settings_shortcuts_fixed_escape = Закрити модальне вікно чи скасувати захоплення
settings_shortcuts_fixed_note = Ці клавіші вбудовані, їх не можна перепризначити.
settings_hold_ceiling_section = Глобальні гарячі клавіші
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
settings_ws_origins_unreadable = Збережений список додаткових дозволених джерел не читається, тому сервер приймає лише сторінки, які роздає сам. Редагування заблоковано, щоб не перезаписати збережений список.
settings_ws_load_failed = Не вдалося завантажити налаштування WebSocket-сервера: { $reason }
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
settings_ws_auth_require_http_label = Вимагати токен для HTTP-файлів оверлеїв
settings_ws_auth_require_http_sublabel = URL оверлеїв містять токен оверлеїв першим сегментом шляху: /overlays/<token>/...
settings_ws_overlay_token_title = Токен оверлеїв
settings_ws_overlay_token_subtitle = Лише читання - дозволяє браузерному джерелу завантажувати файли оверлеїв, і нічого більше
settings_ws_overlay_token_regen_title = Оновити токен оверлеїв?
settings_ws_overlay_token_regen_message = Зміниться кожна URL оверлею, а старі одразу перестануть завантажуватися. Скопіюйте кожну URL оверлею заново в її браузерне джерело в OBS.
settings_ws_overlay_token_restore_failed = Не вдалося оновити токен, а попередній токен оверлеїв відновити не вдалося. Тепер активний новий токен - скопіюйте кожну URL оверлею заново в її браузерне джерело в OBS.
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
actions_group_ungrouped = Без групи
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
actions_modal_concurrent_label = Обходити ліміт черги
actions_modal_concurrent_desc = Запускається одразу, ніби черга дозволяє паралельні запуски. Кроки самої дії все одно йдуть по черзі.
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
action_editor_export = Експортувати як пакет...
action_editor_edit = Редагувати
action_editor_menu_delete = Видалити дію
action_editor_add_trigger = Додати тригер
action_editor_add_step = Додати крок
action_editor_health_unknown_var = Використовує %{ $name }%, якої не надає жоден тригер і не створює жоден попередній крок
action_editor_health_produced_later = Використовує %{ $name }%, але її створює лише пізніший крок
action_editor_health_isolated_sibling = Використовує %{ $name }%, яку створює сусідній крок, що виконується ізольовано й не ділиться нею
action_editor_health_some_triggers = Використовує %{ $name }%, яку надають лише деякі тригери цієї дії
action_editor_health_unverifiable_var = Використовує %{ $name }%, якої не оголошує жоден тригер; один із тригерів цієї дії не оголошує своїх змінних, тож це неможливо перевірити
action_editor_health_last_run_failed = Останній запуск завершився помилкою: { $message }
action_editor_health_disabled = Інтеграцію вимкнено
action_editor_health_integration_disabled = { $name } вимкнено
action_editor_step_disabled_prefix = { $name } вимкнено - цей крок завершується з причиною{" "}
action_editor_health_failed = Інтеграція не запустилася
action_editor_health_integration_failed = { $name } не вдалося запустити
action_editor_health_date_format_not_datetime = Крок форматує %{ $name }% як дату, але ця змінна не є датою й часом, тому шаблон ігнорується і значення показується як є
action_editor_health_invalid_date_pattern = Шаблон дати в %{ $name }:{ $pattern }% недійсний: { $reason }
action_editor_health_invalid_step_config = Налаштування кроку недійсні: { $message }
action_editor_health_undecodable_step = Вкладений крок { $position } неможливо прочитати; він зберігається без змін і пропускається під час запуску дії
step_config_required = Обов'язкове поле
step_config_not_a_number = Має бути числом
step_config_between = Має бути від { $min } до { $max }
step_config_at_least = Має бути не менше ніж { $min }
step_config_at_most = Має бути не більше ніж { $max }
step_config_length_between = Довжина має бути від { $min } до { $max } { $max ->
    [one] символу
    [few] символів
   *[other] символів
}
step_config_length_at_least = Щонайменше { $min } { $min ->
    [one] символ
    [few] символи
   *[other] символів
}
step_config_length_at_most = Щонайбільше { $max } { $max ->
    [one] символ
    [few] символи
   *[other] символів
}
step_config_out_of_range = Значення поза допустимими межами
step_config_count_between = Має містити від { $min } до { $max } { $max ->
    [one] елемента
    [few] елементів
   *[other] елементів
}
step_config_count_at_least = Щонайменше { $min } { $min ->
    [one] елемент
    [few] елементи
   *[other] елементів
}
step_config_count_at_most = Щонайбільше { $max } { $max ->
    [one] елемент
    [few] елементи
   *[other] елементів
}
step_config_not_text = Має бути текстом
step_config_any_of_required = Заповніть хоча б одне з полів: { $fields }
step_config_invalid_choice = Допустимі значення: { $choices }
step_config_invalid_format = Неправильний формат: { $detail }
step_config_field_problem = { $problem } ({ $field })
action_editor_step_failed_notice = { $name } не вдалося запустити - цей крок завершуватиметься помилкою, доки інтеграція не запрацює
action_editor_health_ok = Все гаразд: усі посилання резолвляться, останній запуск успішний
action_editor_health_warn = Статичне попередження
action_editor_health_error = Помилка
action_editor_overlay_order_warning = Рядки оверлею можуть надійти не за порядком
action_editor_overlay_order_warning_hint = Ця дія надсилає вміст в оверлей, чутливий до порядку, а її запуски можуть перекриватися: черга виконує кілька дій одночасно або дія обходить ліміт черги. Черга з паралельністю 1 і дія, що дотримується ліміту черги, зберігають порядок.
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
action_editor_sub_actions_count = { $count ->
    [one] { $count } крок
    [few] { $count } кроки
   *[other] { $count } кроків
}
actions_sub_file_browse = Огляд
actions_sub_files_root_label = Тека файлів
actions_sub_files_root_hint = Кроки з файлами приймають лише шляхи відносно цієї теки - абсолютні шляхи та ".." відхиляються.
actions_sub_files_root_open = Відкрити теку
actions_sub_files_root_create_failed = Не вдалося створити теку файлів: { $reason }
actions_sub_datetime_pick = Обрати
actions_sub_datetime_now = Зараз
actions_sub_datetime_set = Встановити
actions_var_picker_open = Змінна
actions_var_picker_search = Пошук змінних
actions_var_picker_empty = На цьому кроці немає доступних змінних
actions_var_picker_no_format = Без формату
actions_var_picker_group_trigger = Тригер
actions_var_picker_group_some_triggers = Лише деякі тригери
actions_var_picker_group_steps = Попередні кроки
actions_var_picker_group_loop = Цикл
action_editor_sub_count = { $count ->
    [one] { $count } крок
    [few] { $count } кроки
   *[other] { $count } кроків
}
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
action_editor_test_run_empty = Немає кроків для виконання.
action_editor_test_run_default_error = Виконання кроку не вдалося
action_editor_test_run_status_queued = у черзі
action_editor_test_run_status_running = виконується...
action_editor_test_run_status_failed = помилка
action_editor_test_run_status_skipped = пропущено
action_editor_step_wait_ms = { $ms } мс
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
picker_favorites_load_failed = Не вдалося завантажити обране, тому позначення вимкнено, щоб його зберегти
picker_favorites_save_failed = Не вдалося зберегти обране: { $reason }
action_editor_picker_add_sub_title = Додати крок
action_editor_picker_inserting_into = Вставлення у
action_editor_picker_sub_count = · { $count ->
    [one] { $count } крок
    [few] { $count } кроки
   *[other] { $count } кроків
}
action_editor_picker_footer_hint = Додається з розумними типовими значеннями - редагуйте на місці
action_editor_picker_search = Пошук серед { $count } кроків...
action_editor_picker_fires = Запускає
action_editor_picker_available_count = · { $count } доступно
action_editor_trigger_picker_footer_hint = Створює новий тригер обраного типу та прив'язує його

## Редактор дій - назви типів кроків

action_editor_kind_send_chat = Надіслати в чат
action_editor_kind_set_global = Встановити змінну
action_editor_kind_delay = Затримка
action_editor_kind_log = Журнал
action_editor_kind_play_sound = Відтворити звук
action_editor_clip_missing = Звук не знайдено
action_editor_kind_speak = Синтез мовлення
action_editor_kind_read_file = Читати файл
action_editor_kind_random_int = Випадкове число
action_editor_kind_incr_global = Збільшити змінну
action_editor_kind_run_script = Запустити скрипт
action_editor_persisted_note = (збережено)
action_editor_kind_sub_action = Крок

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
triggers_timer_hint_interval = Інтервал: як часто спрацьовує, від 1 хвилини до 24 годин. Перше спрацювання - через один інтервал після збереження або після початку ефіру.
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

settings_scripting_title = Скрипти та HTTP
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
settings_scripting_core_allow_local_label = Дозволити localhost / приватні IP
settings_scripting_core_allow_local_description = Вимикає захист від SSRF для дії HTTP-запиту. Вмикайте лише для локальної розробки.
settings_scripting_ssrf_warning = УВАГА - вимикає захист від SSRF. Вмикайте лише для локальної розробки.
settings_scripting_http_action_section = HTTP-дія
settings_scripting_range_error = Дозволено: { $min } - { $max }
settings_scripting_persist_failed = Не вдалося зберегти налаштування скриптів та HTTP. Після перезапуску повернеться попереднє значення.
settings_scripting_domains_load_failed = Не вдалося завантажити дозволені домени: { $reason }
settings_scripting_domains_save_failed = Не вдалося зберегти дозволені домени: { $reason }

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
actions_error_missing_action = дію не знайдено
actions_error_missing_chain = список кроків не знайдено
actions_duplicate_name_suffix = (копія)
actions_rename_taken = Назву '{ $name }' вже зайнято
actions_deleted_toast = Видалено '{ $name }'
actions_delete_title = Видалити дію?
actions_delete_body = Це видалить дію та всі її кроки й тригери.
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
tts_header_no_engine_ready = Немає готових рушіїв
tts_header_engines_ready = Готово рушіїв: { $count }
tts_engines_rail_sub = { $kind } · { $count ->
    [one] { $count } голос
    [few] { $count } голоси
    [many] { $count } голосів
   *[other] { $count } голосів
}

## TTS Engines - класи рушіїв і типи голосів

tts_engine_class_local = локальний
tts_engine_class_system = системний
tts_engine_class_cloud = хмарний
tts_voice_kind_male = чоловічий
tts_voice_kind_female = жіночий
tts_voice_kind_neural = нейронний
tts_voice_kind_standard = стандартний

## TTS Engines - заголовок деталей

tts_engines_detail_sub = { $kind } рушій · { $count ->
    [one] { $count } голос
    [few] { $count } голоси
    [many] { $count } голосів
   *[other] { $count } голосів
}
tts_engines_detail_sub_region = { $kind } рушій · { $region } · { $count ->
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
tts_engines_persist_disabled_failed_reason = Не вдалося зберегти, які рушії увімкнено: { $reason }
tts_engines_persist_params_failed = Не вдалося зберегти параметри рушія: { $reason }
tts_engines_disabled_load_failed = Не вдалося завантажити, які рушії увімкнено: { $reason }
tts_engines_params_load_failed = Не вдалося завантажити параметри рушія: { $reason }

## TTS Filters - колонка пайплайну

tts_filters_pipeline_intro = Конвеєр, який текст проходить перед озвученням.
tts_filters_load_failed = Не вдалося завантажити збережені фільтри, тому редагування вимкнено, щоб їх зберегти: { $reason }

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
tts_filters_output_strip_emoji = Прибирати емодзі
tts_filters_output_strip_emoji_meta = видаляти символи емодзі перед озвученням
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
tts_spoken_link = link
tts_spoken_says = says
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
tts_aliases_strategy_load_failed = Не вдалося завантажити стратегію призначення голосу, тому її зміну вимкнено, щоб її зберегти: { $reason }
tts_aliases_strategy_save_failed = Не вдалося зберегти стратегію призначення голосу: { $reason }
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
queues_delete_confirm_body = Дії з цієї черги перейдуть до черги { $default }. Це не можна скасувати.

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

chat_badge_owner = OWNER
chat_badge_mod = MOD
chat_badge_vip = VIP
chat_badge_sub = SUB
chat_badge_bot = BOT
chat_badge_partner = PARTNER
chat_badge_prime = PRIME
chat_badge_founder = FOUNDER
chat_badge_turbo = TURBO
chat_badge_hype = HYPE
chat_badge_bits = BITS
chat_badge_bits_leader = BITS LEADER
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
event_feed_step_count = { $count ->
    [one] { $count } крок
    [few] { $count } кроки
   *[other] { $count } кроків
}
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
event_feed_replay_badge = Повтор
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
event_feed_outcome_trigger = → тригер
event_feed_outcome_actions = { $count ->
    [one] → { $count } дія
    [few] → { $count } дії
   *[other] → { $count } дій
}
event_feed_outcome_no_match = немає збігу
event_feed_outcome_sent = надіслано
event_feed_duration_ms = { $ms } мс

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
script_editor_timeout_ms = Тайм-аут: { $ms } мс
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
integration_sign_out_failed = Не вдалося вийти з { $name }: збережені облікові дані не вдалося видалити
integration_signing_out = Вихід...
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
integration_qa_field_select = Оберіть...
integration_qa_field_unavailable = Варіанти недоступні, поки цю інтеграцію відключено
integration_qa_options_unauthorized = Авторизація застаріла - увійдіть знову, щоб завантажити варіанти
integration_qa_options_rate_limited = Забагато запитів - спробуйте завантажити варіанти трохи згодом
integration_qa_options_rejected = Інтеграція відхилила запит - перевірте інші поля
integration_qa_options_not_listable = Ця інтеграція не може показати ці варіанти
integration_qa_options_transport = Не вдалося завантажити варіанти - спробуйте ще раз
option_hint_current_scene = Зараз: { $scene } (В ЕФІРІ)
option_hint_live = в ефірі
option_hint_visible = видиме
option_hint_hidden = приховане
option_hint_loaded = завантажено
option_hint_active = активне
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

## Widget - save indicator

widget_copied_toast = Скопійовано в буфер обміну
widget_save_all_saved = Усі зміни збережено

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
settings_audio_title = Вивід
settings_audio_output_devices = Пристрій виводу
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
settings_audio_routing_destination = Аудіооверлей
settings_audio_routing_destination_none = Немає
settings_audio_routing_no_overlays = Спочатку створи оверлей на екрані оверлеїв.
settings_audio_routing_test = Тест на оверлеї
settings_audio_routing_testing = Відтворення
settings_audio_routing_test_error = Тест оверлея не вдався: { $error }
settings_audio_routing_persist_error = Не вдалося зберегти маршрутизацію: { $error }
settings_audio_routing_in_effect = Зараз діє
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
settings_voice_gate_input_devices = Пристрій вводу
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
script_editor_delete_checking = Перевіряємо, які дії використовують цей скрипт...
script_editor_delete_unused = Жодна дія не використовує цей скрипт. Він переміститься в архів, а видалення можна скасувати одразу після нього.
script_editor_delete_used_by = { $count ->
    [one] Його використовує { $count } дія: { $names }.
    [few] Його використовують { $count } дії: { $names }.
    [many] Його використовують { $count } дій: { $names }.
   *[other] Його використовують { $count } дій: { $names }.
}
script_editor_delete_maybe_used_by = { $count ->
    [one] { $count } дія може вибирати його через змінну: { $names }.
    [few] { $count } дії можуть вибирати його через змінну: { $names }.
    [many] { $count } дій можуть вибирати його через змінну: { $names }.
   *[other] { $count } дій можуть вибирати його через змінну: { $names }.
}
script_editor_delete_archive_note = Їхні кроки не працюватимуть, доки скрипт не буде відновлено. Видалення можна скасувати одразу після нього.
script_editor_deleted_toast = Видалено '{ $name }'
script_editor_delete_failed = Не вдалося видалити скрипт: { $error }
script_editor_restore_failed = Не вдалося відновити скрипт: { $error }
script_editor_rename_title = Перейменувати скрипт?
script_editor_rename_updates_none = Жодна дія не називає цей скрипт напряму.
script_editor_rename_updates = { $count ->
    [one] { $count } дію буде оновлено на нову назву: { $names }.
    [few] { $count } дії буде оновлено на нову назву: { $names }.
    [many] { $count } дій буде оновлено на нову назву: { $names }.
   *[other] { $count } дій буде оновлено на нову назву: { $names }.
}
script_editor_rename_templated = { $count ->
    [one] Перевірте вручну { $count } дію - вона вибирає скрипт через змінну й не буде оновлена: { $names }.
    [few] Перевірте вручну { $count } дії - вони вибирають скрипт через змінну й не будуть оновлені: { $names }.
    [many] Перевірте вручну { $count } дій - вони вибирають скрипт через змінну й не будуть оновлені: { $names }.
   *[other] Перевірте вручну { $count } дій - вони вибирають скрипт через змінну й не будуть оновлені: { $names }.
}
script_editor_renamed_toast = { $count ->
    [one] Перейменовано на '{ $name }' - оновлено { $count } дію
    [few] Перейменовано на '{ $name }' - оновлено { $count } дії
    [many] Перейменовано на '{ $name }' - оновлено { $count } дій
   *[other] Перейменовано на '{ $name }' - оновлено { $count } дій
}
script_editor_rename_check_toast = { $count ->
    [one] Перевірте { $count } дію, що вибирає скрипт через змінну: { $names }
    [few] Перевірте { $count } дії, що вибирають скрипт через змінну: { $names }
    [many] Перевірте { $count } дій, що вибирають скрипт через змінну: { $names }
   *[other] Перевірте { $count } дій, що вибирають скрипт через змінну: { $names }
}
script_editor_rename_failed = Не вдалося перейменувати скрипт: { $error }

## Widget - layout chrome

## Widget - volume slider

## Locale-aware formatting - month abbreviations (uk, genitive short form for date display)

## Locale-aware formatting - relative time (uk four-form plural: one/few/many/other)

## Storage error screen

## Недоступна інтеграція
msg-desktop-health_status = Статус
msg-desktop-health_uptime = Час роботи
msg-desktop-health_endpoint = Адреса
msg-desktop-health_version = Версія

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
midi_devices_load_failed = Не вдалося завантажити запам'ятовані MIDI-пристрої, тому нові не додаються, щоб зберегти список: { $reason }
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
overlays_toast_token_shaped_name = Ця назва схожа на токен оверлеїв. Оберіть іншу назву.
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
config_form_duration_unit_days = дні
field_label_allow_everyone_pings = Дозволити @everyone / @here
field_label_allow_role_pings = Дозволити пінг ролей
field_label_ban_type_filter = Тип бану
field_label_case_sensitive = З урахуванням регістру
field_label_core__action_id = Дія
field_label_core__add_amount = Кількість (від'ємна - відняти)
field_label_core__amount = Кількість
field_label_core__array_source = Змінна-масив
field_label_core__base = Початкові дата й час (або %var%)
field_label_core__body = Тіло запиту
field_label_core__captures_into_var = Змінна для груп збігу (масив)
field_label_core__cases = Варіанти
field_label_core__condition = Умова
field_label_core__content = Вміст
field_label_core__content_type = Тип вмісту
field_label_core__count = Кількість повторів
field_label_core__create_parent_dirs = Створити батьківські папки
field_label_core__custom_format_string = Власний шаблон дати (напр. HH:mm dd.MM.yyyy, d MMMM yyyy, EEEE)
field_label_core__default_chain = За замовчуванням
field_label_core__default_value = Типове значення
field_label_core__delay_amount = Запустити через
field_label_core__due_at = Дата й час (або %var%)
field_label_core__due_at_into_var = Змінна для часу запуску
field_label_core__else_chain = Інакше
field_label_core__encoding = Кодування
field_label_core__end_index = Кінцевий індекс (-1 = до кінця, не включно)
field_label_core__expression = Вираз
field_label_core_logic_switch_case__expression = Значення
field_label_core__follow_redirects = Переходити за перенаправленнями
field_label_core__format = Шаблон дати (напр. dd.MM.yyyy HH:mm, yyyy-MM-dd'T'HH:mm:ssxxx)
field_label_core__format_string = Шаблон дати (напр. HH:mm dd.MM.yyyy, d MMMM yyyy, EEEE)
field_label_core__from = Від (дата й час або %var%)
field_label_core__headers = Заголовки (об'єкт JSON)
field_label_core__icon_path = Шлях до значка
field_label_core__id_into_var = Змінна для ID запланованого запуску
field_label_core__ignore_missing = Ігнорувати відсутній файл
field_label_core__include_dirs = Включати папки
field_label_core__inherit_args = Передати поточні аргументи
field_label_core__into_arg = Змінна для результату
field_label_core__into_var = Змінна для результату
field_label_core__is_regex = Регулярний вираз
field_label_core__items = Елементи (по одному в рядку)
field_label_core__keep_current = Не зупиняти поточну дію
field_label_core__key = Назва змінної
field_label_core__late_tolerance_minutes = Пропустити, якщо запізнення більше ніж
field_label_core__level = Рівень
field_label_core__mark_as = Позначити як
field_label_core__max = Максимум
field_label_core__max_iterations = Макс. ітерацій
field_label_core__max_length = Макс. довжина (0 = без меж)
field_label_core__max_parts = Макс. частин (0 = без меж)
field_label_core__message = Повідомлення
field_label_core__min = Мінімум
field_label_core__mode = Режим
field_label_core__ms = Тривалість
field_label_core__name = Назва змінної
field_label_core__override_outputs = Тестові вихідні дані (об'єкт JSON)
field_label_core__parse_response_as = Розібрати відповідь як
field_label_core__parts = Частини (по одній у рядку)
field_label_core__path = Шлях до файлу (відносно папки файлів)
field_label_core__pattern = Шаблон glob
field_label_core__persisted = Зберігати між перезапусками
field_label_core__platform = Платформа чи сервіс (порожньо = найсвіжіше з усіх)
field_label_core__poll_interval_ms = Інтервал перевірки
field_label_core__probability_true = Імовірність true (0..1)
field_label_core__query_params = Параметри запиту (об'єкт JSON)
field_label_core__queue_id = Черга
field_label_core__read_as = Читати як
field_label_core__reason = Причина
field_label_core__recursive = Рекурсивно
field_label_core__remove_all = Видалити всі збіги
field_label_core__replace_with = Замінити на
field_label_core__result_type = Тип результату
field_label_core__search = Знайти
field_label_core__separator = Роздільник
field_label_core__slot = Слот
field_label_core__source = Текст
field_label_core__start_index = Початковий індекс (від 0, у символах)
field_label_core__target_var = Змінна для результату
field_label_core__template = Шаблон
field_label_core__text = Текст
field_label_core__then_chain = Тоді
field_label_core__timeout_ms = Тайм-аут
field_label_core__title = Заголовок
field_label_core__to = До (дата й час або %var%)
field_label_core__treat_undefined_as_false = Вважати невизначене за false
field_label_core__trigger_instance_id = Тригер
field_label_core__trim_each = Обрізати пробіли в кожній частині
field_label_core__unit = Одиниця
field_label_core__urgency = Терміновість (лише Linux)
field_label_core__url = URL
field_label_core__user_login = Логін користувача
field_label_core__utc = UTC замість місцевого часу
field_label_core__value = Значення
field_label_core__var_name = Назва змінної
field_label_core__weights = Ваги (по одній у рядку, порожньо = рівномірно)
field_label_core__while_condition = Умова продовження
field_label_core_action_cancel_scheduled__into_var = Змінна для результату (bool)
field_label_core_action_cancel_scheduled__key = Ключ (точний збіг)
field_label_core_action_schedule__inherit_args = Передати поточні змінні
field_label_core_action_schedule__key = Ключ (необов'язково, замінює запланований запуск з тим самим ключем)
field_label_core_file_list__path = Шлях до папки (відносно папки файлів)
field_label_core_file_write__mode = Режим запису
field_label_core_globals_array_append__value = Значення для додавання
field_label_core_globals_array_remove__value = Значення для видалення
field_label_core_logic_loop__body = Кроки циклу
field_label_core_notify_show__body = Текст сповіщення
field_label_core_string_regex_match__into_var = Змінна для результату (bool)
field_label_core_string_regex_match__pattern = Шаблон
field_label_core_test_fire_trigger__trigger_instance_id = Екземпляр тригера
field_label_core_time_format__source = Дата й час (або %var%)
field_label_core_time_now__format = Формат
field_label_core_time_parse__source = Текст для розбору
field_label_core_time_parse__utc = Читати текст без зсуву як UTC, а не місцевий час
field_label_donation__provider = Донат-сервіс
field_label_donations__amount = Сума
field_label_donations__currency = Валюта
field_label_donations__donor_name = Ім'я донатера
field_label_donations__message = Повідомлення
field_label_donations__provider = Донат-сервіс
field_label_event_name = Назва події
field_label_first_chatters_only = Лише ті, хто пише вперше
field_label_hotkey__combo = Комбінація клавіш
field_label_ignore_bot_messages = Ігнорувати повідомлення бота
field_label_midi__channel = Канал
field_label_midi__controller = Контролер
field_label_midi__device = Пристрій
field_label_midi__direction = Напрямок
field_label_midi__note = Нота
field_label_midi__program = Програма
field_label_midi_send__channel = Канал (0-15)
field_label_midi_send__controller = Контролер (CC)
field_label_midi_send__note = Нота (NoteOn/NoteOff)
field_label_obs__scene = Сцена
field_label_obs__source_name = Назва джерела
field_label_phrase = Фраза команди
field_label_reward_id = Нагорода
field_label_kick__reward_id = ID нагороди
field_label_script__body = Код скрипту
field_label_script__payload = Дані (об'єкт JSON)
field_label_script__script_name = Скрипт
field_label_script__target_var = Змінна для результату
field_label_timer__interval_minutes = Інтервал
field_label_timer__min_chat_messages = Мін. повідомлень у чаті
field_label_timer__only_while_live = Лише в ефірі
field_label_twitch__color = Колір
field_label_twitch__from_user = Від кого (логін)
field_label_twitch__match_is_regex = Регулярний вираз
field_label_twitch__match_text = Текст для збігу
field_label_twitch__max_bits = Макс. бітсів (-1 = без меж)
field_label_twitch__min_amount_cents = Мін. донат (центи)
field_label_twitch__min_bits = Мін. бітсів
field_label_twitch__min_level = Мін. рівень (1-5)
field_label_twitch__reward_title = Назва нагороди (якщо нагороду не вибрано)
field_label_twitch__sender = Від імені
field_label_twitch__state_filter = Стан гостя (порожнє = усі)
field_label_twitch_automod_message_updated__status_filter = Статус рішення
field_label_twitch_channel_points_redemption__reward_id = Нагорода (порожнє = усі)
field_label_twitch_channel_points_redemption_updated__status_filter = Статус отримання нагороди
field_label_twitch__title = Назва
field_label_twitch_poll_start__title = Питання
field_label_twitch_prediction_start__title = Питання
field_label_twitch__message = Повідомлення
field_label_twitch__target_user_login = Цільовий користувач (логін)
field_label_twitch__to_user_login = Логін отримувача
field_label_twitch_guest_star_invite__target_user_login = Логін гостя
field_label_twitch_guest_star_assign_slot__target_user_login = Логін гостя
field_label_twitch_guest_star_remove_guest__target_user_login = Логін гостя
field_label_twitch__session_id = ID сесії
field_label_twitch__slot_id = ID слота
field_label_twitch__message_id = ID повідомлення
field_label_twitch__parent_message_id = ID початкового повідомлення
field_label_twitch__prediction_id = ID прогнозу
field_label_twitch__poll_id = ID опитування
field_label_twitch__redemption_id = ID отримання нагороди
field_label_twitch__term_id = ID заблокованого терміна
field_label_twitch_automod_add_blocked_term__text = Термін для блокування
field_label_twitch__duration_seconds = Тривалість
field_label_twitch__reason = Причина (необов'язково, до 500 символів)
field_label_twitch_moderation_warn_user__reason = Причина (обов'язково, до 500 символів)
field_label_twitch__target = Цільова платформа
field_label_twitch__to_broadcaster_login = Цільовий канал
field_label_twitch__description = Опис (необов'язково)
field_label_twitch__category_id = ID категорії
field_label_twitch__category_name = Назва категорії
field_label_twitch__tags = Теги (по одному в рядку або через кому)
field_label_twitch__choices = Варіанти (по одному в рядку або через кому, 2-5)
field_label_twitch__channel_points_voting_enabled = Голосування балами каналу
field_label_twitch__channel_points_per_vote = Балів каналу за голос (0 = типово; діє лише за ввімкненого голосування)
field_label_twitch__status = Статус завершення
field_label_twitch__outcomes = Результати (по одному в рядку або через кому, 2-10)
field_label_twitch__prediction_window_seconds = Час на прогноз
field_label_twitch__winning_outcome_id = ID переможного результату
field_label_twitch__cost = Вартість (бали каналу)
field_label_twitch__prompt = Текст запиту для глядача
field_label_twitch_channel_points_create_reward__prompt = Підказка (необов'язково)
field_label_twitch__background_color_hex = Колір фону (#RRGGBB)
field_label_twitch_channel_points_create_reward__background_color_hex = Колір фону (#RRGGBB або порожньо)
field_label_twitch__max_per_stream = Макс. за трансляцію (0 = без обмежень)
field_label_twitch__max_per_user_per_stream = Макс. на глядача за трансляцію (0 = без обмежень)
field_label_twitch__global_cooldown_seconds = Загальне охолодження (0 = вимкнено)
field_label_twitch__is_enabled = Увімкнено
field_label_twitch__requires_user_input = Вимагати введення від глядача
field_label_twitch__should_redemptions_skip_request_queue = Пропускати чергу отримань нагород
field_label_twitch__is_paused = На паузі
field_label_twitch__audio_enabled = Звук увімкнено
field_label_twitch__video_enabled = Відео увімкнено
field_label_twitch__volume = Гучність (0-100)
field_label_twitch__emote_only = Режим лише смайлів
field_label_twitch__follower_mode = Режим лише для фоловерів
field_label_twitch__follower_mode_min_minutes = Мін. час як фоловер
field_label_twitch__slow_mode = Повільний режим
field_label_twitch__slow_mode_wait_seconds = Затримка повільного режиму
field_label_twitch__subscriber_mode = Режим лише для підписників
field_label_twitch__unique_chat_mode = Режим унікального чату
field_label_twitch__overall_level = Загальний рівень (замінює окремі категорії)
field_label_twitch__aggression = Агресія
field_label_twitch__bullying = Цькування
field_label_twitch__disability = Інвалідність
field_label_twitch__misogyny = Мізогінія
field_label_twitch__race_ethnicity_or_religion = Раса, етнічність або релігія
field_label_twitch__sex_based_terms = Терміни за ознакою статі
field_label_twitch__sexuality_sex_or_gender = Сексуальність, стать або ґендер
field_label_twitch__swearing = Лайка
field_label_youtube__channel_id = ID каналу
field_label_youtube__message_id = ID повідомлення
field_label_youtube__message = Повідомлення
field_label_youtube__title = Назва
field_label_youtube__description = Опис
field_label_youtube__category_id = ID категорії
field_label_youtube__image_path = Шлях до зображення
field_label_youtube__duration_seconds = Тривалість
field_label_youtube__duration_secs = Тривалість реклами
field_label_youtube__privacy_status = Приватність трансляції
field_label_youtube__identifier = Псевдонім (@handle) або ID каналу
field_label_youtube__question = Питання
field_label_youtube__options = Варіанти (по одному в рядку, 2-4)
field_label_kick__user_id = ID цільового користувача
field_label_kick__reason = Причина (необов'язково, до 100 символів)
field_label_kick_reward_create__title = Назва нагороди
field_label_kick_reward_update__title = Нова назва (необов'язково)
field_label_kick_channel_update_info__title = Назва трансляції
field_label_kick_reward_create__cost = Вартість (бали каналу)
field_label_kick_reward_update__cost = Нова вартість (необов'язково)
field_label_kick_reward_create__description = Опис (необов'язково)
field_label_kick_reward_update__description = Новий опис (необов'язково)
field_label_kick__category_id = ID категорії
field_label_kick__tags = Теги (через кому, до 10)
field_label_kick__message_id = ID повідомлення
field_label_kick__message = Повідомлення
field_label_kick__as_bot = Надіслати від імені бота
field_label_kick__redemption_ids = ID отримання нагороди (один або кілька)
field_label_kick__query = Пошуковий запит
field_label_kick__slug = Адреса каналу (slug)
field_label_kick__duration_minutes = Тривалість
field_label_obs__caption_text = Текст субтитрів
field_label_obs__on = Активно
field_label_obs_studio_set_enabled__on = Увімкнено
field_label_vtube__model_id = Модель
field_label_vtube_item_pin__model_id = ID моделі
field_label_soundboard__clip_id = Кліп
field_label_vtube__color_r = Червоний
field_label_vtube__color_g = Зелений
field_label_vtube__color_b = Синій
field_label_vtube__color_a = Альфа-канал
field_label_vtube__mix_with_scene_lighting = Змішування з освітленням сцени
field_label_obs__source = Джерело
field_label_obs_media_restart__source = Медіаджерело
field_label_obs_browser_refresh__source = Браузерне джерело
field_label_obs_audio_set_volume__source = Аудіоджерело
field_label_obs_audio_set_mute__source = Джерело
field_label_obs_source_set_input_settings__source = Джерело
field_label_obs_source_get_input_settings__source = Назва джерела
field_label_discord__webhook_name = Вебхук
field_label_discord__content = Підпис
field_label_discord_webhook_update_message__content = Новий вміст
field_label_discord_webhook_send_message__content = Повідомлення
field_label_discord__file_path = Шлях до файлу
field_label_vtube__x = Позиція X
field_label_vtube__y = Позиція Y
field_label_vtube__rotation = Обертання
field_label_vtube__size = Розмір
field_label_vtube__duration = Тривалість (с)
field_label_vtube_item_throw__duration = Тривалість кидка (с)
field_label_discord__message_id = ID повідомлення
field_label_tts__alias_name = Назва псевдоніма
field_label_tts__engine_id = Рушій
field_label_tts__voice_id = Голос
field_label_discord__embed_title = Заголовок
field_label_discord__embed_description = Опис
field_label_obs__transition = Назва переходу
field_label_obs__name = Назва колекції сцен
field_label_obs_profile_switch__name = Назва профілю
field_label_obs__volume_db = Гучність (дБ)
field_label_vtube__hotkey_id = Гаряча клавіша
field_label_tts__text = Текст
field_label_tts__voice = Голос
field_label_tts__wait_for_completion = Чекати завершення
field_label_obs__muted = Звук вимкнено
field_label_midi__port = Вихідний порт
field_label_midi__message_kind = Тип повідомлення
field_label_midi__velocity = Сила натискання (NoteOn/NoteOff)
field_label_midi__value = Значення (CC)
field_label_obs__settings_json = Налаштування (JSON)
field_label_obs__overlay = Об'єднати з наявними налаштуваннями
field_label_tts__keep_current = Дати поточному елементу завершитися
field_label_vtube__param_id = ID параметра
field_label_vtube__value = Значення
field_label_vtube__hold_secs = Утримання (секунди, 0 = одноразово)
field_label_obs__visible = Видимість
field_label_vtube__enabled = Фізика ввімкнена
field_label_vtube__item_instance_id = ID екземпляра предмета
field_label_vtube__pin = Закріпити (вимкніть, щоб відкріпити)
field_label_vtube__vertex_pin_type = Позиція закріплення
field_label_vtube__angle_relative_to = Кут відносно
field_label_vtube__size_relative_to = Розмір відносно
field_label_vtube__art_mesh_id = ID ArtMesh
field_label_vtube__angle = Кут
field_label_soundboard__volume_db = Гучність (дБ)
field_label_obs__path = Тека
field_label_obs_capture_screenshot__path = Шлях до файлу
field_label_vtube__order = Порядок
field_label_vtube__fade_mode = Режим згасання
field_label_obs__locked = Заблоковано
field_label_vtube__file_name = Назва файлу предмета
field_label_vtube__fade_time = Час згасання (с)
field_label_vtube__unload_on_disconnect = Вивантажити, коли плагін відключається
field_label_server__payload = Дані (JSON-об'єкт)
field_label_overlay__overlay_id = Оверлей
field_label_overlay__duration_secs = Власна тривалість
field_label_overlay__wait_for_show = Чекати завершення показу
field_label_obs__filter = Назва фільтра
field_label_obs__enabled = Увімкнено
field_label_vtube__expression_file = Вираз
field_label_vtube__active = Активно
field_label_vtube__from_x = Від X
field_label_vtube__from_y = Від Y
field_label_vtube__to_x = До X
field_label_vtube__to_y = До Y
field_optional_core__skip_if_late = Пропустити, а не запускати із запізненням, якщо forge був закритий
field_optional_core__use_due_at = Запустити в певні дату й час
field_optional_donation__provider = Фільтр за донат-сервісом
field_optional_midi__channel = Фільтр за каналом
field_optional_midi__controller = Фільтр за контролером
field_optional_midi__device = Фільтр за пристроєм
field_optional_midi__direction = Фільтр за напрямком
field_optional_midi__note = Фільтр за нотою
field_optional_midi__program = Фільтр за програмою
field_optional_obs__scene = Фільтр за сценою
field_optional_obs__source_name = Фільтр за джерелом
field_optional_twitch__title = Назва
field_optional_twitch__cost = Вартість (бали каналу)
field_optional_twitch__prompt = Підказка
field_optional_twitch__background_color_hex = Колір фону
field_optional_twitch__max_per_stream = Макс. за трансляцію
field_optional_twitch__max_per_user_per_stream = Макс. на глядача за трансляцію
field_optional_twitch__global_cooldown_seconds = Загальне охолодження (секунди)
field_optional_twitch__volume = Гучність (0-100)
field_optional_vtube__mix_with_scene_lighting = Змішування з освітленням сцени
field_optional_vtube__x = Позиція X
field_optional_vtube__y = Позиція Y
field_optional_vtube__rotation = Обертання
field_optional_vtube__size = Розмір
field_optional_vtube__duration = Тривалість (с)
field_optional_vtube_item_throw__duration = Тривалість кидка (с)
field_optional_tts__voice = Голос (залиште порожнім для голосу за замовчуванням)
field_optional_vtube__model_id = ID моделі
field_optional_vtube__art_mesh_id = ID ArtMesh
field_optional_vtube__angle = Кут
field_optional_vtube__order = Порядок
field_optional_vtube__fade_time = Час згасання (с)
field_optional_overlay__duration_secs = Власна тривалість
field_optional_vtube__from_x = Від X
field_optional_vtube__from_y = Від Y
field_optional_vtube__to_x = До X
field_optional_vtube__to_y = До Y
field_placeholder_core_args_set__value = привіт
field_placeholder_core_logic_stop__reason = чому ланцюжок зупинився
field_placeholder_core_notify_show__title = Стрім починається
field_placeholder_core_string_length__source = Привіт
field_placeholder_core_string_lowercase__source = Привіт, світе
field_placeholder_core_string_regex_match__source = Привіт, світе
field_placeholder_core_string_replace__search = світе
field_placeholder_core_string_replace__source = Привіт, світе
field_placeholder_core_string_substring__source = Привіт, світе
field_placeholder_core_string_titlecase__source = привіт, світе
field_placeholder_core_string_uppercase__source = Привіт, світе
field_placeholder_twitch_guest_star_guest_updated__state_filter = напр. live
field_placeholder_twitch_chat_whisper__from_user = будь-хто
field_placeholder_twitch_chat_whisper__match_text = залиште порожнім, щоб збігалося все
field_placeholder_twitch_poll_start__title = У що пограємо далі?
field_placeholder_twitch_automod_add_blocked_term__text = слово або фраза (2-500 символів)
field_placeholder_twitch_channel_update_category__category_id = напр. 509658
field_placeholder_twitch_channel_update_category__category_name = напр. Just Chatting
field_placeholder_twitch_channel_points_create_reward__title = Моя нагорода
field_placeholder_twitch_channel_points_update_reward__title = Моя нагорода
field_placeholder_twitch_channel_points_create_reward__prompt = Введіть своє повідомлення
field_placeholder_twitch_channel_points_update_reward__prompt = Введіть своє повідомлення
field_placeholder_twitch_prediction_start__title = Чи виграю я цю гру?
field_placeholder_youtube_chat_create_poll__question = Питання опитування
field_placeholder_youtube_stream_update_title__title = Нова назва трансляції
field_placeholder_youtube_stream_update_category__category_id = 20 (Ігри), 24 (Розваги)...
field_placeholder_youtube_lookup_viewer__identifier = @handle або UC...
field_placeholder_youtube_moderation_add_moderator__channel_id = UC... або %user_id%
field_placeholder_youtube_moderation_ban_user__channel_id = UC... або %user_id%
field_placeholder_youtube_moderation_remove_moderator__channel_id = UC... або %user_id%
field_placeholder_youtube_moderation_timeout_user__channel_id = UC... або %user_id%
field_placeholder_youtube_moderation_unban_user__channel_id = UC... або %user_id%
field_placeholder_kick_reward_create__title = напр. Пити воду
field_placeholder_kick_reward_create__cost = напр. 500
field_placeholder_kick_reward_create__description = Залиште порожнім, щоб пропустити
field_placeholder_kick_reward_update__title = Залиште порожнім, щоб не змінювати
field_placeholder_kick_reward_update__cost = Залиште порожнім, щоб не змінювати
field_placeholder_kick_reward_update__description = Залиште порожнім, щоб не змінювати
field_placeholder_kick_channel_update_info__title = Залиште порожнім, щоб не змінювати
field_placeholder_kick_channel_update_info__category_id = Залиште порожнім, щоб не змінювати
field_placeholder_kick_channel_update_info__tags = Залиште порожнім, щоб не змінювати
field_placeholder_kick_reward_redemption_accept__redemption_ids = %redemption_id% або id1,id2,id3 (макс. 25)
field_placeholder_kick_reward_redemption_reject__redemption_ids = %redemption_id% або id1,id2,id3 (макс. 25)
field_placeholder_obs_streaming_send_caption__caption_text = Текст субтитрів для трансляції
field_placeholder_vtube_model_tint__mix_with_scene_lighting = від 0.0 до 1.0
field_placeholder_vtube_model_move__x = від -1.0 до 1.0
field_placeholder_vtube_model_move__y = від -1.0 до 1.0
field_placeholder_vtube_model_move__rotation = від -360 до 360
field_placeholder_vtube_model_move__size = від -100 до 100
field_placeholder_tts_alias_set__alias_name = напр. %user% або ID глядача
field_placeholder_discord_webhook_send_embed__embed_title = Стрім розпочато
field_placeholder_obs_scene_set_transition__transition = напр. Затухання
field_placeholder_obs_collection_switch__name = напр. Основна колекція
field_placeholder_obs_audio_set_volume__volume_db = напр. -6.0
field_placeholder_obs_source_set_input_settings__source = напр. Вебкамера
field_placeholder_vtube_model_set_physics__duration = від 0.5 до 5
field_placeholder_vtube_item_pin__item_instance_id = ID екземпляра предмета з VTS
field_placeholder_vtube_item_pin__model_id = порожньо = поточна модель
field_placeholder_vtube_item_pin__art_mesh_id = порожньо = випадковий ArtMesh
field_placeholder_vtube_item_pin__size = від 0.0 до 1.0
field_placeholder_obs_profile_switch__name = напр. Стрім
field_placeholder_vtube_item_move__item_instance_id = ID екземпляра предмета з VTS
field_placeholder_vtube_item_move__x = від -1.0 до 1.0
field_placeholder_vtube_item_move__y = від -1.0 до 1.0
field_placeholder_vtube_item_move__size = від 0.0 до 1.0
field_placeholder_vtube_item_move__rotation = від -360 до 360
field_placeholder_tts_alias_switch_active__alias_name = напр. %user% або ID глядача
field_placeholder_vtube_item_load__x = від -1.0 до 1.0
field_placeholder_vtube_item_load__y = від -1.0 до 1.0
field_placeholder_vtube_item_load__size = від 0.0 до 1.0
field_placeholder_vtube_item_load__rotation = від -360 до 360
field_placeholder_obs_source_get_input_settings__source = напр. Вебкамера
field_placeholder_obs_filter_set_enabled__filter = напр. Корекція кольору
field_placeholder_vtube_item_throw__from_x = від -1.0 до 1.0
field_placeholder_vtube_item_throw__from_y = від -1.0 до 1.0
field_placeholder_vtube_item_throw__to_x = від -1.0 до 1.0
field_placeholder_vtube_item_throw__to_y = від -1.0 до 1.0
field_placeholder_vtube_item_throw__size = від 0.0 до 1.0

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
integration_failed_open_settings = Відкрити налаштування
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

twitch_bot_section_title = Бот-акаунт
twitch_bot_header_signing_in = вхід
twitch_bot_header_reauth = потрібен вхід
twitch_bot_intro = Надсилайте повідомлення в чат від імені другого акаунта Twitch. Події, модерація та нагороди лишаються на акаунті стримера.
twitch_bot_connect_btn = Підключити бот-акаунт
twitch_bot_private_window_hint = Відкрийте посилання в приватному вікні браузера, де ви увійшли як бот, і введіть код. Не використовуйте вікно, де ви увійшли як стример.
twitch_bot_activation_link = Посилання
twitch_bot_activation_code = Код
twitch_bot_requesting = Запит коду для входу...
twitch_bot_waiting = Очікуємо підтвердження від бот-акаунта...
twitch_bot_try_again_btn = Спробувати ще раз
twitch_bot_disconnect_btn = Відключити
twitch_bot_disconnect_failed = Не вдалося відключити бота: { $error }
twitch_bot_reauth_detail = Бот-акаунту потрібно увійти знову.
twitch_bot_fallback_active = Доти типові повідомлення надсилаються від @{ $login }.
twitch_bot_reauth_btn = Увійти знову
twitch_bot_not_moderator_note = Бот не є модератором вашого каналу. Він потрібен для оголошень і шаутаутів, а не-модератори обмежені 20 повідомленнями за 30 секунд.
twitch_bot_make_moderator_btn = Зробити бота модератором
twitch_bot_sent_as_label = Надсилає
twitch_bot_sent_as_role_bot = бот-акаунт
twitch_bot_sent_as_role_streamer = акаунт стримера
twitch_bot_error_same_account = Це ваш акаунт стримера. Увійдіть як бот у приватному вікні.
twitch_bot_error_denied = Вхід бота відхилено на Twitch.
twitch_bot_error_expired = Термін дії коду минув. Спробуйте ще раз, щоб отримати новий код.
twitch_bot_error_failed = Не вдалося увійти як бот: { $error }
twitch_bot_mod_error_not_connected = Бот-акаунт не підключено.
twitch_bot_mod_error_streamer_reauth = Увійдіть в акаунт стримера знову, щоб дозволити зміну модераторів.
twitch_bot_mod_error_banned = Бота заблоковано у вашому каналі. Спершу розблокуйте його.
twitch_bot_mod_error_vip = Бот має статус VIP у вашому каналі. Спершу зніміть його.
twitch_bot_mod_error_refused = Twitch відмовив: { $reason }
twitch_bot_mod_error_unavailable = Не вдалося змінити модераторів: { $reason }
chat_send_as_me = Я
chat_send_as_bot = Бот
sender_option_default = Типово (бот, якщо підключений)
sender_option_streamer = Стример
sender_option_bot = Лише бот
sender_field_twitch_only_helper = Лише Twitch; на інших платформах ігнорується
event_feed_sent_as = від @{ $login }
announcement_color_primary = Основний (колір каналу)
announcement_color_blue = Синій
announcement_color_green = Зелений
announcement_color_orange = Помаранчевий
announcement_color_purple = Фіолетовий

step_option_unchanged = Не змінювати
step_option_on = Увімкнено
step_option_off = Вимкнено
step_option_level_0 = Без фільтрації (0)
step_option_level_1 = Менше фільтрації (1)
step_option_level_2 = Помірна фільтрація (2)
step_option_level_3 = Більше фільтрації (3)
step_option_level_4 = Максимальна фільтрація (4)
step_option_poll_terminated = Завершити, залишити результати видимими
step_option_poll_archived = Завершити й архівувати (сховати результати)
step_option_ease_linear = Лінійно
step_option_ease_in = Плавний старт
step_option_ease_out = Плавний кінець
step_option_ease_both = Плавний старт і кінець
step_option_ease_overshoot = З перельотом
step_option_ease_zip = Ривок
step_option_pin_center = Центр предмета
step_option_pin_random = Випадкова точка
step_option_rel_world = Світ
step_option_rel_item_rotation = Поточний поворот предмета
step_option_rel_model = Модель
step_option_rel_pin_position = Позиція кріплення
step_option_rel_item_size = Поточний розмір предмета
step_option_midi_note_on = Початок ноти
step_option_midi_note_off = Кінець ноти
step_option_midi_cc = Зміна контролера (CC)
step_option_midi_raw = Сирі байти
step_option_direction_input = Вхід
step_option_direction_output = Вихід
step_option_mark_completed = Завершено
step_option_mark_failed = Невдало
step_option_log_trace = Трасування
step_option_log_debug = Налагодження
step_option_log_info = Інформація
step_option_log_warn = Попередження
step_option_log_error = Помилка
step_option_len_chars = Символи
step_option_len_bytes = Байти
step_option_trim_both = З обох боків
step_option_trim_left = Зліва
step_option_trim_right = Справа
step_option_loop_count = Задана кількість
step_option_loop_foreach = Для кожного елемента масиву
step_option_loop_while = Поки умова істинна
step_option_write_overwrite = Перезаписати
step_option_write_append = Дописати
step_option_write_create_new = Лише створити новий
step_option_result_auto = Авто
step_option_result_int = Ціле число
step_option_result_float = Дробове число
step_option_urgency_low = Низька
step_option_urgency_normal = Звичайна
step_option_urgency_critical = Критична
step_option_parse_text = Текст
step_option_parse_json = JSON
step_option_parse_ignore = Ігнорувати
step_option_unit_seconds = Секунди
step_option_unit_minutes = Хвилини
step_option_unit_hours = Години
step_option_unit_days = Дні
step_option_unit_months = Місяці
step_option_unit_years = Роки
step_option_enc_utf8 = UTF-8
step_option_enc_latin1 = Latin-1
step_option_enc_raw_base64 = Сирі дані (base64)
step_option_read_lines = Масив рядків
step_option_read_whole = Увесь файл
step_option_read_json = JSON
step_option_fmt_iso8601 = ISO 8601
step_option_fmt_unix_seconds = Unix-секунди
step_option_fmt_unix_millis = Unix-мілісекунди
step_option_fmt_custom = Власний формат
step_option_privacy_public = Публічне
step_option_privacy_unlisted = За посиланням
step_option_privacy_private = Приватне
step_option_ban_any = Будь-який
step_option_ban_permanent = Постійний
step_option_ban_temporary = Тимчасовий
step_option_status_any = Будь-який
step_option_status_fulfilled = Виконано
step_option_status_canceled = Скасовано
step_option_status_approved = Схвалено
step_option_status_denied = Відхилено
step_option_status_expired = Прострочено
step_option_position_top = Угорі
step_option_position_bottom = Внизу
step_option_seconds_value = { $seconds } с

## Step and trigger names and descriptions
kind_name_core_action_cancel = Скасувати дію
kind_summary_core_action_cancel = Зупинити поточні запуски дії (або всі запущені дії)
kind_name_core_action_cancel_scheduled = Скасувати заплановану дію
kind_summary_core_action_cancel_scheduled = Скасувати очікуваний запланований запуск із цим точним ключем
kind_name_core_action_disable = Вимкнути дію
kind_summary_core_action_disable = Не дозволяти дії запускатися від тригерів
kind_name_core_action_enable = Увімкнути дію
kind_summary_core_action_enable = Дозволити дії запускатися від тригерів
kind_name_core_action_run = Запустити дію
kind_summary_core_action_run = Виконати кроки іншої дії всередині цього ланцюжка
kind_name_core_action_schedule = Запланувати дію
kind_summary_core_action_schedule = Запустити дію пізніше, навіть після перезапуску
kind_name_core_action_toggle = Перемкнути дію
kind_summary_core_action_toggle = Увімкнути вимкнену дію або вимкнути увімкнену
kind_name_core_args_set = Задати локальний аргумент
kind_summary_core_args_set = Задати змінну в аргументах поточного запуску
kind_name_core_clipboard_copy = Копіювати в буфер обміну
kind_summary_core_clipboard_copy = Скопіювати текст у системний буфер обміну
kind_name_core_clipboard_read = Прочитати буфер обміну
kind_summary_core_clipboard_read = Прочитати системний буфер обміну у змінну
kind_name_core_file_delete = Видалити файл
kind_summary_core_file_delete = Видалити файл із теки файлів; теки ніколи не видаляються
kind_name_core_file_list = Вміст теки
kind_summary_core_file_list = Перелічити вміст теки з файлами; назви записів зберігаються в масив-змінну
kind_name_core_file_read = Прочитати файл
kind_summary_core_file_read = Прочитати текстовий файл у змінну
kind_name_core_file_write = Записати файл
kind_summary_core_file_write = Записати вміст у файл у теці файлів; кількість записаних байтів зберігається в `file.bytes_written`
kind_name_core_globals_array_append = Додати до глобального масиву
kind_summary_core_globals_array_append = Додати значення в глобальну змінну-масив
kind_name_core_globals_array_remove = Вилучити з глобального масиву
kind_summary_core_globals_array_remove = Вилучити збіжні елементи з глобальної змінної-масиву
kind_name_core_globals_decrement = Зменшити глобальну змінну
kind_summary_core_globals_decrement = Зменшити числову глобальну змінну на вказану кількість
kind_name_core_globals_delete = Видалити глобальну змінну
kind_summary_core_globals_delete = Видалити глобальну змінну
kind_name_core_globals_get = Отримати глобальну змінну
kind_summary_core_globals_get = Прочитати глобальну змінну в аргумент
kind_name_core_globals_increment = Збільшити глобальну змінну
kind_summary_core_globals_increment = Збільшити числову глобальну змінну на вказану кількість
kind_name_core_globals_set = Задати глобальну змінну
kind_summary_core_globals_set = Присвоїти глобальній змінній значення
kind_name_core_globals_toggle = Перемкнути глобальну змінну
kind_summary_core_globals_toggle = Перемкнути логічну глобальну змінну між true і false
kind_name_core_http_delete = HTTP DELETE
kind_summary_core_http_delete = Надіслати запит DELETE; відповідь зберігається в `http.*`
kind_name_core_http_get = HTTP GET
kind_summary_core_http_get = Завантажити URL; статус, тіло й заголовки зберігаються в `http.*`
kind_name_core_http_patch = HTTP PATCH
kind_summary_core_http_patch = Надіслати запит PATCH із тілом; відповідь зберігається в `http.*`
kind_name_core_http_post = HTTP POST
kind_summary_core_http_post = Надіслати запит POST із тілом; відповідь зберігається в `http.*`
kind_name_core_http_put = HTTP PUT
kind_summary_core_http_put = Надіслати запит PUT із тілом; відповідь зберігається в `http.*`
kind_name_core_latest_get = Отримати останнє значення
kind_summary_core_latest_get = Прочитати останнє записане значення слота віджета (наприклад, останній донат) в аргумент
kind_name_core_log_write = Записати в журнал
kind_summary_core_log_write = Записати повідомлення в журнал із вибраним рівнем
kind_name_core_logic_break_loop = Перервати цикл
kind_summary_core_logic_break_loop = Вийти з найближчого вкладеного циклу
kind_name_core_logic_continue_loop = Наступна ітерація
kind_summary_core_logic_continue_loop = Перейти до наступної ітерації найближчого вкладеного циклу
kind_name_core_logic_if_then_else = Якщо / То / Інакше
kind_summary_core_logic_if_then_else = Виконати один із двох ланцюжків залежно від умови
kind_name_core_logic_loop = Цикл
kind_summary_core_logic_loop = Повторити ланцюжок задану кількість разів, для кожного елемента масиву або поки виконується умова
kind_name_core_logic_stop = Зупинити дію
kind_summary_core_logic_stop = Негайно зупинити поточний ланцюжок дії
kind_name_core_logic_switch_case = Вибір варіанта
kind_summary_core_logic_switch_case = Виконати ланцюжок варіанта, що збігається зі значенням
kind_name_core_logic_wait = Зачекати
kind_summary_core_logic_wait = Призупинити виконання на кількість мілісекунд (не більше 60 с)
kind_name_core_logic_wait_until = Чекати, доки
kind_summary_core_logic_wait_until = Перевіряти умову, доки вона не справдиться або не мине час очікування
kind_name_core_math_evaluate = Математичний вираз
kind_summary_core_math_evaluate = Обчислити арифметичний вираз і зберегти результат в аргумент
kind_name_core_notify_show = Показати сповіщення
kind_summary_core_notify_show = Показати системне сповіщення
kind_name_core_queue_clear = Очистити чергу
kind_summary_core_queue_clear = Скинути дії, що чекають у черзі, і за потреби зупинити поточну
kind_name_core_queue_pause = Призупинити чергу
kind_summary_core_queue_pause = Не запускати нові дії з черги до відновлення
kind_name_core_queue_resume = Відновити чергу
kind_summary_core_queue_resume = Дозволити призупиненій черзі знову запускати дії
kind_name_core_random_bool = Випадкове так/ні (зважене)
kind_summary_core_random_bool = Згенерувати зважене випадкове логічне значення і зберегти у змінну
kind_name_core_random_float = Випадкове дробове число
kind_summary_core_random_float = Згенерувати випадкове дробове число в [min, max] і зберегти у змінну
kind_name_core_random_int = Випадкове ціле число
kind_summary_core_random_int = Згенерувати випадкове ціле число в [min, max] і зберегти у змінну
kind_name_core_random_pick = Випадковий вибір
kind_summary_core_random_pick = Вибрати випадковий елемент зі списку і зберегти у змінну
kind_name_core_string_concat = Текст - об'єднати
kind_summary_core_string_concat = Об'єднати список рядків через необов'язковий роздільник
kind_name_core_string_format = Текст - шаблон
kind_summary_core_string_format = Скласти рядок із шаблону з підстановками %variable%
kind_name_core_string_length = Текст - довжина
kind_summary_core_string_length = Отримати довжину рядка в байтах або символах Unicode
kind_name_core_string_lowercase = Текст - малі літери
kind_summary_core_string_lowercase = Перетворити рядок на малі літери
kind_name_core_string_regex_match = Текст - збіг із regex
kind_summary_core_string_regex_match = Перевірити рядок на збіг із regex-шаблоном і за потреби захопити групи
kind_name_core_string_replace = Текст - замінити
kind_summary_core_string_replace = Замінити всі входження тексту або regex-шаблону в рядку
kind_name_core_string_split = Текст - розділити
kind_summary_core_string_split = Розділити рядок на масив частин за роздільником
kind_name_core_string_substring = Текст - підрядок
kind_summary_core_string_substring = Вирізати частину рядка за діапазоном індексів символів
kind_name_core_string_titlecase = Текст - з великої літери
kind_summary_core_string_titlecase = Зробити великою першу літеру кожного слова
kind_name_core_string_trim = Текст - обрізати пробіли
kind_summary_core_string_trim = Прибрати пробіли на початку та/або в кінці рядка
kind_name_core_string_uppercase = Текст - великі літери
kind_summary_core_string_uppercase = Перетворити рядок на великі літери
kind_name_core_test_fire_trigger = Тестово запустити тригер
kind_summary_core_test_fire_trigger = Запустити тригер із тестовими даними, щоб виконати його дії
kind_name_core_time_add = Час - додати до дати
kind_summary_core_time_add = Додати або відняти проміжок від дати й часу UTC; при додаванні місяців і років день обрізається до останнього можливого
kind_name_core_time_diff = Час - різниця
kind_summary_core_time_diff = Обчислити `to − from` як дробове число зі знаком у вибраних одиницях (секунди, хвилини, години чи дні)
kind_name_core_time_format = Час - форматувати
kind_summary_core_time_format = Записати дату й час текстом за шаблоном (наприклад, HH:mm dd.MM.yyyy або d MMMM yyyy) у місцевому часі або в UTC, якщо перемикач увімкнено
kind_name_core_time_now = Час - поточний час
kind_summary_core_time_now = Зафіксувати поточний час; дата й час пишуться в `into_var`, відформатований рядок - у `time.formatted` (власний шаблон, як-от HH:mm dd.MM.yyyy, місцевий час або UTC), секунди unix - у `time.unix_seconds`
kind_name_core_time_parse = Час - розібрати рядок
kind_summary_core_time_parse = Прочитати текст як дату й час за шаблоном (наприклад, dd.MM.yyyy HH:mm); текст без зсуву читається як місцевий час або як UTC, якщо перемикач увімкнено
kind_name_core_trigger_disable = Вимкнути тригер
kind_summary_core_trigger_disable = Не дозволяти тригеру запускати дії
kind_name_core_trigger_enable = Увімкнути тригер
kind_summary_core_trigger_enable = Дозволити тригеру запускати дії
kind_name_core_trigger_toggle = Перемкнути тригер
kind_summary_core_trigger_toggle = Увімкнути вимкнений тригер або вимкнути увімкнений
kind_name_core_url_open = Відкрити URL
kind_summary_core_url_open = Відкрити URL у браузері за замовчуванням
kind_name_core_users_get_var = Отримати змінну глядача
kind_summary_core_users_get_var = Прочитати змінну окремого глядача в аргумент
kind_name_core_users_increment_var = Збільшити змінну глядача
kind_summary_core_users_increment_var = Збільшити числову змінну глядача на вказану кількість
kind_name_core_users_set_var = Задати змінну глядача
kind_summary_core_users_set_var = Присвоїти змінній глядача значення
kind_name_discord_webhook_delete_message = Видалити повідомлення вебхука
kind_summary_discord_webhook_delete_message = Видаляє раніше надіслане повідомлення вебхука Discord.
kind_name_discord_webhook_send_embed = Надіслати оформлене повідомлення
kind_summary_discord_webhook_send_embed = Надсилає оформлене повідомлення (embed) у вебхук Discord.
kind_name_discord_webhook_send_file = Надіслати файл
kind_summary_discord_webhook_send_file = Завантажує файл із диска у вебхук Discord із необов'язковим підписом.
kind_name_discord_webhook_send_message = Надіслати текст
kind_summary_discord_webhook_send_message = Надсилає текстове повідомлення у вебхук Discord.
kind_name_discord_webhook_update_message = Редагувати повідомлення
kind_summary_discord_webhook_update_message = Змінює раніше надіслане повідомлення вебхука Discord.
kind_name_donation_received = Отримано донат
kind_summary_donation_received = Спрацьовує, коли надходить донат через підключений сервіс донатів
kind_name_donations_test_donation = Надіслати тестовий донат
kind_summary_donations_test_donation = Оголосити вигаданий донат, що запускає тригери донатів із позначкою тесту
kind_name_hotkey_global_pressed = Гарячу клавішу натиснуто
kind_summary_hotkey_global_pressed = Спрацьовує, коли натиснуто зареєстровану глобальну комбінацію клавіш.
kind_name_hotkey_global_released = Гарячу клавішу відпущено
kind_summary_hotkey_global_released = Спрацьовує, коли відпущено утримувану глобальну комбінацію клавіш.
kind_name_midi_device_connected = MIDI-пристрій підключено
kind_summary_midi_device_connected = Спрацьовує, коли з'являється MIDI-порт (пристрій підключено).
kind_name_midi_device_disconnected = MIDI-пристрій відключено
kind_summary_midi_device_disconnected = Спрацьовує, коли зникає MIDI-порт (пристрій відключено).
kind_name_midi_input_control_change = MIDI Control Change
kind_summary_midi_input_control_change = Спрацьовує, коли надходить повідомлення MIDI Control Change (CC).
kind_name_midi_input_note_off = MIDI Note Off
kind_summary_midi_input_note_off = Спрацьовує, коли надходить повідомлення MIDI Note Off (зокрема Note On зі швидкістю 0).
kind_name_midi_input_note_on = MIDI Note On
kind_summary_midi_input_note_on = Спрацьовує, коли надходить повідомлення MIDI Note On.
kind_name_midi_input_pitch_bend = MIDI Pitch Bend
kind_summary_midi_input_pitch_bend = Спрацьовує, коли надходить повідомлення MIDI Pitch Bend.
kind_name_midi_input_program_change = MIDI Program Change
kind_summary_midi_input_program_change = Спрацьовує, коли надходить повідомлення MIDI Program Change.
kind_name_midi_send = Надіслати MIDI-повідомлення
kind_summary_midi_send = Надсилає MIDI-повідомлення (Note On, Note Off, CC або сире) у вихідний порт.
kind_name_overlay_send = Надіслати в оверлей
kind_summary_overlay_send = Надіслати вміст в оверлей для браузерного джерела
kind_name_script_emit_event = Надіслати власну подію
kind_summary_script_emit_event = Опублікувати іменовану власну подію в шину
kind_name_script_event_custom = Власна подія скрипту
kind_summary_script_event_custom = Спрацьовує, коли rhai-скрипт публікує іменовану власну подію
kind_name_script_run_inline = Виконати вбудований скрипт
kind_summary_script_run_inline = Виконати фрагмент rhai-скрипту, записаний прямо в налаштуваннях
kind_name_script_run_named = Виконати скрипт (за назвою)
kind_summary_script_run_named = Виконати збережений скрипт за назвою
kind_name_server_broadcast = Розіслати клієнтам оверлеїв
kind_summary_server_broadcast = Надіслати JSON-повідомлення всім підключеним клієнтам WebSocket
kind_name_soundboard_sound_play = Відтворити звук
kind_summary_soundboard_sound_play = Відтворити кліп звукової панелі на налаштованому пристрої виводу
kind_name_soundboard_sound_stop = Зупинити звук
kind_summary_soundboard_sound_stop = Зупинити кліп звукової панелі (порожній кліп зупиняє все)
kind_name_soundboard_sound_stop_all = Зупинити всі звуки
kind_summary_soundboard_sound_stop_all = Зупинити всі кліпи звукової панелі, що зараз грають
kind_name_soundboard_volume_set_master = Задати загальну гучність
kind_summary_soundboard_volume_set_master = Задати загальну гучність звукової панелі для всіх кліпів
kind_name_timer_tick = Таймер з інтервалом
kind_summary_timer_tick = Спрацьовує кожні N хвилин, за потреби лише під час трансляції та після достатньої активності в чаті
kind_name_tts_alias_set = Задати голосовий псевдонім
kind_summary_tts_alias_set = Призначити псевдоніму глядача конкретний рушій і голос
kind_name_tts_alias_switch_active = Змінити голос псевдоніма
kind_summary_tts_alias_switch_active = Перевести наявний псевдонім глядача на інший рушій і голос; нічого не робить, якщо в глядача немає псевдоніма
kind_name_tts_queue_clear = Очистити чергу TTS
kind_summary_tts_queue_clear = Скинути елементи черги TTS і за потреби зупинити поточне мовлення
kind_name_tts_queue_pause = Призупинити чергу TTS
kind_summary_tts_queue_pause = Призупинити обробку черги мовлення TTS
kind_name_tts_queue_resume = Відновити чергу TTS
kind_summary_tts_queue_resume = Відновити обробку черги мовлення TTS після паузи
kind_name_tts_queue_skip_current = Пропустити поточне TTS
kind_summary_tts_queue_skip_current = Пропустити поточний елемент TTS і перейти до наступного в черзі
kind_name_tts_speak_stop = Зупинити мовлення
kind_summary_tts_speak_stop = Перервати поточне мовлення TTS
kind_name_tts_speak_text = Озвучити текст
kind_summary_tts_speak_text = Надіслати текст у чергу мовлення TTS, за потреби з вибраним голосом рушія
kind_name_tts_speak_text_with_engine = Озвучити іншим рушієм
kind_summary_tts_speak_text_with_engine = Поставити мовлення TTS у чергу з конкретним рушієм
kind_name_twitch_automod_add_blocked_term = Додати заблокований термін
kind_summary_twitch_automod_add_blocked_term = Додати слово або фразу до списку заблокованих термінів AutoMod
kind_name_twitch_automod_approve_message = Схвалити повідомлення AutoMod
kind_summary_twitch_automod_approve_message = Дозволити затриманому AutoMod повідомленню з'явитися в чаті
kind_name_twitch_automod_deny_message = Відхилити повідомлення AutoMod
kind_summary_twitch_automod_deny_message = Відхилити затримане AutoMod повідомлення, щоб прибрати його з черги
kind_name_twitch_automod_message_held = AutoMod затримав повідомлення
kind_summary_twitch_automod_message_held = Спрацьовує, коли AutoMod затримує повідомлення чату на перевірку модератором
kind_name_twitch_automod_message_updated = Рішення AutoMod оновлено
kind_summary_twitch_automod_message_updated = Спрацьовує, коли модератор схвалює, відхиляє або дає затриманому AutoMod повідомленню закінчитися
kind_name_twitch_automod_remove_blocked_term = Видалити заблокований термін
kind_summary_twitch_automod_remove_blocked_term = Видалити термін зі списку заблокованих термінів AutoMod за його ID
kind_name_twitch_automod_settings_updated = Налаштування AutoMod оновлено
kind_summary_twitch_automod_settings_updated = Спрацьовує, коли модератор змінює налаштування фільтрів AutoMod каналу
kind_name_twitch_automod_terms_updated = Терміни AutoMod оновлено
kind_summary_twitch_automod_terms_updated = Спрацьовує, коли модератор додає або видаляє дозволені чи заблоковані терміни AutoMod
kind_name_twitch_automod_update_settings = Оновити налаштування AutoMod
kind_summary_twitch_automod_update_settings = Задати загальний рівень AutoMod або рівні окремих категорій фільтрів
kind_name_twitch_channel_ban = Глядача забанено (назавжди)
kind_summary_twitch_channel_ban = Спрацьовує, коли глядача назавжди банять у каналі
kind_name_twitch_channel_cancel_raid = Скасувати рейд
kind_summary_twitch_channel_cancel_raid = Скасувати запланований рейд, запущений із цього каналу
kind_name_twitch_channel_chat_settings_updated = Налаштування чату оновлено
kind_summary_twitch_channel_chat_settings_updated = Спрацьовує, коли змінюються налаштування чату стримера (лише смайли, лише фоловери, повільний режим, лише підписники, унікальний чат)
kind_name_twitch_channel_create_marker = Створити маркер трансляції
kind_summary_twitch_channel_create_marker = Поставити позначку на поточному моменті трансляції
kind_name_twitch_channel_follow = Фоловер
kind_summary_twitch_channel_follow = Спрацьовує, коли глядач фоловить ваш канал
kind_name_twitch_channel_moderator_added = Модератора додано
kind_summary_twitch_channel_moderator_added = Спрацьовує, коли глядач отримує статус модератора в каналі
kind_name_twitch_channel_moderator_removed = Модератора знято
kind_summary_twitch_channel_moderator_removed = Спрацьовує, коли з глядача знімають статус модератора
kind_name_twitch_channel_raid_received = Отримано рейд
kind_summary_twitch_channel_raid_received = Спрацьовує, коли інший стример рейдить ваш канал
kind_name_twitch_channel_raid_sent = Рейд надіслано
kind_summary_twitch_channel_raid_sent = Спрацьовує, коли ви рейдите канал іншого стримера
kind_name_twitch_channel_run_ad = Запустити рекламу
kind_summary_twitch_channel_run_ad = Запустити рекламну паузу в каналі стримера
kind_name_twitch_channel_send_shoutout = Надіслати шаутаут
kind_summary_twitch_channel_send_shoutout = Надіслати шаутаут іншому стримеру в чаті каналу
kind_name_twitch_channel_shield_mode_ended = Режим захисту вимкнено
kind_summary_twitch_channel_shield_mode_ended = Спрацьовує, коли модератор вимикає режим захисту в каналі
kind_name_twitch_channel_shield_mode_started = Режим захисту ввімкнено
kind_summary_twitch_channel_shield_mode_started = Спрацьовує, коли модератор вмикає режим захисту в каналі
kind_name_twitch_channel_shoutout_received = Отримано шаутаут
kind_summary_twitch_channel_shoutout_received = Спрацьовує, коли інший стример робить шаутаут каналу
kind_name_twitch_channel_shoutout_sent = Шаутаут надіслано
kind_summary_twitch_channel_shoutout_sent = Спрацьовує, коли стример або модератор робить шаутаут іншому каналу
kind_name_twitch_channel_snooze_ad = Відкласти наступну рекламу
kind_summary_twitch_channel_snooze_ad = Відкласти наступну заплановану рекламну паузу на 5 хвилин
kind_name_twitch_channel_start_raid = Почати рейд
kind_summary_twitch_channel_start_raid = Почати рейд на канал іншого стримера
kind_name_twitch_channel_suspicious_user_message = Повідомлення підозрілого користувача
kind_summary_twitch_channel_suspicious_user_message = Спрацьовує, коли користувач, позначений як підозрілий, пише в каналі
kind_name_twitch_channel_timeout = Глядачу дано таймаут
kind_summary_twitch_channel_timeout = Спрацьовує, коли глядач отримує таймаут у каналі
kind_name_twitch_channel_unban = Глядача розбанено
kind_summary_twitch_channel_unban = Спрацьовує, коли з глядача знімають бан або таймаут
kind_name_twitch_channel_unban_request_created = Запит на розбан створено
kind_summary_twitch_channel_unban_request_created = Спрацьовує, коли забанений глядач подає запит на розбан
kind_name_twitch_channel_unban_request_resolved = Запит на розбан розглянуто
kind_summary_twitch_channel_unban_request_resolved = Спрацьовує, коли модератор схвалює, відхиляє або скасовує запит на розбан
kind_name_twitch_channel_update = Канал оновлено
kind_summary_twitch_channel_update = Спрацьовує, коли стример змінює назву, категорію або мову каналу
kind_name_twitch_channel_update_category = Змінити категорію трансляції
kind_summary_twitch_channel_update_category = Змінити гру чи категорію стримера за її Helix game_id або точною назвою категорії
kind_name_twitch_channel_update_tags = Змінити теги трансляції
kind_summary_twitch_channel_update_tags = Замінити теги трансляції стримера (до 10, кожен до 25 символів); порожнє значення прибирає всі теги
kind_name_twitch_channel_update_title = Змінити назву трансляції
kind_summary_twitch_channel_update_title = Оновити назву трансляції стримера
kind_name_twitch_channel_vip_added = VIP додано
kind_summary_twitch_channel_vip_added = Спрацьовує, коли глядач отримує статус VIP у каналі
kind_name_twitch_channel_vip_removed = VIP знято
kind_summary_twitch_channel_vip_removed = Спрацьовує, коли з глядача знімають статус VIP
kind_name_twitch_channel_warning_acknowledged = Попередження прийнято
kind_summary_twitch_channel_warning_acknowledged = Спрацьовує, коли користувач підтверджує попередження від модератора
kind_name_twitch_channel_warning_sent = Попередження надіслано
kind_summary_twitch_channel_warning_sent = Спрацьовує, коли модератор надсилає користувачу попередження
kind_name_twitch_channel_points_automatic_reward_redeemed = Отримано автоматичну нагороду
kind_summary_twitch_channel_points_automatic_reward_redeemed = Спрацьовує, коли глядач отримує вбудовану автоматичну нагороду за бали каналу
kind_name_twitch_channel_points_cancel_redemption = Скасувати отримання нагороди
kind_summary_twitch_channel_points_cancel_redemption = Скасувати отримання нагороди за бали каналу й повернути глядачу бали
kind_name_twitch_channel_points_create_reward = Створити нагороду за бали каналу
kind_summary_twitch_channel_points_create_reward = Створити нову власну нагороду за бали каналу на каналі стримера
kind_name_twitch_channel_points_delete_reward = Видалити нагороду за бали каналу
kind_summary_twitch_channel_points_delete_reward = Остаточно видалити власну нагороду за бали каналу
kind_name_twitch_channel_points_disable_reward = Вимкнути нагороду за бали каналу
kind_summary_twitch_channel_points_disable_reward = Вимкнути власну нагороду за бали каналу, щоб глядачі не могли її отримати
kind_name_twitch_channel_points_enable_reward = Увімкнути нагороду за бали каналу
kind_summary_twitch_channel_points_enable_reward = Увімкнути власну нагороду за бали каналу, щоб глядачі могли її отримати
kind_name_twitch_channel_points_fulfill_redemption = Виконати отримання нагороди
kind_summary_twitch_channel_points_fulfill_redemption = Позначити отримання нагороди за бали каналу як виконане
kind_name_twitch_channel_points_pause_reward = Призупинити нагороду за бали каналу
kind_summary_twitch_channel_points_pause_reward = Призупинити власну нагороду за бали каналу: нові отримання стають у чергу без виконання
kind_name_twitch_channel_points_redemption = Отримання нагороди
kind_summary_twitch_channel_points_redemption = Спрацьовує, коли глядач отримує нагороду за бали каналу
kind_name_twitch_channel_points_redemption_updated = Отримання нагороди оновлено
kind_summary_twitch_channel_points_redemption_updated = Спрацьовує, коли отримання нагороди за бали каналу виконано або скасовано
kind_name_twitch_channel_points_resume_reward = Відновити нагороду за бали каналу
kind_summary_twitch_channel_points_resume_reward = Відновити призупинену власну нагороду за бали каналу, щоб отримання виконувалися як звичайно
kind_name_twitch_channel_points_reward_added = Нагороду за бали каналу додано
kind_summary_twitch_channel_points_reward_added = Спрацьовує, коли на каналі стримера створюють нову нагороду за бали каналу
kind_name_twitch_channel_points_reward_removed = Нагороду за бали каналу видалено
kind_summary_twitch_channel_points_reward_removed = Спрацьовує, коли з каналу стримера видаляють нагороду за бали каналу
kind_name_twitch_channel_points_reward_updated = Нагороду за бали каналу змінено
kind_summary_twitch_channel_points_reward_updated = Спрацьовує, коли на каналі стримера змінюють наявну нагороду за бали каналу
kind_name_twitch_channel_points_update_reward = Оновити нагороду за бали каналу
kind_summary_twitch_channel_points_update_reward = Оновити вибрані поля наявної власної нагороди за бали каналу
kind_name_twitch_chat_cheer_message = Повідомлення з бітсами
kind_summary_twitch_chat_cheer_message = Спрацьовує, коли повідомлення чату містить бітси
kind_name_twitch_chat_clear = Очистити чат
kind_summary_twitch_chat_clear = Видалити всі повідомлення з чату Twitch
kind_name_twitch_chat_cleared = Чат очищено
kind_summary_twitch_chat_cleared = Спрацьовує, коли з чату видаляють усі повідомлення
kind_name_twitch_chat_command = Команда чату
kind_summary_twitch_chat_command = Спрацьовує, коли повідомлення чату збігається з фразою команди
kind_name_twitch_chat_delete_message = Видалити повідомлення
kind_summary_twitch_chat_delete_message = Видалити конкретне повідомлення з чату Twitch
kind_name_twitch_chat_message = Повідомлення чату
kind_summary_twitch_chat_message = Спрацьовує на кожне повідомлення в чаті
kind_name_twitch_chat_message_deleted = Повідомлення чату видалено
kind_summary_twitch_chat_message_deleted = Спрацьовує, коли модератор або бот видаляє повідомлення чату
kind_name_twitch_chat_reply = Відповісти на повідомлення
kind_summary_twitch_chat_reply = Надіслати відповідь на конкретне повідомлення чату
kind_name_twitch_chat_send_announcement = Надіслати оголошення
kind_summary_twitch_chat_send_announcement = Опублікувати виділене оголошення в чаті Twitch
kind_name_twitch_chat_send_message = Надіслати повідомлення в чат
kind_summary_twitch_chat_send_message = Надіслати повідомлення в чат каналу платформи
kind_name_twitch_chat_send_whisper = Надіслати шепіт
kind_summary_twitch_chat_send_whisper = Надіслати особистий шепіт іншому користувачу
kind_name_twitch_chat_set_mode = Задати режим чату
kind_summary_twitch_chat_set_mode = Налаштувати режим лише смайлів, лише фоловерів, повільний, лише підписників або унікального чату
kind_name_twitch_chat_whisper = Отримано шепіт
kind_summary_twitch_chat_whisper = Спрацьовує, коли акаунт бота отримує шепіт
kind_name_twitch_goal_ended = Ціль завершено
kind_summary_twitch_goal_ended = Спрацьовує, коли ціль автора завершується, досягнута чи ні
kind_name_twitch_goal_get_current = Отримати поточну ціль
kind_summary_twitch_goal_get_current = Прочитати активну ціль автора на каналі стримера й передати її дані у змінні
kind_name_twitch_goal_progress = Прогрес цілі
kind_summary_twitch_goal_progress = Спрацьовує, коли ціль автора отримує оновлення прогресу
kind_name_twitch_goal_started = Ціль розпочато
kind_summary_twitch_goal_started = Спрацьовує, коли на каналі стримера починається ціль автора
kind_name_twitch_guest_star_assign_slot = Призначити слот Guest Star
kind_summary_twitch_guest_star_assign_slot = Призначити запрошеного гостя на пронумерований слот у сесії Guest Star
kind_name_twitch_guest_star_end_session = Завершити сесію Guest Star
kind_summary_twitch_guest_star_end_session = Завершити активну сесію Guest Star на каналі стримера
kind_name_twitch_guest_star_guest_updated = Гостя Guest Star оновлено
kind_summary_twitch_guest_star_guest_updated = Спрацьовує, коли в сесії змінюється стан гостя або слота Guest Star
kind_name_twitch_guest_star_invite = Запросити в Guest Star
kind_summary_twitch_guest_star_invite = Запросити глядача приєднатися до активної сесії Guest Star
kind_name_twitch_guest_star_remove_guest = Прибрати гостя Guest Star
kind_summary_twitch_guest_star_remove_guest = Прибрати гостя з його слота в активній сесії Guest Star
kind_name_twitch_guest_star_session_began = Сесію Guest Star розпочато
kind_summary_twitch_guest_star_session_began = Спрацьовує, коли стример починає нову сесію Guest Star
kind_name_twitch_guest_star_session_ended = Сесію Guest Star завершено
kind_summary_twitch_guest_star_session_ended = Спрацьовує, коли поточна сесія Guest Star завершується
kind_name_twitch_guest_star_settings_updated = Налаштування Guest Star оновлено
kind_summary_twitch_guest_star_settings_updated = Спрацьовує, коли змінюються налаштування Guest Star стримера
kind_name_twitch_guest_star_update_slot = Змінити налаштування слота Guest Star
kind_summary_twitch_guest_star_update_slot = Змінити звук, відео або гучність слота в активній сесії Guest Star
kind_name_twitch_moderation_add_moderator = Додати модератора
kind_summary_twitch_moderation_add_moderator = Надати користувачу статус модератора в каналі
kind_name_twitch_moderation_add_vip = Додати VIP
kind_summary_twitch_moderation_add_vip = Надати користувачу статус VIP у каналі
kind_name_twitch_moderation_ban_user = Забанити користувача
kind_summary_twitch_moderation_ban_user = Назавжди забанити користувача в каналі
kind_name_twitch_moderation_remove_moderator = Зняти модератора
kind_summary_twitch_moderation_remove_moderator = Зняти з користувача статус модератора в каналі
kind_name_twitch_moderation_remove_vip = Зняти VIP
kind_summary_twitch_moderation_remove_vip = Зняти з користувача статус VIP у каналі
kind_name_twitch_moderation_shield_mode_off = Вимкнути режим захисту
kind_summary_twitch_moderation_shield_mode_off = Вимкнути режим захисту на каналі стримера
kind_name_twitch_moderation_shield_mode_on = Увімкнути режим захисту
kind_summary_twitch_moderation_shield_mode_on = Увімкнути режим захисту на каналі стримера
kind_name_twitch_moderation_timeout_user = Дати таймаут користувачу
kind_summary_twitch_moderation_timeout_user = Дати користувачу таймаут у чаті каналу
kind_name_twitch_moderation_unban_user = Розбанити користувача
kind_summary_twitch_moderation_unban_user = Зняти з користувача бан або активний таймаут
kind_name_twitch_moderation_warn_user = Попередити користувача
kind_summary_twitch_moderation_warn_user = Надіслати користувачу офіційне попередження Twitch у каналі
kind_name_twitch_poll_end = Завершити опитування
kind_summary_twitch_poll_end = Негайно завершити активне опитування: 'terminated' лишає результати видимими, 'archived' приховує їх
kind_name_twitch_poll_ended = Опитування завершено
kind_summary_twitch_poll_ended = Спрацьовує, коли опитування завершується (виконано, архівовано або перервано)
kind_name_twitch_poll_progress = Прогрес опитування
kind_summary_twitch_poll_progress = Спрацьовує, коли в активному опитуванні оновлюється кількість голосів
kind_name_twitch_poll_start = Почати опитування
kind_summary_twitch_poll_start = Створити нове опитування Twitch; повертає poll.id для ланцюжка з кроком "Завершити опитування"
kind_name_twitch_poll_started = Опитування розпочато
kind_summary_twitch_poll_started = Спрацьовує, коли на каналі стримера починається опитування
kind_name_twitch_prediction_cancel = Скасувати прогноз
kind_summary_twitch_prediction_cancel = Скасувати активний прогноз і повернути всі бали каналу учасникам
kind_name_twitch_prediction_ended = Прогноз завершено
kind_summary_twitch_prediction_ended = Спрацьовує, коли прогноз визначено або скасовано
kind_name_twitch_prediction_lock = Закрити прогноз
kind_summary_twitch_prediction_lock = Закрити активний прогноз для нових ставок; глядачі більше не можуть змінити свій вибір
kind_name_twitch_prediction_locked = Прогноз закрито
kind_summary_twitch_prediction_locked = Спрацьовує, коли прогноз закрито і ставити бали вже не можна
kind_name_twitch_prediction_progress = Прогрес прогнозу
kind_summary_twitch_prediction_progress = Спрацьовує, коли в активному прогнозі оновлюються суми балів
kind_name_twitch_prediction_resolve = Визначити результат прогнозу
kind_summary_twitch_prediction_resolve = Завершити прогноз, оголосивши переможний результат, і розподілити бали переможцям
kind_name_twitch_prediction_start = Почати прогноз
kind_summary_twitch_prediction_start = Створити новий прогноз Twitch; повертає prediction.id для ланцюжка
kind_name_twitch_prediction_started = Прогноз розпочато
kind_summary_twitch_prediction_started = Спрацьовує, коли на каналі стримера починається прогноз
kind_name_twitch_shared_chat_message_received = Повідомлення спільного чату
kind_summary_twitch_shared_chat_message_received = Спрацьовує, коли надходить повідомлення з іншого каналу через сесію спільного чату
kind_name_twitch_shared_chat_session_began = Сесію спільного чату розпочато
kind_summary_twitch_shared_chat_session_began = Спрацьовує, коли канал стримера приєднується до активної сесії спільного чату
kind_name_twitch_shared_chat_session_ended = Сесію спільного чату завершено
kind_summary_twitch_shared_chat_session_ended = Спрацьовує, коли канал стримера виходить із сесії спільного чату або сесія завершується
kind_name_twitch_shared_chat_session_updated = Сесію спільного чату оновлено
kind_summary_twitch_shared_chat_session_updated = Спрацьовує, коли змінюється активна сесія спільного чату, у якій перебуває канал стримера
kind_name_twitch_stream_ad_break_started = Рекламна пауза почалася
kind_summary_twitch_stream_ad_break_started = Спрацьовує, коли на каналі стримера починається рекламна пауза
kind_name_twitch_stream_offline = Трансляцію завершено
kind_summary_twitch_stream_offline = Спрацьовує, коли ваша трансляція закінчується
kind_name_twitch_stream_online = Трансляція в ефірі
kind_summary_twitch_stream_online = Спрацьовує, коли ваша трансляція виходить в ефір
kind_name_twitch_support_charity_donation = Благодійний донат
kind_summary_twitch_support_charity_donation = Спрацьовує, коли глядач робить донат в активну благодійну кампанію
kind_name_twitch_support_charity_progress = Прогрес благодійної кампанії
kind_summary_twitch_support_charity_progress = Спрацьовує на кожне оновлення прогресу благодійної кампанії
kind_name_twitch_support_charity_started = Благодійну кампанію розпочато
kind_summary_twitch_support_charity_started = Спрацьовує, коли стример починає благодійну кампанію
kind_name_twitch_support_charity_stopped = Благодійну кампанію завершено
kind_summary_twitch_support_charity_stopped = Спрацьовує, коли стример завершує благодійну кампанію
kind_name_twitch_support_cheer = Отримано бітси
kind_summary_twitch_support_cheer = Спрацьовує, коли глядач надсилає бітси
kind_name_twitch_support_gift_sub = Подарована підписка
kind_summary_twitch_support_gift_sub = Спрацьовує, коли користувач дарує одну або кілька підписок
kind_name_twitch_support_hype_train_ended = Hype Train завершився
kind_summary_twitch_support_hype_train_ended = Спрацьовує, коли на каналі завершується Hype Train
kind_name_twitch_support_hype_train_progress = Прогрес Hype Train
kind_summary_twitch_support_hype_train_progress = Спрацьовує на кожне оновлення прогресу Hype Train на налаштованому рівні або вище
kind_name_twitch_support_hype_train_started = Hype Train почався
kind_summary_twitch_support_hype_train_started = Спрацьовує, коли на каналі починається Hype Train
kind_name_twitch_support_resubscriber = Повторна підписка
kind_summary_twitch_support_resubscriber = Спрацьовує, коли глядач продовжує підписку
kind_name_twitch_support_subscriber = Новий підписник
kind_summary_twitch_support_subscriber = Спрацьовує на першу підписку на канал
kind_name_twitch_user_update = Профіль користувача оновлено
kind_summary_twitch_user_update = Спрацьовує, коли автентифікований користувач змінює опис профілю або відображуване ім'я
kind_name_youtube_channel_member = Новий спонсор
kind_summary_youtube_channel_member = Спрацьовує, коли глядач стає новим спонсором каналу YouTube
kind_name_youtube_channel_member_gift = Подаровано спонсорства
kind_summary_youtube_channel_member_gift = Спрацьовує, коли глядач дарує набір спонсорств каналу YouTube
kind_name_youtube_channel_member_gift_received = Отримано подароване спонсорство
kind_summary_youtube_channel_member_gift_received = Спрацьовує, коли глядач отримує подароване спонсорство каналу YouTube
kind_name_youtube_channel_member_milestone = Етап спонсорства
kind_summary_youtube_channel_member_milestone = Спрацьовує, коли спонсор каналу YouTube досягає етапу спонсорства
kind_name_youtube_channel_user_banned = Користувача забанено
kind_summary_youtube_channel_user_banned = Спрацьовує, коли користувача банять (назавжди або тимчасово) у живому чаті YouTube
kind_name_youtube_chat_command = Команда чату
kind_summary_youtube_chat_command = Спрацьовує, коли повідомлення чату YouTube збігається з фразою команди
kind_name_youtube_chat_create_poll = Створити опитування
kind_summary_youtube_chat_create_poll = Опублікувати опитування в активному живому чаті YouTube
kind_name_youtube_chat_delete_message = Видалити повідомлення
kind_summary_youtube_chat_delete_message = Видалити повідомлення з активного живого чату YouTube за його ID
kind_name_youtube_chat_message = Повідомлення чату
kind_summary_youtube_chat_message = Спрацьовує на кожне повідомлення в живому чаті YouTube
kind_name_youtube_chat_message_deleted = Повідомлення видалено
kind_summary_youtube_chat_message_deleted = Спрацьовує, коли модератор видаляє повідомлення з живого чату YouTube
kind_name_youtube_chat_send_message = Надіслати повідомлення в чат
kind_summary_youtube_chat_send_message = Надіслати повідомлення в активний живий чат YouTube
kind_name_youtube_chat_super_chat = Super Chat
kind_summary_youtube_chat_super_chat = Спрацьовує, коли глядач надсилає Super Chat у живому чаті YouTube
kind_name_youtube_chat_super_sticker = Super Sticker
kind_summary_youtube_chat_super_sticker = Спрацьовує, коли глядач надсилає Super Sticker у живому чаті YouTube
kind_name_youtube_lookup_stream_stats = Статистика трансляції
kind_summary_youtube_lookup_stream_stats = Отримати дані активної трансляції YouTube
kind_name_youtube_lookup_viewer = Знайти глядача
kind_summary_youtube_lookup_viewer = Знайти канал YouTube за псевдонімом (@handle) або ID каналу
kind_name_youtube_moderation_add_moderator = Додати модератора
kind_summary_youtube_moderation_add_moderator = Надати користувачу статус модератора в активному живому чаті YouTube
kind_name_youtube_moderation_ban_user = Забанити користувача
kind_summary_youtube_moderation_ban_user = Назавжди забанити користувача в активному живому чаті YouTube
kind_name_youtube_moderation_remove_moderator = Зняти модератора
kind_summary_youtube_moderation_remove_moderator = Зняти з користувача статус модератора в активному живому чаті YouTube
kind_name_youtube_moderation_timeout_user = Дати таймаут користувачу
kind_summary_youtube_moderation_timeout_user = Дати користувачу таймаут в активному живому чаті YouTube
kind_name_youtube_moderation_unban_user = Розбанити користувача
kind_summary_youtube_moderation_unban_user = Зняти бан або таймаут, виданий раніше в цьому сеансі
kind_name_youtube_stream_insert_ad_break = Вставити рекламну паузу
kind_summary_youtube_stream_insert_ad_break = Запустити рекламну вставку посеред активної трансляції YouTube
kind_name_youtube_stream_offline = Трансляцію завершено
kind_summary_youtube_stream_offline = Спрацьовує, коли трансляція YouTube закінчується
kind_name_youtube_stream_online = Трансляція в ефірі
kind_summary_youtube_stream_online = Спрацьовує, коли трансляція YouTube виходить в ефір
kind_name_youtube_stream_set_thumbnail = Задати значок відео
kind_summary_youtube_stream_set_thumbnail = Завантажити власне зображення значка для активної трансляції YouTube
kind_name_youtube_stream_title_changed = Назву трансляції змінено
kind_summary_youtube_stream_title_changed = Спрацьовує, коли змінюють назву активної трансляції YouTube
kind_name_youtube_stream_update_category = Змінити категорію трансляції
kind_summary_youtube_stream_update_category = Задати категорію відео для активної трансляції YouTube
kind_name_youtube_stream_update_description = Змінити опис трансляції
kind_summary_youtube_stream_update_description = Задати опис активної трансляції YouTube
kind_name_youtube_stream_update_privacy = Змінити приватність трансляції
kind_summary_youtube_stream_update_privacy = Задати статус приватності активної трансляції YouTube
kind_name_youtube_stream_update_title = Змінити назву трансляції
kind_summary_youtube_stream_update_title = Задати назву активної трансляції YouTube
kind_name_kick_channel_hosted = Хост каналу
kind_summary_kick_channel_hosted = Спрацьовує, коли інший стример хостить цей канал Kick
kind_name_kick_channel_reward_redemption_updated = Отримання нагороди
kind_summary_kick_channel_reward_redemption_updated = Спрацьовує, коли глядач отримує нагороду каналу Kick
kind_name_kick_channel_subscribed = Підписка
kind_summary_kick_channel_subscribed = Спрацьовує, коли глядач підписується на канал Kick
kind_name_kick_channel_subscription_gifts = Подаровані підписки
kind_summary_kick_channel_subscription_gifts = Спрацьовує, коли глядач дарує підписки на канал Kick
kind_name_kick_channel_update_info = Оновити дані каналу
kind_summary_kick_channel_update_info = Змінити назву, категорію або теги трансляції на каналі Kick. Потрібен дозвіл channel:write
kind_name_kick_chat_command = Команда чату
kind_summary_kick_chat_command = Спрацьовує, коли повідомлення чату Kick починається з фрази команди
kind_name_kick_chat_delete_message = Видалити повідомлення
kind_summary_kick_chat_delete_message = Видалити повідомлення з чату каналу Kick за його ID. Потрібен дозвіл moderation:chat_message:manage
kind_name_kick_chat_message_deleted = Повідомлення видалено
kind_summary_kick_chat_message_deleted = Спрацьовує, коли модератор видаляє повідомлення в чаті Kick
kind_name_kick_chat_message_sent = Повідомлення чату
kind_summary_kick_chat_message_sent = Спрацьовує на кожне повідомлення в живому чаті Kick
kind_name_kick_chat_send_message = Надіслати повідомлення в чат
kind_summary_kick_chat_send_message = Надіслати повідомлення в чат каналу Kick
kind_name_kick_livestream_metadata_updated = Дані трансляції
kind_summary_kick_livestream_metadata_updated = Спрацьовує, коли змінюється назва або категорія трансляції каналу Kick
kind_name_kick_livestream_status_updated = Статус трансляції
kind_summary_kick_livestream_status_updated = Спрацьовує, коли змінюється статус трансляції каналу Kick (в ефірі чи офлайн)
kind_name_kick_lookup_category = Знайти категорію
kind_summary_kick_lookup_category = Шукати категорії Kick за назвою й повертати їхні числові ID
kind_name_kick_lookup_stream_stats = Статистика трансляції
kind_summary_kick_lookup_stream_stats = Отримати дані поточної трансляції вашого каналу Kick
kind_name_kick_lookup_user = Знайти користувача
kind_summary_kick_lookup_user = Знайти канал Kick за адресою (slug) і повідомити його дані та стан ефіру
kind_name_kick_moderation_ban = Забанити користувача
kind_summary_kick_moderation_ban = Назавжди забанити користувача на каналі Kick. Потрібен дозвіл moderation:ban
kind_name_kick_moderation_banned = Користувача забанено
kind_summary_kick_moderation_banned = Спрацьовує, коли модератор банить користувача в чаті Kick
kind_name_kick_moderation_timeout = Дати таймаут користувачу
kind_summary_kick_moderation_timeout = Дати користувачу таймаут на каналі Kick. Тривалість 1-10080 хвилин. Потрібен дозвіл moderation:ban
kind_name_kick_moderation_unban = Розбанити користувача
kind_summary_kick_moderation_unban = Зняти з користувача активний бан або таймаут на каналі Kick. Потрібен дозвіл moderation:ban
kind_name_kick_reward_create = Створити нагороду каналу
kind_summary_kick_reward_create = Створити нову нагороду за бали каналу на Kick. Потрібен дозвіл channel:rewards:write
kind_name_kick_reward_delete = Видалити нагороду каналу
kind_summary_kick_reward_delete = Остаточно видалити нагороду каналу Kick. Потрібні дозвіл channel:rewards:write і право власності на нагороду
kind_name_kick_reward_redemption_accept = Прийняти отримання нагороди
kind_summary_kick_reward_redemption_accept = Прийняти одне або кілька очікуваних отримань нагород Kick. Потрібен дозвіл channel:rewards:write
kind_name_kick_reward_redemption_reject = Відхилити отримання нагороди
kind_summary_kick_reward_redemption_reject = Відхилити одне або кілька очікуваних отримань нагород Kick. Потрібен дозвіл channel:rewards:write
kind_name_kick_reward_update = Оновити нагороду каналу
kind_summary_kick_reward_update = Оновити наявну нагороду каналу Kick. Потрібні дозвіл channel:rewards:write і право власності на нагороду
kind_name_vtube_expression_set = Задати вираз
kind_summary_vtube_expression_set = Увімкнути або вимкнути вираз VTube Studio
kind_name_vtube_expression_state_changed = Стан виразу VTube Studio змінено
kind_summary_vtube_expression_state_changed = Спрацьовує, коли вираз VTube Studio вмикається або вимикається
kind_name_vtube_hotkey_trigger = Натиснути гарячу клавішу
kind_summary_vtube_hotkey_trigger = Запустити гарячу клавішу VTube Studio за її ID
kind_name_vtube_hotkey_triggered = Гарячу клавішу VTube Studio натиснуто
kind_summary_vtube_hotkey_triggered = Спрацьовує, коли активується гаряча клавіша VTube Studio
kind_name_vtube_item_added = Предмет VTube Studio додано
kind_summary_vtube_item_added = Спрацьовує, коли предмет завантажується на сцену VTube Studio
kind_name_vtube_item_load = Завантажити предмет
kind_summary_vtube_item_load = Завантажити файл предмета на сцену VTube Studio
kind_name_vtube_item_move = Перемістити предмет
kind_summary_vtube_item_move = Перемістити, змінити розмір або повернути завантажений предмет VTube Studio на екрані
kind_name_vtube_item_pin = Закріпити предмет
kind_summary_vtube_item_pin = Закріпити або відкріпити завантажений предмет VTube Studio на поточній моделі
kind_name_vtube_item_removed = Предмет VTube Studio прибрано
kind_summary_vtube_item_removed = Спрацьовує, коли предмет прибирають зі сцени VTube Studio
kind_name_vtube_item_throw = Кинути предмет
kind_summary_vtube_item_throw = Створити предмет і анімувати його політ через сцену VTube Studio
kind_name_vtube_item_unload_all = Прибрати всі предмети
kind_summary_vtube_item_unload_all = Прибрати всі предмети, завантажені зараз на сцену VTube Studio
kind_name_vtube_lookup_current_model = Отримати поточну модель
kind_summary_vtube_lookup_current_model = Запитати у VTube Studio поточну завантажену модель
kind_name_vtube_lookup_expressions = Отримати вирази
kind_summary_vtube_lookup_expressions = Запитати у VTube Studio вирази поточної моделі
kind_name_vtube_lookup_hotkeys = Отримати гарячі клавіші
kind_summary_vtube_lookup_hotkeys = Запитати у VTube Studio гарячі клавіші поточної моделі
kind_name_vtube_lookup_items = Отримати предмети
kind_summary_vtube_lookup_items = Запитати у VTube Studio екземпляри предметів, що зараз є на сцені
kind_name_vtube_lookup_parameters = Отримати параметри
kind_summary_vtube_lookup_parameters = Запитати у VTube Studio вхідні та Live2D-параметри поточної моделі
kind_name_vtube_model_config_changed = Налаштування моделі VTube Studio змінено
kind_summary_vtube_model_config_changed = Спрацьовує, коли змінюються налаштування активної моделі VTube Studio
kind_name_vtube_model_load = Завантажити модель
kind_summary_vtube_model_load = Завантажити модель VTube Studio за її ID
kind_name_vtube_model_loaded = Модель VTube Studio завантажено
kind_summary_vtube_model_loaded = Спрацьовує, коли завантажується модель VTube Studio
kind_name_vtube_model_move = Перемістити модель
kind_summary_vtube_model_move = Перемістити або повернути модель VTube Studio на екрані
kind_name_vtube_model_set_physics = Перемкнути фізику
kind_summary_vtube_model_set_physics = Тимчасово перевизначити силу фізики моделі VTube Studio
kind_name_vtube_model_tint = Підфарбувати модель
kind_summary_vtube_model_tint = Накласти кольоровий відтінок на кожен ArtMesh моделі VTube Studio
kind_name_vtube_model_unloaded = Модель VTube Studio вивантажено
kind_summary_vtube_model_unloaded = Спрацьовує, коли модель VTube Studio вивантажується
kind_name_vtube_param_set = Задати параметр
kind_summary_vtube_param_set = Передати значення в параметр VTube Studio, за потреби утримуючи його певний час
kind_name_vtube_params_reset = Повернути в стан спокою
kind_summary_vtube_params_reset = Вимкнути всі активні вирази й повернути модель у стан спокою
kind_name_vtube_tracking_face_found = Обличчя знайдено
kind_summary_vtube_tracking_face_found = Спрацьовує, коли VTube Studio починає відстежувати обличчя
kind_name_vtube_tracking_face_lost = Обличчя втрачено
kind_summary_vtube_tracking_face_lost = Спрацьовує, коли VTube Studio перестає відстежувати обличчя
kind_name_obs_audio_set_mute = Звук джерела: увімкнути чи вимкнути
kind_summary_obs_audio_set_mute = Вимкнути або увімкнути звук аудіоджерела OBS
kind_name_obs_audio_set_volume = Задати гучність джерела
kind_summary_obs_audio_set_volume = Задати гучність аудіоджерела OBS у дБ
kind_name_obs_audio_source_balance_changed = Баланс аудіоджерела змінено
kind_summary_obs_audio_source_balance_changed = Спрацьовує, коли змінюється баланс звуку джерела OBS
kind_name_obs_audio_source_mute_changed = Стан звуку джерела змінено
kind_summary_obs_audio_source_mute_changed = Спрацьовує, коли звук джерела OBS вимикають або вмикають
kind_name_obs_audio_source_sync_offset_changed = Зсув синхронізації аудіо змінено
kind_summary_obs_audio_source_sync_offset_changed = Спрацьовує, коли змінюється зсув синхронізації звуку джерела OBS
kind_name_obs_audio_source_volume_changed = Гучність аудіоджерела змінено
kind_summary_obs_audio_source_volume_changed = Спрацьовує, коли змінюється рівень гучності джерела OBS
kind_name_obs_browser_refresh = Оновити браузерне джерело
kind_summary_obs_browser_refresh = Оновити браузерне джерело в обхід його кешу
kind_name_obs_capture_screenshot = Зберегти знімок джерела
kind_summary_obs_capture_screenshot = Зберегти знімок джерела OBS у файл; формат визначається розширенням файлу
kind_name_obs_collection_changing = Колекція сцен змінюється
kind_summary_obs_collection_changing = Спрацьовує, коли OBS починає перемикатися на іншу колекцію сцен
kind_name_obs_collection_changed = Колекцію сцен змінено
kind_summary_obs_collection_changed = Спрацьовує, коли OBS завершує перемикання на іншу колекцію сцен
kind_name_obs_collection_list_changed = Список колекцій сцен змінено
kind_summary_obs_collection_list_changed = Спрацьовує, коли в OBS додають або видаляють колекції сцен
kind_name_obs_connection_auth_failed = Помилка автентифікації OBS
kind_summary_obs_connection_auth_failed = Спрацьовує, коли OBS відхиляє підключення WebSocket через невдалу автентифікацію
kind_name_obs_connection_connected = OBS підключено
kind_summary_obs_connection_connected = Спрацьовує, коли підключення WebSocket до OBS успішно встановлено
kind_name_obs_connection_disconnected = OBS відключено
kind_summary_obs_connection_disconnected = Спрацьовує, коли підключення WebSocket до OBS втрачено
kind_name_obs_filter_set_enabled = Фільтр: увімкнути чи вимкнути
kind_summary_obs_filter_set_enabled = Увімкнути або вимкнути фільтр на джерелі OBS
kind_name_obs_filter_created = Фільтр створено
kind_summary_obs_filter_created = Спрацьовує, коли до джерела OBS додають фільтр
kind_name_obs_filter_enabled_changed = Стан фільтра змінено
kind_summary_obs_filter_enabled_changed = Спрацьовує, коли фільтр на джерелі OBS вмикають або вимикають
kind_name_obs_filter_removed = Фільтр видалено
kind_summary_obs_filter_removed = Спрацьовує, коли з джерела OBS видаляють фільтр
kind_name_obs_media_restart = Перезапустити медіаджерело
kind_summary_obs_media_restart = Відтворити медіаджерело OBS спочатку
kind_name_obs_profile_current_changed = Профіль змінено
kind_summary_obs_profile_current_changed = Спрацьовує, коли OBS завершує перемикання на інший профіль
kind_name_obs_profile_list_changed = Список профілів змінено
kind_summary_obs_profile_list_changed = Спрацьовує, коли в OBS додають або видаляють профілі
kind_name_obs_profile_switch = Перемкнути профіль
kind_summary_obs_profile_switch = Перемкнути OBS на інший профіль за назвою
kind_name_obs_recording_file_changed = Файл запису змінено
kind_summary_obs_recording_file_changed = Спрацьовує, коли OBS переходить на новий вихідний файл посеред запису (напр. автоматичне розділення файлу)
kind_name_obs_recording_get_status = Отримати стан запису
kind_summary_obs_recording_get_status = Запитати в OBS поточний стан запису, стан паузи та тривалість
kind_name_obs_recording_pause = Призупинити запис
kind_summary_obs_recording_pause = Призупинити запис OBS
kind_name_obs_recording_paused = Запис призупинено
kind_summary_obs_recording_paused = Спрацьовує, коли активний запис OBS призупиняють
kind_name_obs_recording_resume = Відновити запис
kind_summary_obs_recording_resume = Відновити запис OBS після паузи
kind_name_obs_recording_resumed = Запис відновлено
kind_summary_obs_recording_resumed = Спрацьовує, коли призупинений запис OBS продовжує писатися
kind_name_obs_recording_set_active = Задати стан запису
kind_summary_obs_recording_set_active = Почати або зупинити запис OBS, задавши явний стан
kind_name_obs_recording_set_directory = Задати теку запису
kind_summary_obs_recording_set_directory = Задати теку, куди OBS записує файли
kind_name_obs_recording_start = Почати запис
kind_summary_obs_recording_start = Почати запис OBS
kind_name_obs_recording_started = Запис розпочато
kind_summary_obs_recording_started = Спрацьовує, коли запис OBS стає активним і пишеться на диск
kind_name_obs_recording_starting = Запис запускається
kind_summary_obs_recording_starting = Спрацьовує, коли OBS починає запуск запису (до того, як вихід стане активним)
kind_name_obs_recording_status_changed = Стан запису змінено
kind_summary_obs_recording_status_changed = Спрацьовує на будь-яку зміну стану запису (запуск, розпочато, зупинка, зупинено, призупинено, відновлено)
kind_name_obs_recording_stop = Зупинити запис
kind_summary_obs_recording_stop = Зупинити запис OBS
kind_name_obs_recording_stopped = Запис зупинено
kind_summary_obs_recording_stopped = Спрацьовує, коли запис OBS стає неактивним. Аргумент output_path містить шлях до збереженого файлу
kind_name_obs_recording_stopping = Запис зупиняється
kind_summary_obs_recording_stopping = Спрацьовує, коли OBS починає зупинку запису (до того, як вихід стане неактивним)
kind_name_obs_recording_toggle_pause = Перемкнути паузу запису
kind_summary_obs_recording_toggle_pause = Перемкнути стан паузи запису OBS
kind_name_obs_replay_save = Зберегти буфер повтору
kind_summary_obs_replay_save = Зберегти поточний вміст буфера повтору OBS на диск
kind_name_obs_replay_set_active = Задати стан буфера повтору
kind_summary_obs_replay_set_active = Запустити або зупинити буфер повтору OBS, задавши явний стан
kind_name_obs_replay_start = Запустити буфер повтору
kind_summary_obs_replay_start = Запустити буфер повтору OBS
kind_name_obs_replay_stop = Зупинити буфер повтору
kind_summary_obs_replay_stop = Зупинити буфер повтору OBS
kind_name_obs_collection_switch = Перемкнути колекцію сцен
kind_summary_obs_collection_switch = Перемкнути OBS на іншу колекцію сцен за назвою
kind_name_obs_scene_created = Сцену створено
kind_summary_obs_scene_created = Спрацьовує, коли в OBS створюють нову сцену
kind_name_obs_scene_changed = Сцену змінено
kind_summary_obs_scene_changed = Спрацьовує, коли змінюється поточна сцена програми OBS
kind_name_obs_scene_get_list = Отримати список сцен
kind_summary_obs_scene_get_list = Запитати в OBS усі сцени та поточну сцену програми
kind_name_obs_scene_list_changed = Список сцен змінено
kind_summary_obs_scene_list_changed = Спрацьовує, коли в OBS додають, видаляють або переставляють сцени
kind_name_obs_scene_preview_changed = Сцену попереднього перегляду змінено
kind_summary_obs_scene_preview_changed = Спрацьовує, коли змінюється сцена попереднього перегляду в режимі студії
kind_name_obs_scene_removed = Сцену видалено
kind_summary_obs_scene_removed = Спрацьовує, коли в OBS видаляють сцену
kind_name_obs_scene_renamed = Сцену перейменовано
kind_summary_obs_scene_renamed = Спрацьовує, коли в OBS перейменовують сцену
kind_name_obs_scene_set_preview = Задати сцену попереднього перегляду
kind_summary_obs_scene_set_preview = Задати поточну сцену попереднього перегляду в режимі студії OBS
kind_name_obs_scene_set_transition = Задати перехід між сценами
kind_summary_obs_scene_set_transition = Задати активний перехід між сценами за назвою
kind_name_obs_scene_switch_current = Перемкнути сцену
kind_summary_obs_scene_switch_current = Задати поточну сцену програми OBS
kind_name_obs_source_get_input_settings = Отримати налаштування джерела
kind_summary_obs_source_get_input_settings = Запитати в OBS поточні налаштування джерела за назвою
kind_name_obs_source_get_list = Отримати список джерел
kind_summary_obs_source_get_list = Запитати в OBS усі джерела за назвою
kind_name_obs_source_input_created = Джерело створено
kind_summary_obs_source_input_created = Спрацьовує, коли до OBS додають нове джерело
kind_name_obs_source_input_removed = Джерело видалено
kind_summary_obs_source_input_removed = Спрацьовує, коли з OBS видаляють джерело
kind_name_obs_source_input_renamed = Джерело перейменовано
kind_summary_obs_source_input_renamed = Спрацьовує, коли в OBS перейменовують джерело
kind_name_obs_source_scene_item_created = Елемент сцени додано
kind_summary_obs_source_scene_item_created = Спрацьовує, коли в OBS до сцени додають джерело
kind_name_obs_source_lock_changed = Блокування елемента сцени змінено
kind_summary_obs_source_lock_changed = Спрацьовує, коли в OBS блокують або розблоковують елемент сцени
kind_name_obs_source_scene_item_removed = Елемент сцени видалено
kind_summary_obs_source_scene_item_removed = Спрацьовує, коли в OBS прибирають джерело зі сцени
kind_name_obs_source_visibility_changed = Видимість елемента сцени змінено
kind_summary_obs_source_visibility_changed = Спрацьовує, коли в OBS показують або приховують елемент сцени
kind_name_obs_source_set_input_settings = Задати налаштування джерела
kind_summary_obs_source_set_input_settings = Застосувати об'єкт налаштувань JSON до джерела OBS
kind_name_obs_source_set_locked = Джерело: заблокувати чи розблокувати
kind_summary_obs_source_set_locked = Заблокувати або розблокувати джерело в сцені OBS
kind_name_obs_source_set_visible = Джерело: показати чи сховати
kind_summary_obs_source_set_visible = Показати або приховати джерело в сцені OBS
kind_name_obs_streaming_get_status = Отримати стан трансляції
kind_summary_obs_streaming_get_status = Запитати в OBS поточний стан трансляції та її тривалість
kind_name_obs_streaming_send_caption = Надіслати субтитри трансляції
kind_summary_obs_streaming_send_caption = Надіслати текст субтитрів CEA-608 через вихід трансляції OBS
kind_name_obs_streaming_set_active = Задати стан трансляції
kind_summary_obs_streaming_set_active = Почати або зупинити трансляцію OBS, задавши явний стан
kind_name_obs_streaming_start = Почати трансляцію
kind_summary_obs_streaming_start = Почати трансляцію OBS
kind_name_obs_streaming_started = Трансляцію розпочато
kind_summary_obs_streaming_started = Спрацьовує, коли трансляція OBS стає активною і дані передаються
kind_name_obs_streaming_starting = Трансляція запускається
kind_summary_obs_streaming_starting = Спрацьовує, коли OBS починає запуск трансляції (до того, як вихід стане активним)
kind_name_obs_streaming_status_changed = Стан трансляції змінено
kind_summary_obs_streaming_status_changed = Спрацьовує на будь-яку зміну стану трансляції (запуск, розпочато, зупинка, зупинено, перепідключення, перепідключено)
kind_name_obs_streaming_stop = Зупинити трансляцію
kind_summary_obs_streaming_stop = Зупинити трансляцію OBS
kind_name_obs_streaming_stopped = Трансляцію зупинено
kind_summary_obs_streaming_stopped = Спрацьовує, коли трансляція OBS повністю зупинилася
kind_name_obs_streaming_stopping = Трансляція зупиняється
kind_summary_obs_streaming_stopping = Спрацьовує, коли OBS починає зупинку трансляції (до того, як вихід стане неактивним)
kind_name_obs_studio_disable = Вимкнути режим студії
kind_summary_obs_studio_disable = Вимкнути режим студії OBS
kind_name_obs_studio_disabled = Режим студії вимкнено
kind_summary_obs_studio_disabled = Спрацьовує, коли режим студії OBS вимикається
kind_name_obs_studio_enable = Увімкнути режим студії
kind_summary_obs_studio_enable = Увімкнути режим студії OBS
kind_name_obs_studio_enabled = Режим студії увімкнено
kind_summary_obs_studio_enabled = Спрацьовує, коли режим студії OBS вмикається
kind_name_obs_studio_set_enabled = Задати режим студії
kind_summary_obs_studio_set_enabled = Явно увімкнути або вимкнути режим студії OBS
kind_name_obs_studio_trigger_transition = Запустити перехід студії
kind_summary_obs_studio_trigger_transition = Запустити поточний перехід у режимі студії OBS, переносячи попередній перегляд у програму
kind_name_obs_transition_ended = Перехід між сценами завершено
kind_summary_obs_transition_ended = Спрацьовує, коли в OBS завершується перехід між сценами (точка зрізу; відео ще може відтворюватися)
kind_name_obs_transition_started = Перехід між сценами розпочато
kind_summary_obs_transition_started = Спрацьовує, коли в OBS починається перехід між сценами
kind_name_obs_transition_video_ended = Відео переходу завершено
kind_summary_obs_transition_video_ended = Спрацьовує, коли відео переходу повністю відтворено (кінець відео стінгера, а не лише точка зрізу)
kind_name_obs_virtualcam_set_active = Задати стан віртуальної камери
kind_summary_obs_virtualcam_set_active = Запустити або зупинити віртуальну камеру OBS, задавши явний стан
kind_name_obs_virtualcam_started = Віртуальну камеру запущено
kind_summary_obs_virtualcam_started = Спрацьовує, коли віртуальна камера OBS стає активною
kind_name_obs_virtualcam_state_changed = Стан віртуальної камери змінено
kind_summary_obs_virtualcam_state_changed = Спрацьовує на будь-яку зміну стану віртуальної камери (запуск, запущено, зупинка, зупинено)
kind_name_obs_virtualcam_stopped = Віртуальну камеру зупинено
kind_summary_obs_virtualcam_stopped = Спрацьовує, коли віртуальна камера OBS повністю зупинилася
