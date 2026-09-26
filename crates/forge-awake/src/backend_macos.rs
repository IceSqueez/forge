#![allow(unsafe_code)]

use async_trait::async_trait;
use objc2_core_foundation::CFString;

use crate::backend::AwakeBackend;
use crate::{Aspect, AwakeError};

type IoReturn = i32;
type AssertionId = u32;

const IO_RETURN_SUCCESS: IoReturn = 0;
const ASSERTION_LEVEL_ON: u32 = 255;
const PREVENT_DISPLAY_SLEEP: &str = "PreventUserIdleDisplaySleep";
const PREVENT_SYSTEM_SLEEP: &str = "PreventUserIdleSystemSleep";
const POWER_SERVICE: &str = "power manager";

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOPMAssertionCreateWithName(
        assertion_type: &CFString,
        level: u32,
        name: &CFString,
        assertion_id: *mut AssertionId,
    ) -> IoReturn;
    fn IOPMAssertionRelease(assertion_id: AssertionId) -> IoReturn;
}

pub(crate) struct MacBackend {
    reason: String,
    display: Option<AssertionId>,
    system: Option<AssertionId>,
}

impl MacBackend {
    pub(crate) fn new(reason: String) -> Self {
        Self {
            reason,
            display: None,
            system: None,
        }
    }

    fn slot(&mut self, aspect: Aspect) -> &mut Option<AssertionId> {
        match aspect {
            Aspect::Display => &mut self.display,
            Aspect::System => &mut self.system,
        }
    }
}

#[async_trait]
impl AwakeBackend for MacBackend {
    async fn acquire(&mut self, aspect: Aspect) -> Result<(), AwakeError> {
        self.release(aspect).await;
        let assertion_type = CFString::from_str(match aspect {
            Aspect::Display => PREVENT_DISPLAY_SLEEP,
            Aspect::System => PREVENT_SYSTEM_SLEEP,
        });
        let name = CFString::from_str(&self.reason);
        let mut assertion_id: AssertionId = 0;
        // SAFETY: both CFStrings are retained for the whole call and `assertion_id` is a valid, writable out-pointer.
        let status = unsafe {
            IOPMAssertionCreateWithName(
                &assertion_type,
                ASSERTION_LEVEL_ON,
                &name,
                &mut assertion_id,
            )
        };
        if status != IO_RETURN_SUCCESS {
            return Err(AwakeError::Refused {
                service: POWER_SERVICE,
                reason: format!("IOKit error {status:#x}"),
            });
        }
        *self.slot(aspect) = Some(assertion_id);
        Ok(())
    }

    async fn release(&mut self, aspect: Aspect) {
        if let Some(assertion_id) = self.slot(aspect).take() {
            // SAFETY: `assertion_id` was returned by a successful create and is released exactly once.
            let status = unsafe { IOPMAssertionRelease(assertion_id) };
            if status != IO_RETURN_SUCCESS {
                tracing::debug!(status, "stay awake: power assertion release failed");
            }
        }
    }

    fn is_held(&self, aspect: Aspect) -> bool {
        match aspect {
            Aspect::Display => self.display.is_some(),
            Aspect::System => self.system.is_some(),
        }
    }
}

impl Drop for MacBackend {
    fn drop(&mut self) {
        for assertion_id in [self.display.take(), self.system.take()]
            .into_iter()
            .flatten()
        {
            // SAFETY: `assertion_id` was returned by a successful create and has not been released.
            unsafe { IOPMAssertionRelease(assertion_id) };
        }
    }
}
