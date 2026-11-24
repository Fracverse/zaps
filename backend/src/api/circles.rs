use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::Serialize;
use serde_json::json;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use super::AuthenticatedUser;

#[derive(FromRow)]
struct CircleRow {
    id: Uuid,
    name: String,
    description: String,
    objective: String,
    goal_amount: i64,
    currency: String,
    progress_amount: i64,
    contribution_amount: i64,
    contribution_frequency: String,
    contribution_rules: String,
    membership_restrictions: String,
    visibility: String,
    member_count: i32,
    member_limit: Option<i32>,
    is_member: bool,
    join_enabled: bool,
    owner_username: String,
    created_at: String,
}

#[derive(Serialize)]
struct CircleDetails {
    id: Uuid,
    name: String,
    description: String,
    objective: String,
    goal_amount: String,
    currency: String,
    progress_amount: String,
    contribution_amount: String,
    contribution_frequency: String,
    contribution_rules: String,
    membership_restrictions: String,
    visibility: String,
    member_count: i32,
    member_limit: Option<i32>,
    is_member: bool,
    can_join: bool,
    join_unavailable_reason: Option<&'static str>,
    owner_username: String,
    created_at: String,
}

#[derive(Serialize)]
struct JoinResponse {
    joined: bool,
}

fn join_unavailable_reason(
    join_enabled: bool,
    member_count: i32,
    member_limit: Option<i32>,
) -> Option<&'static str> {
    if !join_enabled {
        Some("This circle is not accepting new members.")
    } else if member_limit.is_some_and(|limit| member_count >= limit) {
        Some("This circle has reached its member limit.")
    } else {
        None
    }
}

fn can_join_circle(
    visibility: &str,
    is_member: bool,
    join_enabled: bool,
    member_count: i32,
    member_limit: Option<i32>,
) -> bool {
    visibility == "PUBLIC"
        && !is_member
        && join_unavailable_reason(join_enabled, member_count, member_limit).is_none()
}

pub async fn get_circle(
    State(pool): State<PgPool>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(circle_id): Path<Uuid>,
) -> Response {
    let result = sqlx::query_as::<_, CircleRow>(
        r#"
        SELECT
            c.id,
            c.name,
            c.description,
            c.objective,
            c.goal_amount,
            c.currency,
            COALESCE((
                SELECT SUM(cc.amount)
                FROM circle_contributions cc
                WHERE cc.circle_id = c.id
            ), 0)::BIGINT AS progress_amount,
            c.contribution_amount,
            c.contribution_frequency,
            c.contribution_rules,
            c.membership_restrictions,
            c.visibility,
            (
                SELECT COUNT(*)::INT
                FROM circle_memberships cm
                WHERE cm.circle_id = c.id
            ) AS member_count,
            c.member_limit,
            EXISTS (
                SELECT 1
                FROM circle_memberships cm
                WHERE cm.circle_id = c.id AND cm.user_id = $2
            ) AS is_member,
            c.join_enabled,
            owner.username AS owner_username,
            c.created_at::TEXT AS created_at
        FROM circles c
        JOIN users owner ON owner.id = c.created_by
        WHERE c.id = $1
          AND (
              c.visibility = 'PUBLIC'
              OR EXISTS (
                  SELECT 1
                  FROM circle_memberships cm
                  WHERE cm.circle_id = c.id AND cm.user_id = $2
              )
          )
        "#,
    )
    .bind(circle_id)
    .bind(user.id)
    .fetch_optional(&pool)
    .await;

    match result {
        Ok(Some(circle)) => {
            let join_unavailable_reason = join_unavailable_reason(
                circle.join_enabled,
                circle.member_count,
                circle.member_limit,
            );
            let can_join = can_join_circle(
                &circle.visibility,
                circle.is_member,
                circle.join_enabled,
                circle.member_count,
                circle.member_limit,
            );

            Json(CircleDetails {
                id: circle.id,
                name: circle.name,
                description: circle.description,
                objective: circle.objective,
                goal_amount: circle.goal_amount.to_string(),
                currency: circle.currency,
                progress_amount: circle.progress_amount.to_string(),
                contribution_amount: circle.contribution_amount.to_string(),
                contribution_frequency: circle.contribution_frequency,
                contribution_rules: circle.contribution_rules,
                membership_restrictions: circle.membership_restrictions,
                visibility: circle.visibility,
                member_count: circle.member_count,
                member_limit: circle.member_limit,
                is_member: circle.is_member,
                can_join,
                join_unavailable_reason,
                owner_username: circle.owner_username,
                created_at: circle.created_at,
            })
            .into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Circle not found or unavailable." })),
        )
            .into_response(),
        Err(error) => {
            tracing::error!("Failed to fetch circle details: {error}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Unable to load circle details." })),
            )
                .into_response()
        }
    }
}

