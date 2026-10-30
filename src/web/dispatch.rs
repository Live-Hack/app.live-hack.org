use crate::helper::replace_in_body::escape_html;
use crate::helper::request::{RequestData, log_request};
use crate::web::core_routes::nonlogged::non_logged;
use crate::web::core_utils::cached_page::{HTML_404, HTML_REDIRECT};
use crate::web::core_utils::{custom_type::RouteResult, page_modeler::page_modeler};
use actix_web::{HttpRequest, HttpResponse, Responder, get, web};

#[get("/{path:.*}")]
#[tracing::instrument(level = "info", skip(path, req))]
pub async fn dispatch(path: web::Path<String>, req: HttpRequest) -> impl Responder {
    let request = log_request(&path, &req, "GET");
    // ponytail: logged routes plug in here in phase 01 (auth)
    let content = non_logged(&request).await;
    build_response(content, &request)
}

fn build_response(result: RouteResult, request: &RequestData) -> HttpResponse {
    match result {
        RouteResult::Html(body) => HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body(page_modeler(request, body)),
        RouteResult::Text(body) => HttpResponse::Ok()
            .content_type("text/plain; charset=utf-8")
            .body(body),
        RouteResult::Redirect(target) => {
            let body = HTML_REDIRECT.replace("{{path}}", &escape_html(&target));
            HttpResponse::SeeOther()
                .content_type("text/html; charset=utf-8")
                .append_header(("Location", target))
                .body(body)
        }
        RouteResult::NotFound => HttpResponse::NotFound()
            .content_type("text/html; charset=utf-8")
            .body(page_modeler(request, HTML_404.clone())),
    }
}
