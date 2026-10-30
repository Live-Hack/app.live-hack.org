#!/usr/bin/env python3
"""End-to-end suite: drives a fresh instance through HTTP like a browser and an API client.
Usage: e2e.py BASE_URL COMPOSE_PROJECT. Stdlib only. Every phase adds its section; all run each time."""
import http.cookiejar
import json
import re
import subprocess
import sys
import time
import urllib.error
import urllib.request

BASE = sys.argv[1].rstrip('/')
PROJECT = sys.argv[2]
PASSWORD = 'correct-horse-battery'
results = {'pass': 0, 'fail': 0}


def check(cond, label):
    if cond:
        results['pass'] += 1
    else:
        results['fail'] += 1
        print(f'  FAIL  {label}')
    return cond


def section(name):
    print(f'== {name}')


def sql(query):
    out = subprocess.run(
        ['docker', 'compose', '-p', PROJECT, 'exec', '-T', 'db', 'mysql', '-N', '-ulivehack', '-plivehack', 'livehack', '-e', query],
        capture_output=True, text=True, check=True)
    return out.stdout.strip()


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *a, **k):
        return None


class Client:
    def __init__(self, host=None):
        self.jar = http.cookiejar.CookieJar()
        self.opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(self.jar), NoRedirect)
        self.host = host

    def _do(self, req):
        if self.host:
            req.add_header('Host', self.host)
        try:
            r = self.opener.open(req, timeout=20)
            return r.status, {k.lower(): v for k, v in r.headers.items()}, r.read()
        except urllib.error.HTTPError as e:
            return e.code, {k.lower(): v for k, v in e.headers.items()}, e.read()

    def get(self, path):
        s, h, b = self._do(urllib.request.Request(BASE + path))
        return s, h, b.decode('utf-8', 'replace')

    def api(self, path, body=None, xrw=True, origin=None):
        req = urllib.request.Request(BASE + '/api/v1' + path)
        req.add_header('Accept', 'application/json')
        if body is not None:
            req.data = json.dumps(body).encode()
            req.add_header('Content-Type', 'application/json')
            req.method = 'POST'
        if xrw:
            req.add_header('X-Requested-With', 'fetch')
        if origin:
            req.add_header('Origin', origin)
        s, h, b = self._do(req)
        try:
            data = json.loads(b or b'{}')
        except ValueError:
            data = {'_raw': b[:200]}
        return s, data, h

    def cookie(self, name='lh_session'):
        return next((c.value for c in self.jar if c.name == name), None)


# ---------------------------------------------------------------- 00 foundations
def test_foundations():
    section('00 foundations')
    c = Client()
    s, h, b = c.get('/healthz')
    check(s == 200 and b == 'ok', 'healthz ok')
    s, h, _ = c.get('/assets/css/main.css?v=1')
    check(s == 200 and 'immutable' in h.get('cache-control', ''), 'versioned asset is immutable')
    s, h, _ = c.get('/assets/js/app.js')
    check(s == 200 and 'javascript' in h.get('content-type', ''), 'app.js served')
    s, _, _ = c.get('/assets/../Cargo.toml')
    check(s == 404, 'asset traversal blocked')
    s, _, _ = c.get('/assets/nope.css')
    check(s == 404, 'missing asset 404')
    s, _, b = c.get('/robots.txt')
    check(s == 200 and 'Disallow' in b, 'robots.txt')
    s, h, _ = c.get('/api/v1/')
    check(s == 200, 'api root')


