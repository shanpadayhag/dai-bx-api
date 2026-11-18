use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, Response, StatusCode, header};
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, PgPool};

use crate::board_properties::create::{add_property, option_id, property_id};
use crate::boards::create::board_id;
use crate::calendar_events::create::{create_planned_event, event_id, post_event};
use crate::card_schedules::create::{
    board_with_review_card, create_schedule, once_at, schedule_id, weekly_review,
};
use crate::categories::update::find_category_id;
use crate::support::{
    STRONG_PASSWORD, SignedInPerson, build_test_app, post_json_with_cookies, response_json, send,
    sign_up_with_authenticator,
};
use crate::tracking::finish::minutes_from_now;
use crate::tracking::start::{add_top_level_task, start_task};

const SERIES_FIRST_START: &str = "2026-11-09T09:00:00.000Z";
const TABLES_OUTSIDE_THE_SNAPSHOT: &[&str] = &[
    "_sqlx_migrations",
    "auth_challenges",
    "auth_throttles",
    "email_tokens",
    "invites",
    "known_browsers",
    "pending_sign_in_alerts",
    "recovery_codes",
    "remembered_devices",
    "security_events",
    "session_tokens",
    "sessions",
];

struct AccountRecords {
    board_id: String,
    list_id: String,
    card_id: String,
    property_id: String,
    option_id: String,
    schedule_id: String,
    category_id: String,
    task_group_id: String,
    task_id: String,
    calendar_event_id: String,
    series_id: String,
    tracked_event_id: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RouteAccess {
    Public,
    AnySession,
    OwnedRecord,
}

type RequestBody = fn(&AccountRecords) -> Value;

struct AppRoute {
    method: Method,
    path: &'static str,
    query: &'static str,
    access: RouteAccess,
    request_body: RequestBody,
}

fn empty_body(_: &AccountRecords) -> Value {
    json!({})
}

fn route(
    method: Method,
    path: &'static str,
    access: RouteAccess,
    request_body: RequestBody,
) -> AppRoute {
    AppRoute {
        method,
        path,
        query: "",
        access,
        request_body,
    }
}

fn public(method: Method, path: &'static str) -> AppRoute {
    route(method, path, RouteAccess::Public, empty_body)
}

fn any_session(method: Method, path: &'static str) -> AppRoute {
    route(method, path, RouteAccess::AnySession, empty_body)
}

fn owned(method: Method, path: &'static str) -> AppRoute {
    route(method, path, RouteAccess::OwnedRecord, empty_body)
}

fn owned_with(method: Method, path: &'static str, request_body: RequestBody) -> AppRoute {
    route(method, path, RouteAccess::OwnedRecord, request_body)
}

fn app_routes() -> Vec<AppRoute> {
    vec![
        public(Method::GET, "/health"),
        public(Method::POST, "/auth/sign-up"),
        public(Method::POST, "/auth/verify-email"),
        public(Method::POST, "/auth/verify-email/resend"),
        public(Method::GET, "/auth/mfa/setup"),
        public(Method::POST, "/auth/mfa/setup/confirm"),
        public(Method::POST, "/auth/sign-in"),
        public(Method::POST, "/auth/mfa/verify"),
        public(Method::POST, "/auth/refresh"),
        public(Method::POST, "/auth/password-reset"),
        public(Method::POST, "/auth/password-reset/complete"),
        public(Method::POST, "/account/email/confirm"),
        any_session(Method::POST, "/auth/sign-out"),
        any_session(Method::GET, "/auth/session"),
        any_session(Method::GET, "/account"),
        any_session(Method::PATCH, "/account"),
        any_session(Method::POST, "/account/password"),
        any_session(Method::POST, "/account/email"),
        any_session(Method::POST, "/account/recovery-codes"),
        any_session(Method::POST, "/account/authenticator"),
        any_session(Method::POST, "/account/authenticator/confirm"),
        any_session(Method::GET, "/account/sessions"),
        any_session(Method::POST, "/account/sessions/end"),
        any_session(Method::POST, "/account/sessions/end-all"),
        any_session(Method::GET, "/account/time-tracking"),
        any_session(Method::PATCH, "/account/time-tracking"),
        any_session(Method::POST, "/account/delete"),
        any_session(Method::GET, "/boards"),
        any_session(Method::POST, "/boards"),
        any_session(Method::GET, "/calendar/events"),
        any_session(Method::POST, "/calendar/events"),
        any_session(Method::GET, "/categories"),
        any_session(Method::POST, "/categories"),
        any_session(Method::GET, "/task-groups"),
        any_session(Method::POST, "/task-groups"),
        any_session(Method::POST, "/task-groups/import"),
        any_session(Method::GET, "/tracking/running"),
        owned(Method::GET, "/boards/{board_id}"),
        owned_with(
            Method::PATCH,
            "/boards/{board_id}",
            |_| json!({ "name": "Taken" }),
        ),
        owned(Method::DELETE, "/boards/{board_id}"),
        owned_with(
            Method::POST,
            "/boards/{board_id}/lists",
            |_| json!({ "name": "Taken" }),
        ),
        owned_with(
            Method::POST,
            "/boards/{board_id}/properties",
            |_| json!({ "name": "Taken", "kind": "text" }),
        ),
        owned(Method::GET, "/boards/{board_id}/archived-cards"),
        owned_with(
            Method::PATCH,
            "/lists/{list_id}",
            |_| json!({ "name": "Taken" }),
        ),
        owned(Method::DELETE, "/lists/{list_id}"),
        owned_with(
            Method::POST,
            "/lists/{list_id}/move",
            |_| json!({ "position": 0 }),
        ),
        owned(Method::GET, "/lists/{list_id}/export"),
        owned_with(
            Method::POST,
            "/lists/{list_id}/cards",
            |_| json!({ "title": "Taken" }),
        ),
        owned_with(
            Method::PATCH,
            "/properties/{property_id}",
            |_| json!({ "name": "Taken" }),
        ),
        owned(Method::DELETE, "/properties/{property_id}"),
        owned_with(
            Method::POST,
            "/properties/{property_id}/move",
            |_| json!({ "position": 0 }),
        ),
        owned_with(
            Method::POST,
            "/properties/{property_id}/options",
            |_| json!({ "name": "Taken" }),
        ),
        owned_with(
            Method::PATCH,
            "/options/{option_id}",
            |_| json!({ "name": "Taken" }),
        ),
        owned(Method::DELETE, "/options/{option_id}"),
        owned_with(
            Method::POST,
            "/options/{option_id}/move",
            |_| json!({ "position": 0 }),
        ),
        owned(Method::GET, "/cards/{card_id}"),
        owned_with(
            Method::PATCH,
            "/cards/{card_id}",
            |_| json!({ "title": "Taken" }),
        ),
        owned(Method::DELETE, "/cards/{card_id}"),
        owned_with(
            Method::POST,
            "/cards/{card_id}/move",
            |records| json!({ "listId": records.list_id, "position": 0 }),
        ),
        owned_with(
            Method::PUT,
            "/cards/{card_id}/triggers",
            |records| json!({ "startedListId": records.list_id, "finishedListId": null }),
        ),
        owned(Method::POST, "/cards/{card_id}/archive"),
        owned(Method::POST, "/cards/{card_id}/restore"),
        owned(Method::GET, "/cards/{card_id}/history"),
        owned_with(
            Method::PUT,
            "/cards/{card_id}/values/{property_id}",
            |records| json!({ "optionId": records.option_id }),
        ),
        owned(Method::DELETE, "/cards/{card_id}/values/{property_id}"),
        owned_with(Method::POST, "/cards/{card_id}/schedules", |records| {
            once_at(&records.list_id, "2030-01-07T18:00")
        }),
        owned(Method::POST, "/cards/{card_id}/start"),
        owned_with(Method::PATCH, "/schedules/{schedule_id}", |records| {
            weekly_review(&records.list_id)
        }),
        owned(Method::DELETE, "/schedules/{schedule_id}"),
        owned(Method::POST, "/schedules/{schedule_id}/pause"),
        owned(Method::POST, "/schedules/{schedule_id}/resume"),
        owned(Method::POST, "/schedules/{schedule_id}/run-now"),
        owned(Method::GET, "/schedules/{schedule_id}/runs"),
        owned_with(
            Method::PATCH,
            "/categories/{category_id}",
            |_| json!({ "name": "Taken" }),
        ),
        owned(Method::DELETE, "/categories/{category_id}"),
        owned_with(
            Method::PATCH,
            "/task-groups/{task_group_id}",
            |_| json!({ "name": "Taken" }),
        ),
        owned(Method::DELETE, "/task-groups/{task_group_id}"),
        owned(Method::GET, "/task-groups/{task_group_id}/tasks"),
        owned_with(
            Method::POST,
            "/task-groups/{task_group_id}/tasks",
            |_| json!({ "name": "Taken" }),
        ),
        owned_with(
            Method::PATCH,
            "/tasks/{task_id}",
            |_| json!({ "name": "Taken" }),
        ),
        owned(Method::DELETE, "/tasks/{task_id}"),
        owned(Method::POST, "/tasks/{task_id}/complete"),
        owned(Method::POST, "/tasks/{task_id}/reopen"),
        owned_with(
            Method::POST,
            "/tasks/{task_id}/move",
            |records| json!({ "groupId": records.task_group_id, "parentId": null, "position": 0 }),
        ),
        owned(Method::POST, "/tasks/{task_id}/start"),
        owned_with(
            Method::PATCH,
            "/calendar/events/{event_id}",
            |_| json!({ "title": "Taken" }),
        ),
        owned(Method::DELETE, "/calendar/events/{event_id}"),
        owned_with(
            Method::PATCH,
            "/calendar/series/{series_id}/occurrences/{original_start}",
            |_| json!({ "scope": "this", "title": "Taken" }),
        ),
        AppRoute {
            query: "?scope=this",
            ..owned(
                Method::DELETE,
                "/calendar/series/{series_id}/occurrences/{original_start}",
            )
        },
        owned(Method::POST, "/tracking/{event_id}/check-in"),
        owned_with(
            Method::POST,
            "/tracking/{event_id}/finish",
            |_| json!({ "endsAt": minutes_from_now(1) }),
        ),
    ]
}

fn filled_path(app_route: &AppRoute, records: &AccountRecords) -> String {
    let event_id = if app_route.path.starts_with("/tracking/") {
        &records.tracked_event_id
    } else {
        &records.calendar_event_id
    };
    let filled_template = app_route
        .path
        .replace("{board_id}", &records.board_id)
        .replace("{list_id}", &records.list_id)
        .replace("{card_id}", &records.card_id)
        .replace("{property_id}", &records.property_id)
        .replace("{option_id}", &records.option_id)
        .replace("{schedule_id}", &records.schedule_id)
        .replace("{category_id}", &records.category_id)
        .replace("{task_group_id}", &records.task_group_id)
        .replace("{task_id}", &records.task_id)
        .replace("{series_id}", &records.series_id)
        .replace("{original_start}", SERIES_FIRST_START)
        .replace("{event_id}", event_id);
    format!("{filled_template}{}", app_route.query)
}

async fn create_account_records(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
) -> AccountRecords {
    let scheduled_card = board_with_review_card(db_pool, signed_in_person).await;
    let board = add_property(
        db_pool,
        signed_in_person,
        board_id(&scheduled_card.board),
        json!({ "name": "Size", "kind": "choice", "options": [{ "name": "Small" }] }),
    )
    .await;
    let created_schedule = create_schedule(
        db_pool,
        signed_in_person,
        &scheduled_card.card_id,
        weekly_review(&scheduled_card.to_do_list_id),
    )
    .await;
    let category_response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/categories",
        json!({ "name": "Private work", "color": "#123456" }),
        &signed_in_person.session_cookies,
    )
    .await;
    assert!(category_response.status().is_success());
    let (task_id, task_group_id) =
        add_top_level_task(db_pool, signed_in_person, "Private task").await;
    let planned_event = create_planned_event(
        db_pool,
        signed_in_person,
        "Private event",
        "2026-11-10T09:00",
        "2026-11-10T10:00",
    )
    .await;
    let series_response = post_event(
        db_pool,
        signed_in_person,
        json!({
            "title": "Private series",
            "start": "2026-11-09T09:00",
            "end": "2026-11-09T10:00",
            "inputZone": "UTC",
            "repeat": {
                "frequency": "weekly",
                "interval": 1,
                "weekdays": ["mon"],
                "ends": { "never": true },
            },
        }),
    )
    .await;
    assert_eq!(series_response.status(), StatusCode::CREATED);
    let created_series = response_json(series_response).await;
    let running_item = start_task(db_pool, signed_in_person, &task_id).await;
    AccountRecords {
        board_id: board_id(&board).to_owned(),
        list_id: scheduled_card.to_do_list_id,
        card_id: scheduled_card.card_id,
        property_id: property_id(&board, "Size").to_owned(),
        option_id: option_id(&board, "Size", "Small").to_owned(),
        schedule_id: schedule_id(&created_schedule).to_owned(),
        category_id: find_category_id(db_pool, signed_in_person, "Private work").await,
        task_group_id,
        task_id,
        calendar_event_id: event_id(&planned_event).to_owned(),
        series_id: created_series["series"]["id"]
            .as_str()
            .expect("a series id")
            .to_owned(),
        tracked_event_id: running_item["id"]
            .as_str()
            .expect("a tracked event id")
            .to_owned(),
    }
}

