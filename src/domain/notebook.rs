use serde::{Deserialize, Serialize};

pub const DEFAULT_NOTEBOOK_ID: &str = "notebook_notes";
pub const DEFAULT_NOTEBOOK_NAME: &str = "Notes";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notebook {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub sort_order: i64,
    pub created_at: i64,
    pub updated_at: i64,
}
