use axum::Router;
use utoipa::{
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    Modify, OpenApi,
};
use utoipa_swagger_ui::SwaggerUi;

#[derive(OpenApi)]
#[openapi(
    paths(
        super::address::add_address,
        super::address::remove_address,
        super::person::create_person,
        super::person::list_people,
        super::person::get_person,
        super::person::delete_person,
        super::person::update_person,
    ),
    components(schemas(
        super::address::NewAddress,
        super::person::NewPerson,
        super::person::UpdatePerson,
        super::person::Person,
        super::error::ErrorResponse
    )),
    modifiers(&SecurityAddon),
    tags()
)]
struct ApiDoc;
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .build(),
                ),
            );
        }
    }
}

pub fn router() -> Router {
    Router::new()
        .merge(SwaggerUi::new("/swagger-ui").url("/api-doc/openapi.json", ApiDoc::openapi()))
}
