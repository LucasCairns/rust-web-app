use axum::{
    extract::Path,
    routing::{delete, post},
    Extension, Json, Router,
};
use hyper::StatusCode;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;
use sqlx::PgPool;
use tracing::info;
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use crate::http::{auth::Scoped, error::ErrorResponse};

use super::{auth::WriteUser, error::ApiError};

static UK_POSTCODE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?xi)^
           [A-PR-UWYZ]                               # first area letter
           (?:[A-HK-Y]?                               # optional 2nd area letter
              \d{1,2}                                 # district digits
              [A-Z]?                                  # optional letter suffix in outward code
           |\d                                        # or single digit for short forms
              [0-9A-Z]?                               # optional extra digit/letter for BFPO etc.
           )
           \s*                                        # optional space before inward code
           \d[A-Z]{2}$                                # inward: digit + 2 letters
        ",
    )
    .unwrap()
});

#[derive(Debug, Validate, Deserialize, ToSchema)]
pub struct NewAddress {
    #[validate(length(min = 1, max = 64))]
    building: String,
    #[validate(length(min = 1, max = 64))]
    street: Option<String>,
    #[validate(length(min = 1, max = 64))]
    town_or_city: Option<String>,
    #[validate(regex(path = *UK_POSTCODE))]
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
    user: Scoped<WriteUser>,
    Extension(pool): Extension<PgPool>,
    Path(person_uuid): Path<Uuid>,
    Json(request): Json<NewAddress>,
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
    .fetch_one(&pool)
    .await?;

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
    user: Scoped<WriteUser>,
    Extension(pool): Extension<PgPool>,
    Path(address_uuid): Path<Uuid>,
) -> Result<(), ApiError> {
    let mut tx = pool.begin().await?;

    sqlx::query!(
        r#"
            UPDATE person SET address = NULL WHERE address = $1
            RETURNING id;
        "#,
        address_uuid
    )
    .fetch_all(&mut *tx)
    .await?;

    sqlx::query!(
        r#"
            DELETE FROM address WHERE uuid = $1
            RETURNING id;
        "#,
        address_uuid
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    info!(
        "Client '{}' deleted the address '{}'",
        user.username, address_uuid
    );

    Ok(())
}

pub fn router() -> Router {
    Router::new()
        .route("/person/{person_uuid}/address", post(add_address))
        .route("/address/{address_uuid}", delete(remove_address))
}

#[cfg(test)]
mod tests {
    use super::NewAddress;
    use validator::Validate;

    #[test]
    fn address_is_valid_with_all_fields() {
        let address = NewAddress {
            building: "42 Example Street".to_owned(),
            street: Some("High Road".to_owned()),
            town_or_city: Some("London".to_owned()),
            postcode: "B33 8TH".to_owned(),
        };

        assert!(address.validate().is_ok());
    }

    #[test]
    fn address_is_valid_with_required_fields_only() {
        let address = NewAddress {
            building: "1 Main St".to_owned(),
            street: None,
            town_or_city: None,
            postcode: "M1 1AE".to_owned(),
        };

        assert!(address.validate().is_ok());
    }

    #[test]
    fn address_is_invalid_when_building_is_empty() {
        let address = NewAddress {
            building: "".to_owned(),
            street: None,
            town_or_city: None,
            postcode: "CR2 6XH".to_owned(),
        };

        assert!(address.validate().is_err());
    }

    #[test]
    fn address_is_invalid_when_building_exceeds_max_length() {
        let address = NewAddress {
            building: "a".repeat(65),
            street: None,
            town_or_city: None,
            postcode: "CR2 6XH".to_owned(),
        };

        assert!(address.validate().is_err());
    }

    #[test]
    fn address_is_valid_when_building_is_exactly_max_length() {
        let address = NewAddress {
            building: "a".repeat(64),
            street: None,
            town_or_city: None,
            postcode: "CR2 6XH".to_owned(),
        };

        assert!(address.validate().is_ok());
    }

    #[test]
    fn address_is_invalid_when_postcode_is_empty() {
        let address = NewAddress {
            building: "42 Example Street".to_owned(),
            street: None,
            town_or_city: None,
            postcode: "".to_owned(),
        };

        assert!(address.validate().is_err());
    }

    #[test]
    fn address_is_invalid_when_postcode_is_not_a_uk_format() {
        let address = NewAddress {
            building: "42 Example Street".to_owned(),
            street: None,
            town_or_city: None,
            postcode: "12345".to_owned(),
        };

        assert!(address.validate().is_err());
    }

    #[test]
    fn address_is_invalid_when_postcode_is_us_zip() {
        let address = NewAddress {
            building: "42 Example Street".to_owned(),
            street: None,
            town_or_city: None,
            postcode: "90210".to_owned(),
        };

        assert!(address.validate().is_err());
    }

    #[test]
    fn address_is_valid_when_postcode_is_lowercase() {
        let address = NewAddress {
            building: "Aboyne House".to_owned(),
            street: None,
            town_or_city: Some("Aberdeenshire".to_owned()),
            postcode: "ab34 5jp".to_owned(),
        };

        assert!(address.validate().is_ok());
    }

    #[test]
    fn address_is_valid_when_postcode_has_two_area_letters() {
        let address = NewAddress {
            building: "10 Downing Street".to_owned(),
            street: None,
            town_or_city: Some("London".to_owned()),
            postcode: "SW1A 2AA".to_owned(),
        };

        assert!(address.validate().is_ok());
    }

    #[test]
    fn address_is_valid_when_postcode_is_short() {
        let address = NewAddress {
            building: "City Building".to_owned(),
            street: None,
            town_or_city: Some("London".to_owned()),
            postcode: "EC1A 1BB".to_owned(),
        };

        assert!(address.validate().is_ok());
    }

    #[test]
    fn address_is_valid_when_postcode_has_no_space() {
        let address = NewAddress {
            building: "42 Example Street".to_owned(),
            street: None,
            town_or_city: None,
            postcode: "B338TH".to_owned(),
        };

        assert!(address.validate().is_ok());
    }

    #[test]
    fn address_is_invalid_when_postcode_has_invalid_first_letter() {
        let address = NewAddress {
            building: "42 Example Street".to_owned(),
            street: None,
            town_or_city: None,
            postcode: "Q1 1AA".to_owned(),
        };

        assert!(address.validate().is_err());
    }

    #[test]
    fn address_is_valid_when_inward_code_ends_with_x() {
        let address = NewAddress {
            building: "42 Example Street".to_owned(),
            street: None,
            town_or_city: None,
            postcode: "AB1 1XX".to_owned(),
        };

        assert!(address.validate().is_ok());
    }

    #[test]
    fn address_is_invalid_when_street_exceeds_max_length() {
        let address = NewAddress {
            building: "42 Example Street".to_owned(),
            street: Some("a".repeat(65)),
            town_or_city: None,
            postcode: "CR2 6XH".to_owned(),
        };

        assert!(address.validate().is_err());
    }

    #[test]
    fn address_is_invalid_when_town_or_city_exceeds_max_length() {
        let address = NewAddress {
            building: "42 Example Street".to_owned(),
            street: None,
            town_or_city: Some("a".repeat(65)),
            postcode: "CR2 6XH".to_owned(),
        };

        assert!(address.validate().is_err());
    }
}
