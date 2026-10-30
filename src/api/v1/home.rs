use crate::api::v1::main::ApiResult;
use crate::helper::request::RequestDataAPIV1;

pub async fn default(_request: RequestDataAPIV1) -> ApiResult {
    ApiResult::Json(r#"{"message": "Hello, anonymous user !"}"#.to_string())
}