async fn send_as(
    test_app: Router,
    method: Method,
    request_path: &str,
    request_body: &Value,
    cookie_header: &str,
) -> Response<Body> {
    let mut request_builder = Request::builder()
        .method(method)
        .uri(request_path)
        .header("sec-fetch-site", "same-origin")
        .header(header::CONTENT_TYPE, "application/json");
    if !cookie_header.is_empty() {
        request_builder = request_builder.header(header::COOKIE, cookie_header);
    }
    let request = request_builder
        .body(Body::from(request_body.to_string()))
        .expect("test request should build");
    send(test_app, request).await
}

async fn snapshot_user_data(db_pool: &PgPool) -> Vec<(String, Value)> {
    let table_names: Vec<String> = sqlx::query_scalar(
        "SELECT tablename::text FROM pg_tables WHERE schemaname = 'public' ORDER BY tablename",
    )
    .fetch_all(db_pool)
    .await
    .expect("listing tables should succeed");
    let mut table_snapshots = Vec::new();
    for table_name in table_names {
        if TABLES_OUTSIDE_THE_SNAPSHOT.contains(&table_name.as_str()) {
            continue;
        }
        let table_rows: Value = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT coalesce(jsonb_agg(to_jsonb(row_data) ORDER BY to_jsonb(row_data)::text), '[]')
             FROM {table_name} AS row_data"
        )))
        .fetch_one(db_pool)
        .await
        .expect("reading a table should succeed");
        table_snapshots.push((table_name, table_rows));
    }
    table_snapshots
}