# ---------------------------------------------------------------- 01 auth
def test_auth():
    section('01 auth & accounts')
    admin = Client()
    s, h, _ = admin.get('/')
    check(s == 303 and h.get('location') == '/setup', 'empty platform redirects to /setup')
    s, _, b = admin.get('/setup')
    check(s == 200 and 'data-api="/api/v1/setup"' in b, 'setup page renders')
    s, d, _ = admin.api('/me')
    check(s == 200 and d.get('user') is None, 'me is anonymous')

    s, d, _ = admin.api('/setup', {'username': 'admin', 'password': PASSWORD}, xrw=False)
    check(s == 403, 'POST without X-Requested-With refused (CSRF)')
    s, d, _ = admin.api('/setup', {'username': 'admin', 'password': PASSWORD}, origin='http://evil.example')
    check(s == 403, 'POST from foreign Origin refused (CSRF)')
    s, d, _ = admin.api('/setup', {'username': 'admin', 'password': 'short'})
    check(s == 400, 'setup rejects short password')
    s, d, _ = admin.api('/setup', {'username': 'admin', 'password': PASSWORD, 'site_name': 'Test CTF'})
    check(s == 200 and d['user']['role'] == 'admin', 'setup creates admin')
    check(admin.cookie() and len(admin.cookie()) == 64, 'session cookie set')
    s, d, _ = Client().api('/setup', {'username': 'other', 'password': PASSWORD})
    check(s == 409, 'second setup refused')
    s, h, _ = admin.get('/setup')
    check(s == 303 and h.get('location') == '/', 'setup page closed after setup')
    s, _, b = admin.get('/')
    check(s == 200 and 'Test CTF' in b and '>admin<' in b, 'home shows site name and user menu')
    check('href="/admin"' in b, 'admin link in menu for admin')

    # XSS: settings value is escaped in pages
    s, d, _ = admin.api('/admin/settings', {'site_name': '<script>alert(1)</script>'})
    check(s == 200, 'admin updates settings')
    s, _, b = admin.get('/')
    check('<script>alert(1)</script>' not in b and '&lt;script&gt;alert(1)&lt;/script&gt;' in b, 'site name is HTML-escaped')
    admin.api('/admin/settings', {'site_name': 'Test CTF'})
    s, d, _ = admin.api('/admin/settings', {'registration': 'sometimes'})
    check(s == 400, 'invalid setting value refused')

    # registration (open)
    p1 = Client()
    s, _, b = p1.get('/auth/register')
    check(s == 200 and 'data-api="/api/v1/auth/register"' in b, 'register page (open)')
    s, d, _ = p1.api('/auth/register', {'username': '<script>', 'password': PASSWORD})
    check(s == 400, 'invalid username refused')
    s, d, _ = p1.api('/auth/register', {'username': 'player1', 'email': 'p1@example.com', 'password': PASSWORD})
    check(s == 200 and d['user']['role'] == 'player', 'player registers')
    s, d, _ = Client().api('/auth/register', {'username': 'PLAYER1', 'password': PASSWORD})
    check(s == 409, 'username unique, case-insensitive')
    s, d, _ = Client().api('/auth/register', {'username': 'player9', 'email': 'P1@example.com', 'password': PASSWORD})
    check(s == 409, 'email unique')
    s, d, _ = p1.api('/admin/settings')
    check(s == 403, 'player cannot read admin settings')
    s, _, b = p1.get('/')
    check('href="/admin"' not in b, 'no admin link for player')

    # login / logout
    s, d, h = p1.api('/auth/logout', {})
    check(s == 200 and 'Max-Age=0' in h.get('set-cookie', ''), 'logout clears cookie')
    s, d, _ = p1.api('/me')
    check(d.get('user') is None, 'logged out')
    s, d, _ = p1.api('/auth/login', {'login': 'player1', 'password': 'wrong-password'})
    check(s == 401, 'wrong password refused')
    s, d, _ = p1.api('/auth/login', {'login': 'nobody', 'password': 'wrong-password'})
    check(s == 401 and d['error'] == 'Wrong username or password.', 'unknown user gets same message')
    s, d, _ = p1.api('/auth/login', {'login': 'p1@example.com', 'password': PASSWORD})
    check(s == 200, 'login by email')
    s, h, _ = p1.get('/auth/login?next=/account/settings')
    check(s == 303 and h.get('location') == '/account/settings', 'login page redirects when signed in')
    s, h, _ = Client().get('/auth/login?next=//evil.com')
    check(s == 200, 'login page renders')
    s, _, b = Client().get('/auth/login?next=//evil.com')
    check('data-redirect="/"' in b, 'open redirect neutralised')
    s, h, _ = Client().get('/account/settings')
    check(s == 303 and h['location'] == '/auth/login?next=/account/settings', 'settings needs login')
    s, _, b = p1.get('/account/settings')
    check(s == 200 and 'player1' in b, 'settings page')

    # invite-only registration
    admin.api('/admin/settings', {'registration': 'invite'})
    s, d, _ = Client().api('/auth/register', {'username': 'player2', 'password': PASSWORD})
    check(s == 403, 'invite mode: no invite refused')
    s, _, b = Client().get('/auth/register')
    check('invitation link' in b, 'invite mode: register page explains')
    s, d, _ = p1.api('/admin/invites', {'uses': 1})
    check(s == 403, 'player cannot create invites')
    s, d, _ = admin.api('/admin/invites', {'uses': 1, 'hours': 1})
    check(s == 201 and '/auth/register?invite=' in d.get('url', ''), 'admin creates invite')
    invite = d['path'].split('invite=')[1]
    s, _, b = Client().get(d['path'])
    check(s == 200 and invite in b, 'invite link opens register form')
    p2 = Client()
    s, d, _ = p2.api('/auth/register', {'username': 'player1', 'password': PASSWORD, 'invite': invite})
    check(s == 409, 'taken username with invite refused')
    s, d, _ = p2.api('/auth/register', {'username': 'player2', 'password': PASSWORD, 'invite': invite})
    check(s == 200, 'register with invite (use refunded after conflict)')
    s, d, _ = Client().api('/auth/register', {'username': 'player3', 'password': PASSWORD, 'invite': invite})
    check(s == 403, 'invite single use')
    s, _, b = Client().get('/auth/register?invite=' + invite)
    check('invalid or expired' in b, 'used invite page explains')
    admin.api('/admin/settings', {'registration': 'closed'})
    s, _, b = Client().get('/auth/register')
    check('closed' in b, 'closed mode page')
    s, _, b = Client().get('/auth/login')
    check('/auth/register' not in b, 'no sign-up link when closed')
    admin.api('/admin/settings', {'registration': 'open'})

    # admin reset link
    uuid = sql("SELECT uuid FROM users WHERE username='player2'")
    s, d, _ = admin.api(f'/admin/users/{uuid}/reset-link', {})
    check(s == 201 and '/auth/reset?token=' in d['url'], 'admin creates reset link')
    reset = d['path'].split('token=')[1]
    s, _, b = Client().get(d['path'])
    check(s == 200 and 'data-api="/api/v1/auth/reset"' in b, 'reset page')
    s, d, _ = Client().api('/auth/reset', {'token': reset, 'password': 'new-password-123'})
    check(s == 200, 'password reset')
    s, d, _ = p2.api('/me')
    check(d.get('user') is None, 'reset signs out existing sessions')
    s, d, _ = Client().api('/auth/reset', {'token': reset, 'password': 'new-password-456'})
    check(s == 403, 'reset link single use')
    s, d, _ = p2.api('/auth/login', {'login': 'player2', 'password': 'new-password-123'})
    check(s == 200, 'login with new password')
    s, _, b = Client().get('/auth/reset?token=deadbeef')
    check('invalid or expired' in b, 'bad reset token page')

    # password change revokes other sessions
    other = Client()
    other.api('/auth/login', {'login': 'player2', 'password': 'new-password-123'})
    s, d, _ = p2.api('/account/sessions')
    check(s == 200 and len(d['sessions']) == 2 and sum(x['current'] for x in d['sessions']) == 1, 'session list marks current')
    s, d, _ = p2.api('/account/password', {'current': 'nope-nope-nope', 'password': PASSWORD})
    check(s == 403, 'password change needs current password')
    s, d, _ = p2.api('/account/password', {'current': 'new-password-123', 'password': PASSWORD})
    check(s == 200 and d['revoked'] == 1, 'password changed, other session revoked')
    s, d, _ = other.api('/me')
    check(d.get('user') is None, 'other session signed out')
    s, d, _ = p2.api('/me')
    check(d['user']['username'] == 'player2', 'current session kept')
    other.api('/auth/login', {'login': 'player2', 'password': PASSWORD})
    s, d, _ = p2.api('/account/sessions/revoke', {})
    check(s == 200 and d['revoked'] == 1, 'revoke other sessions')
    s, d, _ = p2.api('/account/username', {'username': 'admin'})
    check(s == 409, 'rename to taken username refused')
    s, d, _ = p2.api('/account/username', {'username': 'player-two'})
    check(s == 200, 'rename')

    # bans
    sql("UPDATE users SET banned_at = NOW() WHERE username='player-two'")
    s, d, _ = p2.api('/me')
    check(d.get('user') is None, 'banned user session ignored')
    s, d, _ = Client().api('/auth/login', {'login': 'player-two', 'password': PASSWORD})
    check(s == 403, 'banned user cannot log in')
    sql("UPDATE users SET banned_at = NULL WHERE username='player-two'")

    # rate limit
    rl = Client()
    codes = [rl.api('/auth/login', {'login': 'player1', 'password': 'bad-password'})[0] for _ in range(11)]
    check(codes[-1] == 429 and codes[0] == 401, 'login rate limited per user')

    # pages
    s, _, b = Client().get('/nope')
    check(s == 404 and 'Page not found' in b or '404' in b, '404 page')
    s, h, _ = Client().get('/auth/login')
    check(h.get('x-frame-options') == 'DENY' and h.get('x-content-type-options') == 'nosniff', 'security headers')
    check(Client_gzip('/auth/login') in ('gzip', 'br', 'zstd'), 'html is compressed')


