use crate::helper::request::{Ctx, build};
use crate::helper::template::{Vars, escape_html};
use crate::helper::trace::trace_error;
use crate::models::user;
use crate::web::page::{PageResult, RouteResult, page_html};
use crate::web::routes::{account, auth, challenges, home, txt};
use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse, get, web};

async fn route(ctx: &Ctx, segs: &[&str]) -> PageResult {
    // An empty platform only shows the setup page.
    if !user::setup_done(ctx.tenant_uuid()).await? {
        return match segs {
            ["setup"] => auth::setup_page(ctx),
            ["robots.txt"] => Ok(txt::robot()),
            _ => Ok(RouteResult::Redirect("/setup".into())),
        };
    }
    match segs {
        [] => home::home(ctx),
        ["setup"] => Ok(RouteResult::Redirect("/".into())),
        ["auth", "login"] => auth::login_page(ctx),
        ["auth", "register"] => auth::register_page(ctx).await,
        ["auth", "reset"] => auth::reset_page(ctx).await,
        ["account", "settings"] => account::settings_page(ctx),
        ["challenges"] => challenges::list(ctx).await,
        ["challenges", slug] => challenges::detail(ctx, slug).await,
        ["challenges", slug, "files", file, _name] => challenges::file(ctx, slug, file).await,
        ["robots.txt"] => Ok(txt::robot()),
        ["security.txt"] | [".well-known", "security.txt"] => Ok(txt::security(ctx)),
        _ => Ok(RouteResult::NotFound),
    }
}

#[get("/{path:.*}")]
pub async fn dispatch(path: web::Path<String>, req: HttpRequest) -> HttpResponse {
    let Some(ctx) = build(&req, &path, "GET").await else {
        return HttpResponse::NotFound()
            .content_type("text/plain")
            .body("Unknown host");
    };
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    match route(&ctx, &segs).await {
        Ok(result) => build_response(&ctx, &req, result),
        Err(e) => {
            trace_error(format!("page {}: {e}", ctx.path));
            error_page(
                &ctx,
                StatusCode::INTERNAL_SERVER_ERROR,
                "Something went wrong",
                "The server hit an error. Try again in a moment.",
            )
        }
    }
}

fn error_page(ctx: &Ctx, status: StatusCode, title: &str, text: &str) -> HttpResponse {
    let mut v = Vars::new();
    v.set("code", status.as_u16().to_string())
        .set("message", text);
    html(status, page_html(ctx, "error.html", title, v))
}

fn html(status: StatusCode, body: String) -> HttpResponse {
    HttpResponse::build(status)
        .content_type("text/html; charset=utf-8")
        .insert_header(("Cache-Control", "no-store"))
        .insert_header(("X-Content-Type-Options", "nosniff"))
        .insert_header(("Referrer-Policy", "same-origin"))
        .insert_header(("X-Frame-Options", "DENY"))
        .body(body)
}

fn build_response(ctx: &Ctx, req: &HttpRequest, result: RouteResult) -> HttpResponse {
    match result {
        RouteResult::Html(body) => html(StatusCode::OK, body),
        RouteResult::Text(body) => HttpResponse::Ok()
            .content_type("text/plain; charset=utf-8")
            .body(body),
        RouteResult::Redirect(target) => HttpResponse::SeeOther()
            .insert_header(("Location", target.clone()))
            .content_type("text/html; charset=utf-8")
            .body(format!("<a href=\"{0}\">{0}</a>", escape_html(&target))),
        RouteResult::File(path, name) => match actix_files::NamedFile::open(&path) {
            Ok(f) => f
                .set_content_type("application/octet-stream".parse().unwrap())
                .set_content_disposition(actix_web::http::header::ContentDisposition {
                    disposition: actix_web::http::header::DispositionType::Attachment,
                    parameters: vec![actix_web::http::header::DispositionParam::Filename(name)],
                })
                .into_response(req),
            Err(_) => error_page(
                ctx,
                StatusCode::NOT_FOUND,
                "Page not found",
                "This file is missing.",
            ),
        },
        RouteResult::NotFound => error_page(
            ctx,
            StatusCode::NOT_FOUND,
            "Page not found",
            "The page you are looking for is not available.",
        ),
    }
}
