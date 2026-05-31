use forge_registry::{RegistryError, TriggerRegistry};

use crate::triggers::chat::ChatDescriptor;
use crate::triggers::follow::FollowDescriptor;
use crate::triggers::gift_sub::GiftSubDescriptor;
use crate::triggers::spell::SpellDescriptor;
use crate::triggers::subscription::SubscriptionDescriptor;

pub fn register_trovo_triggers(registry: &mut TriggerRegistry) -> Result<(), RegistryError> {
    registry.register(Box::new(ChatDescriptor))?;
    registry.register(Box::new(SpellDescriptor))?;
    registry.register(Box::new(SubscriptionDescriptor))?;
    registry.register(Box::new(FollowDescriptor))?;
    registry.register(Box::new(GiftSubDescriptor))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn register_adds_all_five_descriptors() {
        let mut reg = TriggerRegistry::new();
        register_trovo_triggers(&mut reg).unwrap();
        assert_eq!(reg.all().count(), 5);
    }

    #[test]
    fn duplicate_registration_returns_error() {
        let mut reg = TriggerRegistry::new();
        register_trovo_triggers(&mut reg).unwrap();
        let result = register_trovo_triggers(&mut reg);
        assert!(result.is_err());
    }

    #[test]
    fn all_kind_ids_are_reachable() {
        let mut reg = TriggerRegistry::new();
        register_trovo_triggers(&mut reg).unwrap();

        let ids = [
            "trovo.chat",
            "trovo.spell",
            "trovo.subscription",
            "trovo.follow",
            "trovo.gift_sub",
        ];

        for id in ids {
            assert!(reg.get(id).is_some(), "missing kind id: {id}");
        }
    }

    #[test]
    fn all_descriptors_are_platform_specific_trovo() {
        use forge_registry::KindPlatformContract;
        use forge_types::PlatformId;

        let mut reg = TriggerRegistry::new();
        register_trovo_triggers(&mut reg).unwrap();

        for descriptor in reg.all() {
            assert_eq!(
                descriptor.platform_contract(),
                KindPlatformContract::PlatformSpecific(PlatformId::Trovo),
                "descriptor '{}' must be platform-specific Trovo",
                descriptor.id()
            );
        }
    }
}
