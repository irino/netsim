use std::sync::Mutex;
use std::sync::OnceLock;

use ir::DeviceConfig;
use thiserror::Error;
use yang3::context::Context;
use yang3::data::{DataFormat, DataParserFlags, DataTree, DataValidationFlags};

use crate::context::create_yang_context;
use crate::mapper::{map_device_config_to_yang_json, YangInstanceDocuments};

/// Singleton context shared across validations to avoid reloading YANG schemas on each run.
static GLOBAL_CONTEXT: OnceLock<Mutex<Context>> = OnceLock::new();

fn get_shared_context() -> &'static Mutex<Context> {
    GLOBAL_CONTEXT.get_or_init(|| Mutex::new(create_yang_context()))
}

/// Errors occurring during IETF YANG validation.
#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("libyang validation failed for {module_section}: {message}")]
    LibyangError {
        module_section: String,
        message: String,
    },
    #[error("JSON serialization or mapping error: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// Validates raw JSON text against the loaded YANG schemas in the provided context.
pub fn validate_yang_json(
    ctx: &Context,
    json_str: &str,
    section: &str,
) -> Result<(), ValidationError> {
    let mut tree = DataTree::parse_string(
        ctx,
        json_str,
        DataFormat::JSON,
        DataParserFlags::STRICT,
        DataValidationFlags::NO_STATE,
    )
    .map_err(|e| ValidationError::LibyangError {
        module_section: section.to_string(),
        message: e.to_string(),
    })?;

    tree.validate(DataValidationFlags::NO_STATE)
        .map_err(|e| ValidationError::LibyangError {
            module_section: section.to_string(),
            message: e.to_string(),
        })?;

    Ok(())
}

/// Validates a `DeviceConfig` against pinned IETF YANG data models.
///
/// Both the unified inter-module document (resolving leafrefs across interfaces and routing)
/// and standalone partitioned documents are validated.
pub fn validate_device_config(
    config: &DeviceConfig,
) -> Result<YangInstanceDocuments, ValidationError> {
    let docs = map_device_config_to_yang_json(config);
    let ctx_lock = get_shared_context().lock().unwrap();

    // 1. Validate the combined document to ensure inter-module cross-references (leafrefs) resolve
    validate_yang_json(
        &ctx_lock,
        &docs.combined_json,
        "ietf-interfaces & ietf-routing combined",
    )?;

    // 2. Validate standalone interfaces if present
    if let Some(ref if_json) = docs.interfaces_json {
        validate_yang_json(&ctx_lock, if_json, "ietf-interfaces / ietf-ip")?;
    }

    Ok(docs)
}
