use dioxus::prelude::document;
use serde::de::DeserializeOwned;

/// Evaluate JavaScript that is safe to run again. Dioxus can report `Finished`
/// when an evaluator has already completed; a fresh evaluator is sufficient for
/// read-only/idempotent bridge operations and avoids surfacing a false failure.
pub(super) async fn join_idempotent<T: DeserializeOwned>(
    script: &str,
) -> Result<T, document::EvalError> {
    match document::eval(script).join::<T>().await {
        Err(document::EvalError::Finished) => document::eval(script).join::<T>().await,
        result => result,
    }
}
