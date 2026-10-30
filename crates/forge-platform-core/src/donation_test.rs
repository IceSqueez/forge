use std::collections::BTreeMap;

use forge_types::{IntegrationId, MessageCatalog, MessageCode, SubActionStep, Variant};

use crate::builtin::{
    QuickAction, QuickActionAccent, QuickActionField, QuickActionFieldKind, QuickActionFieldValue,
    QuickActionLiveness, SectionIcon,
};

pub const TEST_DONATION_SUB_ACTION: &str = "donations.test_donation";

pub const TEST_DONATION_PROVIDER_KEY: &str = "provider";
pub const TEST_DONATION_DONOR_KEY: &str = "donor_name";
pub const TEST_DONATION_AMOUNT_KEY: &str = "amount";
pub const TEST_DONATION_CURRENCY_KEY: &str = "currency";
pub const TEST_DONATION_MESSAGE_KEY: &str = "message";

pub const TEST_DONATION_DONOR: &str = "Test donor";
pub const TEST_DONATION_AMOUNT: &str = "50";
pub const TEST_DONATION_CURRENCY: &str = "UAH";
pub const TEST_DONATION_MESSAGE: &str = "This is a test donation";

const DONATION_NAMESPACE: &str = "donations";

const SEND_TEST_DONATION_LABEL: MessageCode =
    MessageCode::from_static(DONATION_NAMESPACE, "send_test_donation");
const DONOR_FIELD_LABEL: MessageCode =
    MessageCode::from_static(DONATION_NAMESPACE, "field_donor_name");
const AMOUNT_FIELD_LABEL: MessageCode =
    MessageCode::from_static(DONATION_NAMESPACE, "field_amount");
const CURRENCY_FIELD_LABEL: MessageCode =
    MessageCode::from_static(DONATION_NAMESPACE, "field_currency");
const MESSAGE_FIELD_LABEL: MessageCode =
    MessageCode::from_static(DONATION_NAMESPACE, "field_message");

pub const TEST_DONATION_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: DONATION_NAMESPACE,
    codes: &[
        SEND_TEST_DONATION_LABEL,
        DONOR_FIELD_LABEL,
        AMOUNT_FIELD_LABEL,
        CURRENCY_FIELD_LABEL,
        MESSAGE_FIELD_LABEL,
    ],
    en_ftl: include_str!("../locales/en/messages.ftl"),
    uk_ftl: include_str!("../locales/uk/messages.ftl"),
};

pub fn test_donation_step(provider: &IntegrationId) -> SubActionStep {
    let text = |value: &str| Variant::String(value.to_owned());
    SubActionStep {
        kind_id: TEST_DONATION_SUB_ACTION.to_owned(),
        config: BTreeMap::from([
            (
                TEST_DONATION_PROVIDER_KEY.to_owned(),
                text(provider.as_str()),
            ),
            (
                TEST_DONATION_DONOR_KEY.to_owned(),
                text(TEST_DONATION_DONOR),
            ),
            (
                TEST_DONATION_AMOUNT_KEY.to_owned(),
                text(TEST_DONATION_AMOUNT),
            ),
            (
                TEST_DONATION_CURRENCY_KEY.to_owned(),
                text(TEST_DONATION_CURRENCY),
            ),
            (
                TEST_DONATION_MESSAGE_KEY.to_owned(),
                text(TEST_DONATION_MESSAGE),
            ),
        ]),
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

pub fn test_donation_quick_action(provider: &IntegrationId) -> QuickAction {
    let field = |key: &str, label: MessageCode, kind: QuickActionFieldKind, default: &str| {
        QuickActionField {
            key: key.to_owned(),
            label: label.to_string(),
            kind,
            default: Some(QuickActionFieldValue::Text(default.to_owned())),
            placeholder: None,
            hint: None,
            required: false,
        }
    };
    QuickAction {
        label: SEND_TEST_DONATION_LABEL.to_string(),
        icon: SectionIcon::new("coin"),
        enabled: true,
        locked_reason: None,
        liveness: QuickActionLiveness::Unknown,
        group: None,
        group_icon: None,
        group_accent: None,
        destructive: false,
        accent: QuickActionAccent::Bits,
        subaction_template: test_donation_step(provider),
        fields: vec![
            field(
                TEST_DONATION_DONOR_KEY,
                DONOR_FIELD_LABEL,
                QuickActionFieldKind::Text,
                TEST_DONATION_DONOR,
            ),
            field(
                TEST_DONATION_AMOUNT_KEY,
                AMOUNT_FIELD_LABEL,
                QuickActionFieldKind::Text,
                TEST_DONATION_AMOUNT,
            )
            .required(),
            field(
                TEST_DONATION_CURRENCY_KEY,
                CURRENCY_FIELD_LABEL,
                QuickActionFieldKind::Text,
                TEST_DONATION_CURRENCY,
            )
            .required(),
            field(
                TEST_DONATION_MESSAGE_KEY,
                MESSAGE_FIELD_LABEL,
                QuickActionFieldKind::Multiline,
                TEST_DONATION_MESSAGE,
            ),
        ],
        collection: None,
    }
}
