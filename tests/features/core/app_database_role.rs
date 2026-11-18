use sqlx::PgPool;

use crate::support::production_sized_pool;

#[sqlx::test]
async fn the_app_under_test_connects_as_the_limited_role(db_pool: PgPool) {
    let current_role: String = sqlx::query_scalar("SELECT current_user::text")
        .fetch_one(&production_sized_pool(&db_pool))
        .await
        .expect("the app pool should connect");

    assert_eq!(current_role, "daibx_app");
}
