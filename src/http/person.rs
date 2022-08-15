use axum::{extract::Path, routing::get, Extension, Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use time::{Date, OffsetDateTime, PrimitiveDateTime};
use uuid::Uuid;
use validator::{Validate, ValidationError};

use super::error::ApiError;

#[derive(Debug, Validate, Deserialize)]
struct NewPerson {
    #[validate(length(min = 1, max = 64))]
    first_name: String,
    #[validate(length(min = 1, max = 64))]
    family_name: String,
    #[validate(custom = "date_not_in_future")]
    date_of_birth: Date,
}

fn date_not_in_future(date: &Date) -> Result<(), ValidationError> {
    if *date > OffsetDateTime::now_utc().date() {
        return Err(ValidationError::new("date_not_in_future"));
    }

    Ok(())
}

#[derive(Debug, Validate, Deserialize)]
struct UpdatePerson {
    #[validate(length(min = 1, max = 64))]
    first_name: Option<String>,
    #[validate(length(min = 1, max = 64))]
    family_name: Option<String>,
    #[validate(custom = "date_not_in_future")]
    date_of_birth: Option<Date>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Person {
    id: Uuid,
    first_name: String,
    family_name: String,
    date_of_birth: Date,
    created: PrimitiveDateTime,
    last_edited: PrimitiveDateTime,
}

// #[axum_macros::debug_handler] // Useful for debugging trait bound errors
async fn create_person(
    db: Extension<PgPool>,
    Json(request): Json<NewPerson>,
) -> Result<Json<Person>, ApiError> {
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

    Ok(Json(person))
}

async fn list_people(db: Extension<PgPool>) -> Result<Json<Vec<Person>>, ApiError> {
    let people = sqlx::query_as!(
        Person,
        r#"
            SELECT uuid AS id, created, last_edited, first_name, family_name, date_of_birth FROM person;
        "#
    )
    .fetch_all(&*db)
    .await?;

    Ok(Json(people))
}

async fn get_person(
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

    Ok(Json(person))
}

async fn delete_person(
    db: Extension<PgPool>,
    Path(person_uuid): Path<Uuid>,
) -> Result<(), ApiError> {
    sqlx::query!(
        r#"
            DELETE FROM person WHERE uuid = $1;
        "#,
        person_uuid
    )
    .execute(&*db)
    .await
    .map_err(|e| match e {
        sqlx::Error::RowNotFound => {
            ApiError::NotFound(format!("Person not found for the UUID: {person_uuid}"))
        }
        _ => ApiError::DatabaseError(e),
    })?;

    Ok(())
}

async fn update_person(
    db: Extension<PgPool>,
    Json(request): Json<UpdatePerson>,
    Path(person_uuid): Path<Uuid>,
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
            UPDATE person SET first_name = $1, family_name = $2, date_of_birth = $3
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

    Ok(Json(updated_person))
}

pub fn router() -> Router {
    Router::new()
        .route("/person", get(list_people).post(create_person))
        .route(
            "/person/:person_uuid",
            get(get_person).put(update_person).delete(delete_person),
        )
}

#[cfg(test)]
mod tests {
    use time::macros::date;
    use validator::Validate;

    use super::NewPerson;

    #[test]
    fn new_person() {
        let new_person = NewPerson {
            first_name: "John".to_owned(),
            family_name: "Doe".to_owned(),
            date_of_birth: date!(1900 - 1 - 1),
        };

        assert!(new_person.validate().is_ok(), "Should be a valid person");
    }

    #[test]
    fn new_person_from_the_future() {
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
