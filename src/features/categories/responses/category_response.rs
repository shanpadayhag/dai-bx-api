use serde::Serialize;

use crate::features::categories::entities::category::Category;

#[derive(Debug, Serialize)]
pub struct CategoryResponse {
    pub id: String,
    pub name: String,
    pub color: String,
}

impl From<Category> for CategoryResponse {
    fn from(category: Category) -> Self {
        Self {
            id: category.id.to_string(),
            name: category.name,
            color: category.color,
        }
    }
}
