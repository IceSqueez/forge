UPDATE action_history
SET context = json_set(
    context,
    '$.telemetry',
    (
        SELECT json_group_array(
            CASE
                WHEN json_type(step.value, '$.outcome.Failed') = 'text' THEN json_set(
                    step.value,
                    '$.outcome',
                    json_object(
                        'Failed',
                        json_object('verbatim', json_extract(step.value, '$.outcome.Failed'))
                    )
                )
                WHEN json_type(step.value, '$.outcome.Skipped') = 'text' THEN json_set(
                    step.value,
                    '$.outcome',
                    json_object(
                        'Skipped',
                        json_object('verbatim', json_extract(step.value, '$.outcome.Skipped'))
                    )
                )
                ELSE json(step.value)
            END
            ORDER BY step.key
        )
        FROM json_each(action_history.context, '$.telemetry') AS step
    )
)
WHERE CASE
    WHEN NOT json_valid(context) THEN 0
    WHEN json_type(context, '$.telemetry') IS NOT 'array' THEN 0
    ELSE EXISTS (
        SELECT 1
        FROM json_each(action_history.context, '$.telemetry') AS step
        WHERE json_type(step.value, '$.outcome.Failed') = 'text'
           OR json_type(step.value, '$.outcome.Skipped') = 'text'
    )
END;

UPDATE action_history
SET context = json_set(
    context,
    '$.outcome',
    json_object('Failed', json_object('verbatim', json_extract(context, '$.outcome.Failed')))
)
WHERE CASE
    WHEN json_valid(context) THEN json_type(context, '$.outcome.Failed') = 'text'
    ELSE 0
END;

UPDATE action_history
SET outcome = json_object('Failed', json_object('verbatim', json_extract(outcome, '$.Failed')))
WHERE CASE
    WHEN json_valid(outcome) THEN json_type(outcome, '$.Failed') = 'text'
    ELSE 0
END;
