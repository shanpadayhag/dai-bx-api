use serde::Serialize;

use crate::features::board_lists::entities::board_list::{ExportedCard, ExportedValue, ListExport};

#[derive(Debug, Serialize)]
pub struct NamedResponse {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct ExportedValueResponse {
    pub property: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedCardResponse {
    pub id: String,
    pub title: String,
    pub due_date: Option<String>,
    pub due_time: Option<String>,
    pub values: Vec<ExportedValueResponse>,
    pub notes: String,
}

#[derive(Debug, Serialize)]
pub struct ListExportResponse {
    pub board: NamedResponse,
    pub list: NamedResponse,
    pub cards: Vec<ExportedCardResponse>,
}

impl From<ExportedValue> for ExportedValueResponse {
    fn from(exported_value: ExportedValue) -> Self {
        Self {
            property: exported_value.property_name,
            value: exported_value.value,
        }
    }
}

impl From<ExportedCard> for ExportedCardResponse {
    fn from(exported_card: ExportedCard) -> Self {
        Self {
            id: exported_card.id.to_string(),
            title: exported_card.title,
            due_date: exported_card.due_date,
            due_time: exported_card.due_time,
            values: exported_card
                .values
                .into_iter()
                .map(ExportedValueResponse::from)
                .collect(),
            notes: exported_card.notes,
        }
    }
}

impl From<ListExport> for ListExportResponse {
    fn from(list_export: ListExport) -> Self {
        Self {
            board: NamedResponse {
                name: list_export.header.board_name,
            },
            list: NamedResponse {
                name: list_export.header.list_name,
            },
            cards: list_export
                .cards
                .into_iter()
                .map(ExportedCardResponse::from)
                .collect(),
        }
    }
}
