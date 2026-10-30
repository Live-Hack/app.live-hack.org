use crate::helper::request::RequestData;
use crate::web::core_utils::custom_type::RouteResult;
use crate::web::core_utils::utils::read_html;
use crate::web::routes::txt;

pub async fn non_logged(request_data: &RequestData) -> RouteResult {
    match request_data.path.as_str() {
        "/" => read_html("html/home.html"),

        "/robots.txt" => txt::robot(),
        "/security.txt" | "/.well-known/security.txt" => txt::security(),

        _ => RouteResult::NotFound,
    }
}