def Client_gzip(path):
    req = urllib.request.Request(BASE + path, headers={'Accept-Encoding': 'gzip, br'})
    with urllib.request.urlopen(req) as r:
        return r.headers.get('Content-Encoding', '') or 'none'


# ---------------------------------------------------------------- tenancy
def test_tenancy():
    section('01 multi-tenant isolation')
    sql("INSERT INTO tenants (uuid, slug, name, host) VALUES (UUID(), 'beta', 'Beta', 'beta.localhost')")
    sql("INSERT INTO settings (tenant_uuid, `key`, value) SELECT uuid, 'registration', 'open' FROM tenants WHERE slug='beta'")
    b = Client(host='beta.localhost:18080')
    deadline = time.time() + 40
    s, h = 0, {}
    while time.time() < deadline:
        s, h, _ = b.get('/')
        if s == 303 and h.get('location') == '/setup':
            break
        time.sleep(2)
    check(s == 303 and h.get('location') == '/setup', 'new tenant picked up by cache, needs its own setup')
    s, d, _ = b.api('/auth/login', {'login': 'admin', 'password': PASSWORD})
    check(s == 401, 'users of tenant A cannot log in on tenant B')
    s, d, _ = b.api('/setup', {'username': 'admin', 'password': PASSWORD})
    check(s == 200, 'same username allowed on another tenant')
    a = Client()
    a.api('/auth/login', {'login': 'admin', 'password': PASSWORD})
    stolen = Client(host='beta.localhost:18080')
    stolen.jar.set_cookie(http.cookiejar.Cookie(0, 'lh_session', a.cookie(), None, False, 'localhost', True, False, '/', True, False, None, False, None, None, {}))
    s, d, _ = stolen.api('/me')
    check(d.get('user') is None, "tenant A session is not valid on tenant B")


if __name__ == '__main__':
    t0 = time.time()
    for fn in [test_foundations, test_auth, test_tenancy]:
        try:
            fn()
        except Exception as e:  # a crash in a section is a failure, keep going
            check(False, f'{fn.__name__} crashed: {e!r}')
    print(f"\n{results['pass']} passed, {results['fail']} failed in {time.time() - t0:.1f}s")
    sys.exit(1 if results['fail'] else 0)
