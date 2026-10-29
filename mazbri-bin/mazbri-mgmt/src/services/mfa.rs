mod common;
mod enable_flow;
mod required_flow;
pub(crate) mod second_factor;
mod setup_flow;
pub(crate) mod step_up;
mod verification_flow;

pub(crate) use common::{authenticate_stage_token, guard_session_rotation};
pub use enable_flow::{EnableParams, enable};
pub use required_flow::{
    RequiredEnableParams, RequiredSetupParams, required_enable, required_setup,
};
pub use setup_flow::{SetupParams, regenerate_backup_codes, setup};
pub use step_up::{
    Category, close_category, delete_category_if_match, load_category_window, open_category_window,
};
pub use verification_flow::{
    DisableParams, VerifyParams, disable, ensure_mfa_can_be_disabled, status, verify,
};

#[cfg(test)]
pub(crate) use common::{write_required_enable, write_required_setup};
