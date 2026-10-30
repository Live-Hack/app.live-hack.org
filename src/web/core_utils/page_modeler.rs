use crate::helper::find_insert::find_insert;
use crate::helper::replace_in_body::replace_html;
use crate::helper::request::RequestData;
use std::fs;

/// Fills `{{inject_x}}` with `html/inject/x.html`, then the per-request blocks.
pub fn page_modeler(_request_data: &RequestData, mut response: String) -> String {
    for name in find_insert(&response) {
        if let Ok(content) = fs::read_to_string(format!("html/inject/{}.html", name)) {
            response = replace_html(&response, &format!("inject_{}", name), &content);
        }
    }

    // ponytail: user menu / admin nav come back in phase 01 when sessions exist
    response = replace_html(&response, "login_space", "");
    response
}
