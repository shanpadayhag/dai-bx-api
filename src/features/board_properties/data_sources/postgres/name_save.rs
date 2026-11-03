use sqlx::postgres::PgQueryResult;

use crate::features::board_properties::entities::board_property::NameSave;

pub fn name_save_outcome(
    write_result: Result<PgQueryResult, sqlx::Error>,
) -> Result<NameSave, sqlx::Error> {
    match write_result {
        Ok(write_outcome) if write_outcome.rows_affected() == 0 => Ok(NameSave::Missing),
        Ok(_) => Ok(NameSave::Saved),
        Err(sqlx::Error::Database(database_error)) if database_error.is_unique_violation() => {
            Ok(NameSave::NameTaken)
        }
        Err(other_error) => Err(other_error),
    }
}
