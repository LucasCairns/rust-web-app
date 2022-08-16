use axum::Router;
use utoipa::OpenApi;
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
    modifiers(),
    tags()
)]
struct ApiDoc;

pub fn router() -> Router {
    Router::new()
        .merge(SwaggerUi::new("/swagger-ui/*tail").url("/api-doc/openapi.json", ApiDoc::openapi()))
}
