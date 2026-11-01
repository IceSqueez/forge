mod form;
mod page;

pub(crate) use form::{
    AttemptStatus, BOX_PAD_H, ConnectFlag, ConnectFlagSpec, ConnectFlags, EndpointFields,
    FIELD_BLOCK_GAP, FormButtons, StripContent, StripLook, box_frame, endpoint_host_is_valid,
    endpoint_input, endpoint_row, field_label, flag_rows, form_buttons, input_box, mono_box_text,
    outcome_strip, persist_flag, read_endpoint, strip_frame, strip_icon,
};
pub(crate) use page::{ConnectPage, GuideStep, fragment, mono_fragment};