pub async fn join_circle(
    State(pool): State<PgPool>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(circle_id): Path<Uuid>,
) -> Response {
    let mut transaction = match pool.begin().await {
        Ok(transaction) => transaction,
        Err(error) => {
            tracing::error!("Failed to start circle join transaction: {error}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Unable to join this circle." })),
            )
                .into_response();
        }
    };

    let circle = sqlx::query_as::<_, (String, bool, Option<i32>)>(
        r#"
        SELECT visibility, join_enabled, member_limit
        FROM circles
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(circle_id)
    .fetch_optional(&mut *transaction)
    .await;

    let Some((visibility, join_enabled, member_limit)) = (match circle {
        Ok(circle) => circle,
        Err(error) => {
            tracing::error!("Failed to read circle before joining: {error}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Unable to join this circle." })),
            )
                .into_response();
        }
    }) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Circle not found or unavailable." })),
        )
            .into_response();
    };

    if visibility != "PUBLIC" {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Circle not found or unavailable." })),
        )
            .into_response();
    }

    let is_member = match sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM circle_memberships WHERE circle_id = $1 AND user_id = $2)",
    )
    .bind(circle_id)
    .bind(user.id)
    .fetch_one(&mut *transaction)
    .await
    {
        Ok(is_member) => is_member,
        Err(error) => {
            tracing::error!("Failed to check circle membership before joining: {error}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Unable to join this circle." })),
            )
                .into_response();
        }
    };

    if is_member {
        return Json(JoinResponse { joined: true }).into_response();
    }

    let member_count = match sqlx::query_scalar::<_, i32>(
        "SELECT COUNT(*)::INT FROM circle_memberships WHERE circle_id = $1",
    )
    .bind(circle_id)
    .fetch_one(&mut *transaction)
    .await
    {
        Ok(count) => count,
        Err(error) => {
            tracing::error!("Failed to count circle members before joining: {error}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Unable to join this circle." })),
            )
                .into_response();
        }
    };

    if let Some(reason) = join_unavailable_reason(join_enabled, member_count, member_limit) {
        return (StatusCode::CONFLICT, Json(json!({ "error": reason }))).into_response();
    }

    if let Err(error) = sqlx::query(
        "INSERT INTO circle_memberships (circle_id, user_id) VALUES ($1, $2)",
    )
    .bind(circle_id)
    .bind(user.id)
    .execute(&mut *transaction)
    .await
    {
        tracing::error!("Failed to add circle membership: {error}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Unable to join this circle." })),
        )
            .into_response();
    }

    if let Err(error) = transaction.commit().await {
        tracing::error!("Failed to commit circle membership: {error}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Unable to join this circle." })),
        )
            .into_response();
    }

    Json(JoinResponse { joined: true }).into_response()
}

#[cfg(test)]
mod tests {
    use super::{can_join_circle, join_unavailable_reason};

    #[test]
    fn join_requires_an_open_slot_and_an_accepting_circle() {
        assert_eq!(join_unavailable_reason(true, 4, Some(5)), None);
        assert_eq!(
            join_unavailable_reason(true, 5, Some(5)),
            Some("This circle has reached its member limit.")
        );
        assert_eq!(
            join_unavailable_reason(false, 0, None),
            Some("This circle is not accepting new members.")
        );
        assert_eq!(join_unavailable_reason(true, 100, None), None);
    }

    #[test]
    fn only_eligible_nonmembers_can_join_public_circles() {
        assert!(can_join_circle("PUBLIC", false, true, 4, Some(5)));
        assert!(!can_join_circle("PRIVATE", false, true, 4, Some(5)));
        assert!(!can_join_circle("PUBLIC", true, true, 4, Some(5)));
        assert!(!can_join_circle("PUBLIC", false, false, 4, Some(5)));
        assert!(!can_join_circle("PUBLIC", false, true, 5, Some(5)));
    }
}
