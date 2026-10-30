use crate::api::v1::{core, home};
use crate::helper::request::{RequestDataAPIV1, get_request_body, log_request};
use actix_web::{CustomizeResponder, HttpRequest, HttpResponse, get, post, web};
use serde_json::Value;

//   _____     _ _____             _ _
//  |  _  |___|_| __  |___ ___ _ _| | |_
//  |     | . | |    -| -_|_ -| | | |  _|
//  |__|__|  _|_|__|__|___|___|___|_|_|
//        |_|
pub enum ApiResult {
    Json(String),
    JsonWithHeaders {
        body: String,
        headers: Vec<(String, String)>,
    },
    NotFound,
    Unauthorized,
    BadRequest(String),
}

//   _____     _    _____         _
//  |  _  |___|_|  | __  |___ _ _| |_ ___ ___
//  |     | . | |  |    -| . | | |  _| -_|_ -|
//  |__|__|  _|_|  |__|__|___|___|_| |___|___|
//        |_|

pub async fn main_hander(
    request: RequestDataAPIV1,
    _body: Value,
) -> CustomizeResponder<HttpResponse> {
    let response = match request.path.as_str() {
        "/" => home::default(request).await,
        _ => ApiResult::NotFound,
    };
    core::api_response(response)
}

#[post("/{path:.*}")]
pub async fn v1_handlerpost(
    path: web::Path<String>,
    payload: web::Payload,
    req: HttpRequest,
) -> CustomizeResponder<HttpResponse> {
    let request_body = get_request_body(payload).await;
    main_hander(log_request(&path, &req, "POST"), request_body).await
}

#[get("/{path:.*}")]
pub async fn v1_handler(
    path: web::Path<String>,
    req: HttpRequest,
) -> CustomizeResponder<HttpResponse> {
    main_hander(log_request(&path, &req, "GET"), Value::Null).await
}
