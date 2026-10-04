use super::*;
use forge_storage::ScheduledRunRepo;

impl ScreenActionsView {
    pub fn with_scheduled_runs(mut self, repo: Arc<dyn ScheduledRunRepo>) -> Self {
        self.scheduled_runs = Some(repo);
        self
    }

    pub(super) fn load_delete_scheduled_count(&mut self, id: ActionId, cx: &mut Context<Self>) {
        self.delete_scheduled_count = None;
        let Some(repo) = self.scheduled_runs.as_ref().map(Arc::clone) else {
            return;
        };
        async_bridge::run_async(
            &self.rt_handle,
            async move { repo.count_pending_for_action(id).await },
            move |this, result, cx| {
                if let Ok(count) = result {
                    this.delete_scheduled_count = Some((id, count));
                    cx.notify();
                }
            },
            cx,
        );
    }

    pub(super) fn delete_scheduled_note(&self, id: ActionId) -> Option<String> {
        match self.delete_scheduled_count {
            Some((counted, count)) if counted == id && count > 0 => Some(tr!(
                "actions_delete_scheduled_runs",
                count = i64::try_from(count).unwrap_or(i64::MAX)
            )),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_storage::Language;

    use crate::i18n::install_language;

    fn scheduled_runs_sentence(language: Language, count: i64) -> String {
        install_language(language);
        forge_components::tr!("actions_delete_scheduled_runs", count = count)
            .chars()
            .filter(|c| !matches!(c, '\u{2068}' | '\u{2069}'))
            .collect()
    }

    #[test]
    fn the_scheduled_runs_sentence_picks_the_plural_form_for_the_count() {
        let one = "Також буде скасовано {n} її запланований запуск.";
        let few = "Також буде скасовано {n} її заплановані запуски.";
        let many = "Також буде скасовано {n} її запланованих запусків.";
        for (language, count, form) in [
            (Language::Uk, 1, one),
            (Language::Uk, 21, one),
            (Language::Uk, 2, few),
            (Language::Uk, 4, few),
            (Language::Uk, 22, few),
            (Language::Uk, 5, many),
            (Language::Uk, 11, many),
            (Language::Uk, 12, many),
            (Language::Uk, 14, many),
            (Language::Uk, 25, many),
            (
                Language::En,
                1,
                "Its {n} pending scheduled run will be cancelled too.",
            ),
            (
                Language::En,
                2,
                "Its {n} pending scheduled runs will be cancelled too.",
            ),
        ] {
            assert_eq!(
                scheduled_runs_sentence(language, count),
                form.replace("{n}", &count.to_string()),
                "{language:?} {count}"
            );
        }
    }
}
