use std::sync::Arc;

use tauri::State;

use crate::app_core::AppCore;
use crate::domain::types::{Category, NewCategory};
use crate::host::error::IpcError;
use crate::shared::ids::CategoryId;

#[tauri::command(async)]
pub fn category_list(core: State<Arc<AppCore>>) -> Result<Vec<Category>, IpcError> {
    Ok(core.categories.get_list()?)
}

#[tauri::command(async)]
pub fn category_create(
    core: State<Arc<AppCore>>,
    name: String,
    color: String,
    description: Option<String>,
    parent_id: Option<CategoryId>,
) -> Result<Category, IpcError> {
    Ok(core.categories.create(&NewCategory {
        name,
        color,
        description,
        parent_id,
    })?)
}

#[tauri::command(async)]
pub fn category_update(
    core: State<Arc<AppCore>>,
    id: CategoryId,
    name: String,
    color: String,
    description: Option<String>,
    parent_id: Option<CategoryId>,
) -> Result<(), IpcError> {
    core.categories.update(
        id,
        &NewCategory {
            name,
            color,
            description,
            parent_id,
        },
    )?;
    Ok(())
}

#[tauri::command(async)]
pub fn category_delete(core: State<Arc<AppCore>>, id: CategoryId) -> Result<(), IpcError> {
    Ok(core.categories.delete(id)?)
}
