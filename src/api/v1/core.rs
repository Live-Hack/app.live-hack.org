use crate::api::v1::main::ApiResult;
use actix_web::{CustomizeResponder, HttpResponse, Responder};
use serde_json::json;

pub fn api_response(result: ApiResult) -> CustomizeResponder<HttpResponse> {
    match result {
        ApiResult::Json(data) => HttpResponse::Ok()
            .content_type("application/json")
            .body(data)
            .customize(),

        ApiResult::JsonWithHeaders { body, headers } => {
            let mut response = HttpResponse::Ok();
            response.content_type("application/json");
            for (k, v) in headers {
                response.append_header((k.as_str(), v.as_str()));
            }
            response.body(body).customize()
        }

        ApiResult::NotFound => HttpResponse::NotFound()
            .content_type("application/json")
            .body(r#"{"error":"Not found"}"#)
            .customize(),

        ApiResult::Unauthorized => HttpResponse::Unauthorized()
            .content_type("application/json")
            .body(r#"{"error":"Unauthorized"}"#)
            .customize(),

        ApiResult::BadRequest(message) => HttpResponse::BadRequest()
            .content_type("application/json")
            .body(json!({"error": message}).to_string())
            .customize(),
    }
}
