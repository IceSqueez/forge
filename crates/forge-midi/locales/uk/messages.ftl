msg-midi-backend_unavailable = Системна служба MIDI недоступна
msg-midi-health_input_ports = Вхідні порти
msg-midi-health_output_ports = Вихідні порти
msg-midi-health_last_note_on = Остання нота
msg-midi-health_events_per_minute = Подій / хв
msg-midi-raw_bytes_shape = Має бути списком значень байтів
msg-midi-port_not_found = MIDI-порт "{ $port }" не знайдено.
msg-midi-port_disconnected = MIDI-порт "{ $port }" відключено.
msg-midi-invalid_status_byte = MIDI-повідомлення має недійсний статусний байт ({ $byte }).
msg-midi-port_connect_failed = Не вдалося підключитися до MIDI-порту.
msg-midi-send_failed = Не вдалося надіслати MIDI-повідомлення.
msg-midi-supervisor_unavailable = Служба MIDI не запущена.
msg-midi-health_input_connected = підключено
msg-midi-health_disabled = вимкнено
msg-midi-health_output_available = доступно
msg-midi-health_note_time = { $time }
msg-midi-health_last_minute = за останню хвилину
msg-midi-condition_any_note = будь-яка нота
msg-midi-condition_any_controller = будь-який контролер
msg-midi-condition_any_program = будь-яка програма
msg-midi-condition_any_channel = будь-який канал
msg-midi-condition_any_direction = будь-який напрямок
msg-midi-condition_device = пристрій={ $device }
msg-midi-condition_direction = напрямок={ $direction ->
    [input] вхід
    [output] вихід
   *[other] { $direction }
}
msg-midi-condition_note = нота={ $number }
msg-midi-condition_controller = cc={ $number }
msg-midi-condition_program = програма={ $number }
msg-midi-condition_channel = канал={ $channel }