fn declared_methods(route_arguments: &str) -> Vec<&'static str> {
    [
        ("get", "GET"),
        ("post", "POST"),
        ("patch", "PATCH"),
        ("put", "PUT"),
        ("delete", "DELETE"),
    ]
    .into_iter()
    .filter(|(builder_name, _)| {
        route_arguments
            .match_indices(&format!("{builder_name}("))
            .any(|(match_start, _)| {
                !route_arguments[..match_start]
                    .ends_with(|previous: char| previous.is_alphanumeric() || previous == '_')
            })
    })
    .map(|(_, method_name)| method_name)
    .collect()
}

fn closing_parenthesis(source_text: &str) -> usize {
    let mut open_count = 1;
    for (character_index, source_character) in source_text.char_indices() {
        match source_character {
            '(' => open_count += 1,
            ')' => open_count -= 1,
            _ => {}
        }
        if open_count == 0 {
            return character_index;
        }
    }
    panic!("a route should close its parenthesis");
}

fn nest_prefix(app_source: &str, feature_name: &str) -> String {
    let router_call = format!("{feature_name}::router::router())");
    app_source
        .lines()
        .find(|source_line| source_line.contains(".nest(") && source_line.contains(&router_call))
        .and_then(|source_line| source_line.split('"').nth(1))
        .unwrap_or("")
        .to_owned()
}

