use tauri::State;

use crate::domain::types::{Category, NewCategory};
use crate::host::error::IpcError;
use crate::shared::ids::CategoryId;
use crate::taxonomy::CategorySvc;

#[tauri::command]
pub fn list_categories(categories: State<Box<dyn CategorySvc>>) -> Result<Vec<Category>, IpcError> {
    Ok(categories.get_list()?)
}

#[tauri::command]
pub fn create_category(
    categories: State<Box<dyn CategorySvc>>,
    name: String,
    color: String,
    description: Option<String>,
    parent_id: Option<CategoryId>,
) -> Result<Category, IpcError> {
    Ok(categories.create(&NewCategory {
        name,
        color,
        description,
        parent_id,
    })?)
}

#[tauri::command]
pub fn update_category(
    categories: State<Box<dyn CategorySvc>>,
    id: CategoryId,
    name: String,
    color: String,
    description: Option<String>,
    parent_id: Option<CategoryId>,
) -> Result<(), IpcError> {
    categories.update(
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

#[tauri::command]
pub fn delete_category(categories: State<Box<dyn CategorySvc>>, id: CategoryId) -> Result<(), IpcError> {
    Ok(categories.delete(id)?)
}
