use forge_types::SubActionOutcome;

use crate::error::SoundboardError;
use crate::vocabulary::messages;

pub(crate) fn outcome(result: &Result<(), SoundboardError>) -> SubActionOutcome {
    match result {
        Ok(()) => SubActionOutcome::Success,
        Err(error) => SubActionOutcome::Failed(messages::failure(error).into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use forge_types::UserText;

    #[test]
    fn a_successful_control_call_is_a_success() {
        assert!(matches!(outcome(&Ok(())), SubActionOutcome::Success));
    }

    #[test]
    fn a_failed_control_call_carries_the_mapped_message_not_the_error_text() {
        let error = SoundboardError::ClipNotFound("kick".to_owned());

        assert_eq!(
            outcome(&Err(error)),
            SubActionOutcome::Failed(UserText::Message(messages::failure(
                &SoundboardError::ClipNotFound("kick".to_owned())
            )))
        );
    }
}