fn routes_declared_in_the_app() -> BTreeSet<(String, String)> {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let app_source = fs::read_to_string(crate_root.join("src/lib.rs")).expect("src/lib.rs");
    let mut declared_routes = BTreeSet::new();
    for feature_entry in fs::read_dir(crate_root.join("src/features")).expect("the features") {
        let feature_path = feature_entry.expect("a feature").path();
        let Ok(router_source) = fs::read_to_string(feature_path.join("router.rs")) else {
            continue;
        };
        let feature_name = feature_path
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .expect("a feature name");
        let route_prefix = nest_prefix(&app_source, feature_name);
        for route_source in router_source.split(".route(").skip(1) {
            let route_arguments = &route_source[..closing_parenthesis(route_source)];
            let route_path = route_arguments.split('"').nth(1).expect("a route path");
            let full_path = match route_path {
                "/" => route_prefix.clone(),
                _ => format!("{route_prefix}{route_path}"),
            };
            let (_, method_source) = route_arguments
                .split_once(',')
                .expect("a route should name its handlers");
            for method_name in declared_methods(method_source) {
                declared_routes.insert((method_name.to_owned(), full_path.clone()));
            }
        }
    }
    declared_routes
}

#[test]
fn the_table_lists_every_route_in_the_app() {
    let listed_routes: BTreeSet<(String, String)> = app_routes()
        .iter()
        .map(|app_route| (app_route.method.to_string(), app_route.path.to_owned()))
        .collect();

    let declared_routes = routes_declared_in_the_app();

    assert!(!declared_routes.is_empty());
    assert_eq!(
        declared_routes
            .difference(&listed_routes)
            .collect::<Vec<_>>(),
        Vec::<&(String, String)>::new(),
        "add each new route to the table"
    );
    assert_eq!(
        listed_routes
            .difference(&declared_routes)
            .collect::<Vec<_>>(),
        Vec::<&(String, String)>::new(),
        "remove routes that no longer exist"
    );
}

