#![allow(unsafe_code)]

use std::ffi::c_void;

use async_trait::async_trait;
use windows::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE};
use windows::Win32::System::Power::{
    DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, HPOWERNOTIFY, POWER_REQUEST_TYPE, PowerClearRequest,
    PowerCreateRequest, PowerRegisterSuspendResumeNotification, PowerRequestDisplayRequired,
    PowerRequestSystemRequired, PowerSetRequest, PowerUnregisterSuspendResumeNotification,
};
use windows::Win32::System::Threading::{
    POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0,
};
use windows::Win32::UI::WindowsAndMessaging::{DEVICE_NOTIFY_CALLBACK, PBT_APMRESUMEAUTOMATIC};
use windows::core::PWSTR;

use crate::backend::{AwakeBackend, DisruptionSender};
use crate::{Aspect, AwakeError};

const POWER_REQUEST_CONTEXT_VERSION: u32 = 0;
const POWER_SERVICE: &str = "power manager";

const DISPLAY_REQUESTS: [POWER_REQUEST_TYPE; 2] =
    [PowerRequestDisplayRequired, PowerRequestSystemRequired];
const SYSTEM_REQUESTS: [POWER_REQUEST_TYPE; 1] = [PowerRequestSystemRequired];

struct PowerRequest {
    handle: HANDLE,
    kinds: &'static [POWER_REQUEST_TYPE],
}

// SAFETY: a power request handle is a process-wide kernel object, valid from any thread.
unsafe impl Send for PowerRequest {}

impl PowerRequest {
    fn create(reason: &str, kinds: &'static [POWER_REQUEST_TYPE]) -> Result<Self, AwakeError> {
        let mut wide: Vec<u16> = reason.encode_utf16().chain(std::iter::once(0)).collect();
        let context = REASON_CONTEXT {
            Version: POWER_REQUEST_CONTEXT_VERSION,
            Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
            Reason: REASON_CONTEXT_0 {
                SimpleReasonString: PWSTR(wide.as_mut_ptr()),
            },
        };
        // SAFETY: `context` and the NUL-terminated `wide` buffer outlive the call; the kernel copies the string.
        let handle = unsafe { PowerCreateRequest(&context) }.map_err(refused)?;
        let request = Self { handle, kinds };
        for kind in kinds {
            // SAFETY: `request.handle` is a live power request owned by `request`.
            unsafe { PowerSetRequest(request.handle, *kind) }.map_err(refused)?;
        }
        Ok(request)
    }
}

impl Drop for PowerRequest {
    fn drop(&mut self) {
        for kind in self.kinds {
            // SAFETY: `self.handle` is a live power request owned by `self`.
            let _ = unsafe { PowerClearRequest(self.handle, *kind) };
        }
        // SAFETY: `self.handle` is owned by `self` and closed exactly once, here.
        let _ = unsafe { CloseHandle(self.handle) };
    }
}

struct ResumeWatch {
    registration: HPOWERNOTIFY,
    _params: Box<DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS>,
    _sink: Box<DisruptionSender>,
}

// SAFETY: the boxed parameters are only read by the OS callback, and the sender they point at is Send + Sync.
unsafe impl Send for ResumeWatch {}

impl ResumeWatch {
    fn register(disruptions: DisruptionSender) -> Option<Self> {
        let sink = Box::new(disruptions);
        let mut params = Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
            Callback: Some(on_power_event),
            Context: std::ptr::from_ref::<DisruptionSender>(&sink)
                .cast_mut()
                .cast(),
        });
        let mut registration: *mut c_void = std::ptr::null_mut();
        // SAFETY: `params` and the sender it points at are heap-owned by the returned watch, which unregisters before freeing them.
        let status = unsafe {
            PowerRegisterSuspendResumeNotification(
                DEVICE_NOTIFY_CALLBACK,
                HANDLE(std::ptr::from_mut(params.as_mut()).cast()),
                &mut registration,
            )
        };
        if status != ERROR_SUCCESS {
            tracing::warn!(
                code = status.0,
                "stay awake: resume notifications unavailable; holds are not renewed after sleep"
            );
            return None;
        }
        Some(Self {
            registration: HPOWERNOTIFY(registration as isize),
            _params: params,
            _sink: sink,
        })
    }
}

impl Drop for ResumeWatch {
    fn drop(&mut self) {
        // SAFETY: `self.registration` came from a successful registration and is unregistered exactly once.
        let _ = unsafe { PowerUnregisterSuspendResumeNotification(self.registration) };
    }
}

unsafe extern "system" fn on_power_event(
    context: *const c_void,
    event: u32,
    _setting: *const c_void,
) -> u32 {
    if event == PBT_APMRESUMEAUTOMATIC {
        // SAFETY: `context` points at the sender owned by the live `ResumeWatch` that registered this callback.
        let sink = unsafe { &*context.cast::<DisruptionSender>() };
        for aspect in Aspect::ALL {
            let _ = sink.send(aspect);
        }
    }
    ERROR_SUCCESS.0
}

pub(crate) struct WindowsBackend {
    reason: String,
    display: Option<PowerRequest>,
    system: Option<PowerRequest>,
    _resume: Option<ResumeWatch>,
}

impl WindowsBackend {
    pub(crate) fn new(reason: String, disruptions: DisruptionSender) -> Self {
        Self {
            reason,
            display: None,
            system: None,
            _resume: ResumeWatch::register(disruptions),
        }
    }
}

#[async_trait]
impl AwakeBackend for WindowsBackend {
    async fn acquire(&mut self, aspect: Aspect) -> Result<(), AwakeError> {
        self.release(aspect).await;
        match aspect {
            Aspect::Display => {
                self.display = Some(PowerRequest::create(&self.reason, &DISPLAY_REQUESTS)?);
            }
            Aspect::System => {
                self.system = Some(PowerRequest::create(&self.reason, &SYSTEM_REQUESTS)?);
            }
        }
        Ok(())
    }

    async fn release(&mut self, aspect: Aspect) {
        match aspect {
            Aspect::Display => self.display = None,
            Aspect::System => self.system = None,
        }
    }

    fn is_held(&self, aspect: Aspect) -> bool {
        match aspect {
            Aspect::Display => self.display.is_some(),
            Aspect::System => self.system.is_some(),
        }
    }
}

fn refused(err: windows::core::Error) -> AwakeError {
    AwakeError::Refused {
        service: POWER_SERVICE,
        reason: err.message(),
    }
}
