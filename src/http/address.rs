use axum::{
    extract::Path,
    routing::{delete, post},
    Extension, Json, Router,
};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;
use validator::Validate;

use super::error::ApiError;

#[derive(Debug, Validate, Deserialize)]
struct NewAddress {
    #[validate(length(min = 1, max = 64))]
    building: String,
    #[validate(length(min = 1, max = 64))]
    street: Option<String>,
    #[validate(length(min = 1, max = 64))]
    town_or_city: Option<String>,
    #[validate(length(min = 1, max = 8))]
    postcode: String,
}

async fn add_address(
    db: Extension<PgPool>,
    Json(request): Json<NewAddress>,
    Path(person_uuid): Path<Uuid>,
) -> Result<(), ApiError> {
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
            WHERE p.uuid = $5;
        "#,
        request.building,
        request.street,
        request.town_or_city,
        request.postcode,
        person_uuid,
    )
    .execute(&*db)
    .await?;

    Ok(())
}

async fn remove_address(
    db: Extension<PgPool>,
    Path(address_uuid): Path<Uuid>,
) -> Result<(), ApiError> {
    let mut tx = db.begin().await?;

    sqlx::query!(
        r#"
            UPDATE person SET address = NULL WHERE address = $1;
        "#,
        address_uuid
    )
    .execute(&mut tx)
    .await?;

    sqlx::query!(
        r#"
            DELETE FROM address WHERE uuid = $1;
        "#,
        address_uuid
    )
    .execute(&mut tx)
    .await?;

    tx.commit().await?;

    Ok(())
}

pub fn router() -> Router {
    Router::new()
        .route("/person/:person_uuid/address", post(add_address))
        .route("/address/:address_uuid", delete(remove_address))
}
