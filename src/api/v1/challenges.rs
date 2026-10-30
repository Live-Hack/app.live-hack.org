use crate::api::v1::core::{ApiResult, bad, err, not_found, ok, require_user, too_many};
use crate::helper::ratelimit;
use crate::helper::request::{Ctx, field_raw};
use crate::models::chall::{self, Filter};
use crate::models::solve::{self, Scope, check_flag};
use crate::models::user::Role;
use actix_web::http::StatusCode;
use serde_json::{Value, json};

pub async fn list(ctx: &Ctx) -> ApiResult {
    let u = require_user(ctx)?;
    let f = Filter {
        q: ctx.param("q").to_string(),
        category: ctx.param("category").to_string(),
        difficulty: ctx.param("difficulty").to_string(),
        state: ctx.param("state").to_string(),
        sort: ctx.param("sort").to_string(),
        page: ctx.param("page").parse().unwrap_or(1).max(1),
        per_page: ctx.param("per_page").parse().unwrap_or(50),
    };
    let (cards, total) = chall::list_practice(ctx.tenant_uuid(), &u.uuid, &f).await?;
    ok(json!({ "challenges": cards, "total": total, "page": f.page }))
}

/// Which scoreboard a solve on this challenge counts for, for this user.
/// Practice for now; events plug in here.
async fn resolve_scope(
    _ctx: &Ctx,
    _chall: &chall::Chall,
) -> Result<Option<Scope>, crate::api::v1::core::ApiError> {
    Ok(Some(Scope::default()))
}

pub async fn submit(ctx: &Ctx, uuid: &str, body: &Value) -> ApiResult {
    let u = require_user(ctx)?;
    let t = ctx.tenant_uuid();
    let value = field_raw(body, "flag");
    if value.trim().is_empty() {
        return Err(bad("Enter a flag."));
    }
    if value.len() > 1024 {
        return Err(bad("This flag is too long."));
    }
    let Some(c) = chall::get(t, uuid).await? else {
        return Err(not_found());
    };

    let can_test = ctx.has_role(Role::Admin) || chall::is_author(&c.uuid, &u.uuid).await?;
    let live = c.status == "published";
    if !live && !can_test {
        return Err(not_found());
    }
    if !ratelimit::allow(&format!("flag:{}:{}", u.uuid, c.uuid), 10, 60).await?
        || !ratelimit::allow(&format!("flag:{}", u.uuid), 60, 60).await?
    {
        return Err(too_many());
    }

    // Organizers can try the flag of a draft without it counting anywhere.
    if !live {
        let correct = check_flag(&c.flag_type, &c.flag, value);
        return ok(
            json!({ "status": if correct { "correct" } else { "wrong" }, "test": true,
            "message": if correct { "Correct flag (test mode, not recorded)." } else { "Wrong flag (test mode)." } }),
        );
    }

    let Some(scope) = resolve_scope(ctx, &c).await? else {
        return Err(err(
            StatusCode::FORBIDDEN,
            "This challenge is not open for submissions right now.",
        ));
    };
    if chall::solved_at(&c.uuid, scope.event_key(), scope.scope_key(&u.uuid))
        .await?
        .is_some()
    {
        return ok(json!({ "status": "already_solved", "message": "Already solved." }));
    }

    let expected = c.flag.clone();
    let correct = check_flag(&c.flag_type, &expected, value);
    solve::record_submission(t, &c.uuid, &u.uuid, &scope, value, correct, &ctx.ip).await?;
    if !correct {
        return ok(json!({ "status": "wrong", "message": "Wrong flag. Try again." }));
    }
    if !solve::record_solve(t, &c.uuid, &u.uuid, &scope).await? {
        return ok(json!({ "status": "already_solved", "message": "Already solved." }));
    }
    let first_blood = solve::solve_count(&c.uuid, scope.event_key()).await? == 1;
    ok(json!({
        "status": "correct",
        "points": c.points,
        "first_blood": first_blood,
        "message": if first_blood { format!("First blood! +{} points", c.points) } else { format!("Correct! +{} points", c.points) },
    }))
}
