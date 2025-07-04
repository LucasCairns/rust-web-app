use axum::{extract::Path, routing::get, Extension, Json, Router};
use hyper::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use time::{Date, OffsetDateTime};
use tracing::info;
use utoipa::ToSchema;
use uuid::Uuid;
use validator::{Validate, ValidationError};

use crate::http::error::ErrorResponse;

use super::auth::{ReadUser, WriteUser};
use super::error::ApiError;

#[derive(Debug, Validate, Deserialize, ToSchema)]
pub struct NewPerson {
    #[validate(length(min = 1, max = 64))]
    first_name: String,
    #[validate(length(min = 1, max = 64))]
    family_name: String,
    #[validate(custom(function = "date_not_in_future"))]
    date_of_birth: Date,
}

fn date_not_in_future(date: &Date) -> Result<(), ValidationError> {
    if *date > OffsetDateTime::now_utc().date() {
        return Err(ValidationError::new("date_not_in_future"));
    }

    Ok(())
}

#[derive(Debug, Validate, Deserialize, ToSchema)]
pub struct UpdatePerson {
    #[validate(length(min = 1, max = 64))]
    first_name: Option<String>,
    #[validate(length(min = 1, max = 64))]
    family_name: Option<String>,
    #[validate(custom(function = "date_not_in_future"))]
    date_of_birth: Option<Date>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    id: Uuid,
    first_name: String,
    family_name: String,
    date_of_birth: Date,
    created: OffsetDateTime,
    last_edited: OffsetDateTime,
}

/// Create a new person
///
/// Requires the scope `write`
#[utoipa::path(
    post,
    tag = "person",
    path = "/person",
    request_body = NewPerson,
    responses(
        (status = 201, description = "Person created successfully", body = Person),
        (status = 409, description = "Person already exists", body = ErrorResponse),
    ),
    security(
        ("bearer" = [])
    )
)]
async fn create_person(
    user: WriteUser,
    db: Extension<PgPool>,
    Json(request): Json<NewPerson>,
) -> Result<(StatusCode, Json<Person>), ApiError> {
    request.validate()?;

    let person = sqlx::query_as!(
        Person,
        r#"
            INSERT INTO person (first_name, family_name, date_of_birth)
            VALUES ($1, $2, $3)
            RETURNING uuid AS id, created, last_edited, first_name, family_name, date_of_birth;
        "#,
        request.first_name,
        request.family_name,
        request.date_of_birth
    )
    .fetch_one(&*db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(dbe) if dbe.constraint().is_some() => ApiError::Conflict(format!(
            "Unable to create person due to constraint: {}",
            dbe.constraint().unwrap()
        )),
        _ => ApiError::DatabaseError(e),
    })?;

    info!("Client '{}' created person '{}'", user.username, person.id);

    Ok((StatusCode::CREATED, Json(person)))
}

/// List all people
///
/// Requires the scope `read`
#[utoipa::path(
    get,
    tag = "person",
    path = "/person",
    responses(
        (status = 200, description = "List all people", body = [Person]),
    ),
    security(
        ("bearer" = [])
    )
)]
async fn list_people(user: ReadUser, db: Extension<PgPool>) -> Result<Json<Vec<Person>>, ApiError> {
    let people = sqlx::query_as!(
        Person,
        r#"
            SELECT uuid AS id, created, last_edited, first_name, family_name, date_of_birth FROM person;
        "#
    )
    .fetch_all(&*db)
    .await?;

    info!(
        "Client '{}' retrieved {} person(s)",
        user.username,
        people.len(),
    );

    Ok(Json(people))
}

/// Get a person
///
/// Requires the scope `read`
#[utoipa::path(
    get,
    tag = "person",
    path = "/person/{person_uuid}",
    params(
        ("person_uuid" = Uuid, Path, description = "The UUID of the person")
    ),
    responses(
        (status = 200, description = "The person matching the given UUID", body = Person),
        (status = 404, description = "Person not found", body = ErrorResponse),
    ),
    security(
        ("bearer" = [])
    )
)]
async fn get_person(
    user: ReadUser,
    db: Extension<PgPool>,
    Path(person_uuid): Path<Uuid>,
) -> Result<Json<Person>, ApiError> {
    let person = sqlx::query_as!(
        Person,
        r#"
            SELECT uuid AS id, created, last_edited, first_name, family_name, date_of_birth FROM person WHERE uuid = $1;
        "#,
        person_uuid
    )
    .fetch_one(&*db)
    .await
    .map_err(|e| match e {
        sqlx::Error::RowNotFound => ApiError::NotFound(format!("Person not found for the UUID: {person_uuid}")),
        _ => ApiError::DatabaseError(e),
    })?;

    info!(
        "Client '{}' retrieved person '{}'",
        person.id, user.username
    );

    Ok(Json(person))
}

