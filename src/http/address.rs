use axum::{
    extract::Path,
    routing::{delete, post},
    Extension, Json, Router,
};
use hyper::StatusCode;
use serde::Deserialize;
use sqlx::PgPool;
use tracing::info;
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use super::{auth::WriteUser, error::ApiError};

#[derive(Debug, Validate, Deserialize, ToSchema)]
pub struct NewAddress {
    #[validate(length(min = 1, max = 64))]
    building: String,
    #[validate(length(min = 1, max = 64))]
    street: Option<String>,
    #[validate(length(min = 1, max = 64))]
    town_or_city: Option<String>,
    #[validate(length(min = 1, max = 8))]
    postcode: String,
}

/// Create an address for a person
///
/// Requires the scope `write`
#[utoipa::path(
    post,
    tag = "address",
    path = "/person/{person_uuid}/address",
    request_body = NewAddress,
    params(
        ("person_uuid" = Uuid, Path, description = "The UUID of the person to create an address for")
    ),
    responses(
        (status = 201, description = "Address created successfully"),
        (status = 404, description = "Person not found", body = ErrorResponse),
    ),
    security(
        ("bearer" = []),
    )
)]
pub async fn add_address(
    user: WriteUser,
    db: Extension<PgPool>,
    Json(request): Json<NewAddress>,
    Path(person_uuid): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    request.validate()?;

    sqlx::query!(
        r#"
            WITH new_address AS (
                INSERT INTO address(building, street, town_or_city, postcode)
                VALUES ($1, $2, $3, $4)
                RETURNING uuid, created, last_edited, building, street, town_or_city, postcode
            )
            UPDATE person p
            SET address = new_address.uuid, last_edited = now()
            FROM new_address
            WHERE p.uuid = $5
            RETURNING p.uuid as id;
        "#,
        request.building,
        request.street,
        request.town_or_city,
        request.postcode,
        person_uuid,
    )
    .fetch_one(&*db)
    .await
    .map_err(|e| match e {
        sqlx::Error::RowNotFound => {
            ApiError::NotFound(format!("Person not found for the UUID: {person_uuid}"))
        }
        _ => ApiError::DatabaseError(e),
    })?;

    info!(
        "Client '{}' created an address for the person '{}'",
        user.username, person_uuid
    );

    Ok(StatusCode::CREATED)
}

/// Remove an address
///
/// Requires the scope `write`
#[utoipa::path(
    delete,
    tag = "address",
    path = "/address/{address_uuid}",
    params(
        ("address_uuid" = Uuid, Path, description = "The UUID of the address to remove")
    ),
    responses(
        (status = 200, description = "Address deleted successfully"),
        (status = 404, description = "Address not found", body = ErrorResponse),
    ),
    security(
        ("bearer" = [])
    )
)]
pub async fn remove_address(
    user: WriteUser,
    db: Extension<PgPool>,
    Path(address_uuid): Path<Uuid>,
) -> Result<(), ApiError> {
    let mut tx = db.begin().await?;

    sqlx::query!(
        r#"
            UPDATE person SET address = NULL WHERE address = $1
            RETURNING id;
        "#,
        address_uuid
    )
    .fetch_all(&mut tx)
    .await
    .map_err(|e| match e {
        sqlx::Error::RowNotFound => {
            ApiError::NotFound(format!("Person not found with the address: {address_uuid}"))
        }
        _ => ApiError::DatabaseError(e),
    })?;

    sqlx::query!(
        r#"
            DELETE FROM address WHERE uuid = $1
            RETURNING id;
        "#,
        address_uuid
    )
    .fetch_one(&mut tx)
    .await
    .map_err(|e| match e {
        sqlx::Error::RowNotFound => {
            ApiError::NotFound(format!("Address not found for the UUID: {address_uuid}"))
        }
        _ => ApiError::DatabaseError(e),
    })?;

    tx.commit().await?;

    info!(
        "Client '{}' deleted the address '{}'",
        user.username, address_uuid
    );

    Ok(())
}

pub fn router() -> Router {
    Router::new()
        .route("/person/:person_uuid/address", post(add_address))
        .route("/address/:address_uuid", delete(remove_address))
}
