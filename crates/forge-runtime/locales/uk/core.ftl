msg-core-unknown_action = Дії { $action } не існує
msg-core-unknown_trigger_instance = Тригера { $instance } не існує
msg-core-action_nesting_too_deep = Дії вкладені одна в одну занадто глибоко
msg-core-action_run_cancelled = Вкладену дію скасовано
msg-core-action_run_failed = Не вдалося запустити вкладену дію
msg-core-scheduler_not_ready = Планувальник черг ще не запущено
msg-core-scheduler_stopped = Планувальник відкладених запусків зупинено
msg-core-scheduler_failed = Планувальник черг відхилив запит
msg-core-schedule_failed = Не вдалося запланувати запуск
msg-core-storage_failed = Не вдалося прочитати або зберегти дані
msg-core-system_service_failed = Система не змогла виконати запит
msg-core-task_failed = Крок зупинився, не завершившись
msg-core-http_failed = Запит не вдався
msg-core-condition_failed = Не вдалося обчислити умову
msg-core-field_required = Поле { $field } обов'язкове
msg-core-file_path_empty = Шлях до файлу порожній
msg-core-file_path_absolute = Абсолютні шляхи заборонені. Покладіть файл у теку файлів і вкажіть шлях відносно неї
msg-core-file_path_parent = ".." заборонено. Шлях має залишатися всередині теки файлів
msg-core-file_path_escapes = Шлях проходить через посилання, яке зламане або веде за межі теки файлів
msg-core-file_not_found = Файл не знайдено в теці файлів
msg-core-directory_not_found = Теку не знайдено в теці файлів
msg-core-directory_too_large = Тека містить більше ніж { $limit } { $limit ->
    [one] запис
    [few] записи
   *[other] записів
}
msg-core-nested_steps_unavailable = Вкладені кроки не можуть виконуватися у швидкій дії
msg-core-action_disabled = Дію вимкнено
msg-core-path_not_directory = Шлях не веде до теки
msg-core-path_not_file = Шлях не веде до файлу
msg-core-file_too_large = Файл більший за { $limit } { $limit ->
    [one] байт
    [few] байти
   *[other] байтів
}
msg-core-file_io_failed = Не вдалося отримати доступ до файлу
msg-core-file_not_json = Файл не є коректним JSON
msg-core-file_not_base64 = Вміст не є коректним base64
msg-core-file_char_not_latin1 = Символ { $char } не можна записати в кодуванні Latin-1
msg-core-global_missing = Глобальної змінної { $name } не існує
msg-core-global_not_array = Очікувався масив, але глобальна змінна містить { $actual }
msg-core-global_not_bool = Очікувалося логічне значення, але глобальна змінна містить { $actual }
msg-core-amount_not_negatable = Значення { $amount } не можна змінити на протилежне
msg-core-user_variable_not_numeric = Збережена змінна користувача не є числом
msg-core-user_login_unset = Змінну логіна користувача %{ $variable }% не задано в цьому запуску
msg-core-user_login_empty = Логін користувача порожній
msg-core-loop_iteration_limit = Цикл foreach зупинено на ліміті ітерацій: { $limit }. Елементів у масиві: { $items }
msg-core-loop_iterations_unrecorded = Кроки ітерацій від { $from } до { $to } не записано
msg-core-substring_start_out_of_range = Початковий індекс { $start } виходить за межі тексту (довжина: { $length })
msg-core-substring_end_out_of_range = Кінцевий індекс { $end } виходить за межі тексту (довжина: { $length })
msg-core-substring_start_after_end = Початковий індекс { $start } більший за кінцевий { $end }
msg-core-time_out_of_range = Зсув дати { $base } виходить за підтримуваний діапазон (кількість: { $amount }, одиниця: { $unit ->
    [seconds] секунди
    [minutes] хвилини
    [hours] години
    [days] дні
    [months] місяці
    [years] роки
   *[other] { $unit }
})
msg-core-time_pattern_invalid = Шаблон дати { $pattern } некоректний
msg-core-time_text_unreadable = Текст { $text } не відповідає шаблону дати { $pattern }
msg-core-search_pattern_invalid = Текст пошуку не є коректним регулярним виразом
msg-core-expression_undefined_variables = Вираз використовує змінні, які не задано: { $variables }
msg-core-expression_failed = Не вдалося обчислити вираз
msg-core-url_not_http = Можна відкривати лише адреси http і https
msg-core-overlay_service_not_running = Служба оверлеїв не запущена
msg-core-overlay_show_cleared = Чергу показів оверлея очищено до виконання цього показу
msg-core-overlay_show_withdrawn = Оверлей вимкнено або видалено до виконання показу
msg-core-overlay_send_failed = Оверлей відхилив повідомлення
msg-core-script_not_found = Скрипт { $name } не знайдено
msg-core-script_failed = Скрипт { $name } завершився помилкою
msg-core-inline_script_failed = Вбудований скрипт завершився помилкою
msg-core-script_panicked = Скрипт несподівано зупинився
msg-core-test_donation_not_announced = Не вдалося озвучити тестовий донат
msg-core-voice_not_engine_voice = { $voice } не є голосом рушія. Оберіть голос ще раз
msg-core-voice_not_installed = Голос { $voice } не встановлено. Оберіть голос ще раз
msg-core-run_depth_exceeded = Не запущено: подія-тригер глибша за { $limit } запусків дій (цикл між діями?)
msg-core-action_stopped = Зупинено
msg-core-unknown_step_kind = Невідомий тип кроку: { $kind }
msg-core-step_disabled = Крок вимкнено
msg-core-condition_not_met = Умову не виконано
msg-core-regex_invalid = Некоректний регулярний вираз
msg-core-time_pattern_legacy = Застаріла частина шаблону { $component } не має відповідника в шаблонах дати. Перепишіть шаблон літерами, наприклад HH:mm dd.MM.yyyy
msg-core-entry_without_value = { $name } не має значення
msg-core-random_max_below_min = Не може бути меншим за мінімум
msg-core-random_range_too_wide = Діапазон завеликий для вибірки
msg-core-weight_not_a_number = Вага { $position } не є числом
msg-core-weight_count_mismatch = Вказано ваг: { $weights }, а елементів: { $items }
msg-core-weights_unusable = Ці ваги неможливо використати
msg-core-output_not_convertible = Це значення неможливо зберегти як змінну
msg-core-currency_invalid = Має бути трилітерним кодом валюти
msg-core-amount_invalid = Має бути невід'ємною сумою
msg-core-condition_any_donation_service = будь-який сервіс донатів
msg-core-condition_any_event = будь-яка подія
msg-core-condition_timer_live_only = лише під час ефіру
msg-core-condition_donation_service = сервіс = { $service }
msg-core-condition_event = подія = { $event }
msg-core-condition_timer_every = кожні { $minutes } хв
msg-core-condition_timer_min_messages = ≥{ $count } { $count ->
    [one] повідомлення
    [few] повідомлення
    [many] повідомлень
   *[other] повідомлення
}