/// Delete a person
///
/// Requires the scope `write`
#[utoipa::path(
    delete,
    tag = "person",
    path = "/person/{person_uuid}",
    params(
        ("person_uuid" = Uuid, Path, description = "The UUID of the person")
    ),
    responses(
        (status = 200, description = "Person deleted successfully"),
        (status = 404, description = "Person not found", body = ErrorResponse),
    ),
    security(
        ("bearer" = [])
    )
)]
async fn delete_person(
    user: WriteUser,
    db: Extension<PgPool>,
    Path(person_uuid): Path<Uuid>,
) -> Result<(), ApiError> {
    sqlx::query!(
        r#"
            DELETE FROM person WHERE uuid = $1
            RETURNING uuid as id;
        "#,
        person_uuid
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
        "Client '{}' deleted person '{}'",
        user.username, person_uuid
    );

    Ok(())
}

/// Update a person
///
/// Requires the scope `write`
#[utoipa::path(
    put,
    tag = "person",
    path = "/person/{person_uuid}",
    params(
        ("person_uuid" = Uuid, Path, description = "The UUID of the person")
    ),
    request_body = UpdatePerson,
    responses(
        (status = 200, description = "Person updated successfully"),
        (status = 404, description = "Person not found", body = ErrorResponse),
    ),
    security(
        ("bearer" = [])
    )
)]
async fn update_person(
    user: WriteUser,
    db: Extension<PgPool>,
    Path(person_uuid): Path<Uuid>,
    Json(request): Json<UpdatePerson>,
) -> Result<Json<Person>, ApiError> {
    let existing = sqlx::query_as!(
        Person,
        r#"
            SELECT uuid AS id, created, last_edited, first_name, family_name, date_of_birth FROM person WHERE uuid = $1;
        "#,
        person_uuid
    )
    .fetch_one(&*db)
    .await
    .map_err(|e| match e {
        sqlx::Error::RowNotFound => ApiError::NotFound(format!("Person not found for the UUID: {person_uuid}")),
        _ => ApiError::DatabaseError(e),
    })?;

    let updated_person = sqlx::query_as!(
        Person,
        r#"
            UPDATE person SET first_name = $1, family_name = $2, date_of_birth = $3, last_edited = now()
            WHERE uuid = $4
            RETURNING uuid AS id, created, last_edited, first_name, family_name, date_of_birth;
        "#,
        request.first_name.unwrap_or(existing.first_name),
        request.family_name.unwrap_or(existing.family_name),
        request.date_of_birth.unwrap_or(existing.date_of_birth),
        person_uuid
    )
    .fetch_one(&*db)
    .await?;

    info!(
        "Client '{}' updated person '{}'",
        user.username, updated_person.id
    );

    Ok(Json(updated_person))
}

pub fn router() -> Router {
    Router::new()
        .route("/person", get(list_people).post(create_person))
        .route(
            "/person/{person_uuid}",
            get(get_person).put(update_person).delete(delete_person),
        )
}

#[cfg(test)]
mod tests {
    use time::macros::date;
    use validator::Validate;

    use super::NewPerson;

    #[test]
    fn new_person_is_valid_when_dob_is_in_the_future() {
        let new_person = NewPerson {
            first_name: "John".to_owned(),
            family_name: "Doe".to_owned(),
            date_of_birth: date!(1900 - 1 - 1),
        };

        assert!(new_person.validate().is_ok(), "Should be a valid person");
    }

    #[test]
    fn new_person_is_invalid_when_dob_is_in_the_future() {
        let new_person = NewPerson {
            first_name: "John".to_owned(),
            family_name: "Doe".to_owned(),
            date_of_birth: date!(2050 - 1 - 1),
        };

        assert!(
            new_person.validate().is_err(),
            "Should return a validation error"
        );
    }
}
