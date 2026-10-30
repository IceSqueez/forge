msg-obs-connect_refused = OBS не приймає підключення. Запустіть OBS і ввімкніть його WebSocket-сервер.
msg-obs-unreachable = Не вдалося зв'язатися з OBS. Перевірте адресу та мережу.
msg-obs-password_rejected = OBS відхилив пароль. Введіть пароль WebSocket ще раз.
msg-obs-connect_failed = Не вдалося підключитися до OBS.
msg-obs-connection_lost = З'єднання з OBS втрачено.
msg-obs-unresponsive = OBS перестав відповідати на запити.
msg-obs-health_stream = Стрім
msg-obs-health_recording = Запис
msg-obs-health_cpu_fps = ЦП · FPS
msg-obs-health_dropped_frames = Пропущено
msg-obs-not_connected = Немає підключення до OBS.
msg-obs-request_timed_out = OBS не відповів вчасно.
msg-obs-request_rejected = OBS відхилив запит { $request }.
msg-obs-protocol_error = OBS надіслав відповідь, яку forge не зміг розібрати.
msg-obs-payload_invalid = Не вдалося закодувати дані запиту для OBS.
msg-obs-studio_mode_inactive = Режим студії вимкнено в OBS. Спершу ввімкніть його підзадачею "Set Studio Mode".
msg-obs-health_live = Наживо
msg-obs-health_off = Вимкнено
msg-obs-health_paused = Пауза
msg-obs-health_active = Запис іде
msg-obs-health_lagging = затримки
msg-obs-health_smooth = плавно
msg-obs-health_cpu_fps_value = { NUMBER($cpu, minimumFractionDigits: 1, maximumFractionDigits: 1) }% · { NUMBER($fps, minimumFractionDigits: 1, maximumFractionDigits: 1) }
msg-obs-health_dropped_count = { $count ->
    [one] { $count } кадр
    [few] { $count } кадри
   *[other] { $count } кадрів
}
msg-obs-health_dropped_share = { NUMBER($percent, minimumFractionDigits: 2, maximumFractionDigits: 2) }%