#[sqlx::test]
async fn every_signed_in_route_refuses_a_request_without_a_session(db_pool: PgPool) {
    let record_owner = sign_up_with_authenticator(&db_pool).await;
    let owner_records = create_account_records(&db_pool, &record_owner).await;
    let data_before = snapshot_user_data(&db_pool).await;

    for app_route in app_routes()
        .iter()
        .filter(|app_route| app_route.access != RouteAccess::Public)
    {
        let response = send_as(
            build_test_app(db_pool.clone()),
            app_route.method.clone(),
            &filled_path(app_route, &owner_records),
            &(app_route.request_body)(&owner_records),
            "",
        )
        .await;

        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{} {}",
            app_route.method,
            app_route.path
        );
    }

    assert_eq!(snapshot_user_data(&db_pool).await, data_before);
}

#[sqlx::test]
async fn every_owned_route_refuses_another_account_and_changes_nothing(db_pool: PgPool) {
    let record_owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let owner_records = create_account_records(&db_pool, &record_owner).await;
    let data_before = snapshot_user_data(&db_pool).await;

    for app_route in app_routes()
        .iter()
        .filter(|app_route| app_route.access == RouteAccess::OwnedRecord)
    {
        let response = send_as(
            build_test_app(db_pool.clone()),
            app_route.method.clone(),
            &filled_path(app_route, &owner_records),
            &(app_route.request_body)(&owner_records),
            &other_person.session_cookies,
        )
        .await;

        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{} {}",
            app_route.method,
            app_route.path
        );
        assert_eq!(
            snapshot_user_data(&db_pool).await,
            data_before,
            "{} {} changed data",
            app_route.method,
            app_route.path
        );
    }
}

