pub enum RouteResult {
    Html(String),
    Text(String),
    Redirect(String),
    NotFound,
}
