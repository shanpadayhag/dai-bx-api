use serde::Serialize;

use crate::features::boards::entities::board::{
    Board, BoardCard, BoardList, BoardProperty, BoardPropertyKind, BoardSummary, CardValue,
    CardValueContent, PropertyOption,
};

#[derive(Debug, Serialize)]
pub struct BoardSummaryResponse {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct BoardListResponse {
    pub id: String,
    pub name: String,
    pub position: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CardValueContentResponse {
    OptionId(String),
    Text(String),
    Number(String),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardValueResponse {
    pub property_id: String,
    #[serde(flatten)]
    pub content: CardValueContentResponse,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardCardResponse {
    pub id: String,
    pub list_id: String,
    pub position: i32,
    pub title: String,
    pub due_date: Option<String>,
    pub due_time: Option<String>,
    pub values: Vec<CardValueResponse>,
    pub linked_task_count: i64,
    pub done_linked_task_count: i64,
    pub next_move: Option<NextMoveResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextMoveResponse {
    pub at: String,
    pub list_name: String,
}

#[derive(Debug, Serialize)]
pub struct PropertyOptionResponse {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub position: i32,
}

#[derive(Debug, Serialize)]
pub struct BoardPropertyResponse {
    pub id: String,
    pub name: String,
    pub kind: BoardPropertyKind,
    pub position: i32,
    pub options: Vec<PropertyOptionResponse>,
}

#[derive(Debug, Serialize)]
pub struct BoardResponse {
    pub board: BoardSummaryResponse,
    pub lists: Vec<BoardListResponse>,
    pub cards: Vec<BoardCardResponse>,
    pub properties: Vec<BoardPropertyResponse>,
}

impl From<BoardSummary> for BoardSummaryResponse {
    fn from(board_summary: BoardSummary) -> Self {
        Self {
            id: board_summary.id.to_string(),
            name: board_summary.name,
        }
    }
}

impl From<BoardList> for BoardListResponse {
    fn from(board_list: BoardList) -> Self {
        Self {
            id: board_list.id.to_string(),
            name: board_list.name,
            position: board_list.position,
        }
    }
}

impl From<CardValue> for CardValueResponse {
    fn from(card_value: CardValue) -> Self {
        Self {
            property_id: card_value.property_id.to_string(),
            content: match card_value.content {
                CardValueContent::Option(option_id) => {
                    CardValueContentResponse::OptionId(option_id.to_string())
                }
                CardValueContent::Text(text_value) => CardValueContentResponse::Text(text_value),
                CardValueContent::Number(number_value) => {
                    CardValueContentResponse::Number(number_value)
                }
            },
        }
    }
}

impl From<BoardCard> for BoardCardResponse {
    fn from(board_card: BoardCard) -> Self {
        Self {
            id: board_card.id.to_string(),
            list_id: board_card.list_id.to_string(),
            position: board_card.position,
            title: board_card.title,
            due_date: board_card.due_date,
            due_time: board_card.due_time,
            values: board_card
                .values
                .into_iter()
                .map(CardValueResponse::from)
                .collect(),
            linked_task_count: board_card.linked_task_count,
            done_linked_task_count: board_card.done_linked_task_count,
            next_move: board_card.next_move.map(|next_move| NextMoveResponse {
                at: next_move.at.to_iso_text(),
                list_name: next_move.list_name,
            }),
        }
    }
}

impl From<PropertyOption> for PropertyOptionResponse {
    fn from(property_option: PropertyOption) -> Self {
        Self {
            id: property_option.id.to_string(),
            name: property_option.name,
            color: property_option.color,
            position: property_option.position,
        }
    }
}

impl From<BoardProperty> for BoardPropertyResponse {
    fn from(board_property: BoardProperty) -> Self {
        Self {
            id: board_property.id.to_string(),
            name: board_property.name,
            kind: board_property.kind,
            position: board_property.position,
            options: board_property
                .options
                .into_iter()
                .map(PropertyOptionResponse::from)
                .collect(),
        }
    }
}

impl From<Board> for BoardResponse {
    fn from(board: Board) -> Self {
        Self {
            board: board.board.into(),
            lists: board
                .lists
                .into_iter()
                .map(BoardListResponse::from)
                .collect(),
            cards: board
                .cards
                .into_iter()
                .map(BoardCardResponse::from)
                .collect(),
            properties: board
                .properties
                .into_iter()
                .map(BoardPropertyResponse::from)
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    #[test]
    fn serializes_each_value_kind_under_its_own_key_with_the_number_as_a_string() {
        let property_id = Uuid::now_v7();
        let option_id = Uuid::now_v7();
        let serialized_values: Vec<serde_json::Value> = [
            CardValueContent::Option(option_id),
            CardValueContent::Text(String::from("https://example.com")),
            CardValueContent::Number(String::from("2.50")),
        ]
        .into_iter()
        .map(|content| {
            serde_json::to_value(CardValueResponse::from(CardValue {
                property_id,
                content,
            }))
            .expect("a value should serialize")
        })
        .collect();

        assert_eq!(
            serialized_values,
            vec![
                json!({ "propertyId": property_id.to_string(), "optionId": option_id.to_string() }),
                json!({ "propertyId": property_id.to_string(), "text": "https://example.com" }),
                json!({ "propertyId": property_id.to_string(), "number": "2.50" }),
            ]
        );
    }
}