#[sqlx::test]
async fn an_own_record_cannot_point_at_another_accounts_records(db_pool: PgPool) {
    let record_owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let owner_records = create_account_records(&db_pool, &record_owner).await;
    let own_records = create_account_records(&db_pool, &other_person).await;
    let data_before = snapshot_user_data(&db_pool).await;
    let requests_with_foreign_ids = [
        (
            Method::POST,
            format!("/cards/{}/move", own_records.card_id),
            json!({ "listId": owner_records.list_id, "position": 0 }),
        ),
        (
            Method::PUT,
            format!("/cards/{}/triggers", own_records.card_id),
            json!({ "startedListId": owner_records.list_id, "finishedListId": null }),
        ),
        (
            Method::PUT,
            format!(
                "/cards/{}/values/{}",
                own_records.card_id, owner_records.property_id
            ),
            json!({ "optionId": owner_records.option_id }),
        ),
        (
            Method::PUT,
            format!(
                "/cards/{}/values/{}",
                own_records.card_id, own_records.property_id
            ),
            json!({ "optionId": owner_records.option_id }),
        ),
        (
            Method::POST,
            format!("/cards/{}/schedules", own_records.card_id),
            once_at(&owner_records.list_id, "2030-01-07T18:00"),
        ),
        (
            Method::PATCH,
            format!("/schedules/{}", own_records.schedule_id),
            weekly_review(&owner_records.list_id),
        ),
        (
            Method::POST,
            format!("/task-groups/{}/tasks", own_records.task_group_id),
            json!({ "name": "Taken", "parentId": owner_records.task_id }),
        ),
        (
            Method::POST,
            format!("/tasks/{}/move", own_records.task_id),
            json!({ "groupId": owner_records.task_group_id, "parentId": null, "position": 0 }),
        ),
        (
            Method::POST,
            format!("/tasks/{}/move", own_records.task_id),
            json!({
                "groupId": own_records.task_group_id,
                "parentId": owner_records.task_id,
                "position": 0,
            }),
        ),
        (
            Method::POST,
            String::from("/calendar/events"),
            json!({
                "title": "Taken",
                "categoryId": owner_records.category_id,
                "start": "2026-11-11T09:00",
                "end": "2026-11-11T10:00",
                "inputZone": "UTC",
            }),
        ),
        (
            Method::PATCH,
            format!("/calendar/events/{}", own_records.calendar_event_id),
            json!({ "categoryId": owner_records.category_id }),
        ),
        (
            Method::PATCH,
            String::from("/account/time-tracking"),
            json!({ "trackedCategoryId": owner_records.category_id }),
        ),
    ];

    for (request_method, request_path, request_body) in requests_with_foreign_ids {
        let response = send_as(
            build_test_app(db_pool.clone()),
            request_method.clone(),
            &request_path,
            &request_body,
            &other_person.session_cookies,
        )
        .await;

        assert!(
            response.status().is_client_error(),
            "{request_method} {request_path} answered {}",
            response.status()
        );
        assert_eq!(
            snapshot_user_data(&db_pool).await,
            data_before,
            "{request_method} {request_path} changed data"
        );
    }
}

#[sqlx::test]
async fn another_accounts_session_cannot_be_ended(db_pool: PgPool) {
    let record_owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let active_session_sql = "SELECT sessions.id
         FROM sessions JOIN users ON users.id = sessions.user_id
         WHERE users.email = $1 AND sessions.revoked_at IS NULL";
    let owner_session_id: uuid::Uuid = sqlx::query_scalar(active_session_sql)
        .bind(&record_owner.email)
        .fetch_one(&db_pool)
        .await
        .expect("the owner should have a session");

    let response = send_as(
        build_test_app(db_pool.clone()),
        Method::POST,
        "/account/sessions/end",
        &json!({ "sessionId": owner_session_id.to_string(), "password": STRONG_PASSWORD }),
        &other_person.session_cookies,
    )
    .await;

    assert!(response.status().is_client_error());
    let still_active_session_id: uuid::Uuid = sqlx::query_scalar(active_session_sql)
        .bind(&record_owner.email)
        .fetch_one(&db_pool)
        .await
        .expect("the owner's session should still be active");
    assert_eq!(still_active_session_id, owner_session_id);
}
