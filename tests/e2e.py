#!/usr/bin/env python3
"""End-to-end suite: drives a fresh instance through HTTP like a browser and an API client.
Usage: e2e.py BASE_URL COMPOSE_PROJECT. Stdlib only. Every phase adds its section; all run each time."""
import hashlib
import http.cookiejar
import io
import zipfile
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

    def raw(self, path, data, ctype='application/octet-stream'):
        req = urllib.request.Request(BASE + '/api/v1' + path, data=data, method='POST')
        req.add_header('X-Requested-With', 'fetch')
        req.add_header('Content-Type', ctype)
        s, h, b = self._do(req)
        try:
            return s, json.loads(b or b'{}')
        except ValueError:
            return s, {'_raw': b[:200]}

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
    codes = [rl.api('/auth/login', {'login': 'ghost-user', 'password': 'bad-password'})[0] for _ in range(11)]
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


# ---------------------------------------------------------------- 02 shell UI
def test_shell():
    section('02 shell UI')
    c = Client()
    c.api('/auth/login', {'login': 'admin', 'password': PASSWORD})
    for path in ['/', '/account/settings', '/nope']:
        s, _, b = c.get(path)
        check('class="header"' in b and 'class="footer"' in b, f'{path} has shared header and footer')
        check('/assets/css/app.css?v=' in b and 'bootstrap-grid' not in b, f'{path} loads app.css, no bootstrap grid')
        check(b.count('<script') == 1, f'{path} loads a single script')
    sizes = {}
    for a in ['css/main.css', 'css/app.css', 'css/bootstrap-reboot.min.css', 'js/app.js']:
        s, h, b = c.get('/assets/' + a)
        sizes[a] = len(b.encode())
    check(sizes['js/app.js'] < 20_000, f"app.js under 20 KB ({sizes['js/app.js']})")
    css = sizes['css/main.css'] + sizes['css/app.css'] + sizes['css/bootstrap-reboot.min.css']
    check(css < 160_000, f'CSS under 160 KB raw ({css})')
    s, _, b = c.get('/assets/css/main.css')
    check('@import' not in b, 'no render-blocking @import in CSS')
    check(b.count('{') == b.count('}'), 'main.css braces balanced')
    s, _, b = c.get('/assets/css/app.css')
    check(b.count('{') == b.count('}'), 'app.css braces balanced')
    s, _, b = Client().get('/nope')
    check(s == 404 and 'Sign in' in b, '404 page for anonymous has sign-in button')


# ---------------------------------------------------------------- 03 challenges
def add_chall(slug, name, category='web', difficulty='easy', points=100, flag='LH{flag}', flag_type='static',
              status='published', practice=1, description='Find the flag.', ctype='file', static_url=''):
    t = sql("SELECT uuid FROM tenants WHERE slug='default'")
    esc = lambda v: v.replace('\\', '\\\\').replace("'", "\\'")
    sql(f"INSERT INTO challs (uuid, tenant_uuid, slug, name, description, category, difficulty, points, ctype, static_url, flag_type, flag, status, practice, published_at) "
        f"VALUES (UUID(), '{t}', '{slug}', '{esc(name)}', '{esc(description)}', '{category}', '{difficulty}', {points}, '{ctype}', '{static_url}', '{flag_type}', '{esc(flag)}', '{status}', {practice}, NOW())")
    return sql(f"SELECT uuid FROM challs WHERE slug='{slug}'")


def test_challenges():
    section('03 challenges & flags')
    admin = Client(); admin.api('/auth/login', {'login': 'admin', 'password': PASSWORD})
    p1 = Client(); p1.api('/auth/login', {'login': 'player1', 'password': PASSWORD})
    p3 = Client(); p3.api('/auth/register', {'username': 'player3', 'password': PASSWORD})
    s, h, _ = Client().get('/challenges')
    check(s == 303 and h['location'].startswith('/auth/login'), 'challenges need login')
    s, _, b = p1.get('/challenges')
    check(s == 200 and 'No challenges found' in b, 'empty catalogue state')

    web = add_chall('web-101', 'Cookie <b>Monster</b>', description='# Intro\n\n<script>alert(1)</script>\n\nLook at **cookies** <img src=x onerror=alert(1)>')
    add_chall('rsa-easy', 'RSA baby', category='crypto', difficulty='medium', points=300, flag='lh{case}', flag_type='case_insensitive')
    rx = add_chall('regex-one', 'Regex one', category='misc', points=50, flag=r'LH\{[0-9]{4}\}', flag_type='regex')
    draft = add_chall('draft-one', 'Secret draft', status='draft')
    add_chall('event-only', 'Event only', practice=0)
    add_chall('online', 'Online one', ctype='static', static_url='https://example.com/chall')
    for i in range(24):
        add_chall(f'bulk-{i:02d}', f'Bulk {i:02d}', category='forensics', points=10 + i)

    s, _, b = p1.get('/challenges')
    check(s == 200 and '28 challenges' in b, 'catalogue counts published practice challenges only')
    check('Secret draft' not in b and 'Event only' not in b, 'draft and event-only hidden')
    check('Cookie &lt;b&gt;Monster&lt;/b&gt;' in b or 'Cookie &lt;b&gt;Monster' in b or 'page=2' in b, 'names escaped or on page 2')
    check('page=2' in b and b.count('chall-grid-card"') + b.count('chall-grid-card ') >= 24, 'paginated 24 per page')
    s, _, b = p1.get('/challenges?category=crypto')
    check('RSA baby' in b and 'Bulk 00' not in b and '1 challenge<' in b, 'filter by category')
    s, _, b = p1.get('/challenges?q=bulk%2001')
    check('Bulk 01' in b and '1 challenge<' in b, 'search by name')
    s, _, b = p1.get('/challenges?q=%25')
    check('0 challenges' in b, 'LIKE wildcards are escaped')
    s, _, b = p1.get('/challenges?difficulty=medium')
    check('RSA baby' in b and '1 challenge<' in b, 'filter by difficulty')
    s, _, b = p1.get('/challenges?sort=points_desc')
    check(b.find('RSA baby') < b.find('Bulk 23'), 'sort by points')

    # detail
    s, _, b = p1.get('/challenges/web-101')
    check(s == 200 and 'id="intro"' in b and 'href="#intro"' in b, 'markdown rendered with working heading anchors')
    check('<script>alert(1)</script>' not in b and 'onerror' not in b and '<strong>cookies</strong>' in b, 'raw HTML dropped from markdown')
    check('Cookie &lt;b&gt;Monster&lt;/b&gt;' in b, 'challenge name escaped')
    check('data-api="/api/v1/challenges/' + web + '/submit"' in b, 'flag form present')
    s, _, b = p1.get('/challenges/draft-one')
    check(s == 404, 'player cannot see a draft')
    s, _, b = admin.get('/challenges/draft-one')
    check(s == 200 and 'notice' in b, 'admin previews a draft')
    s, _, b = p1.get('/challenges/online')
    check('https://example.com/chall' in b, 'static challenge shows its URL')
    s, _, b = p1.get('/challenges/nope')
    check(s == 404, 'unknown challenge 404')

    # files
    t = sql("SELECT uuid FROM tenants WHERE slug='default'")
    key = '11111111-2222-3333-4444-555555555555'
    subprocess.run(['docker', 'compose', '-p', PROJECT, 'exec', '-T', 'app', 'sh', '-c',
                    f'mkdir -p /app/data/files/{t} && printf "hello pcap" > /app/data/files/{t}/{key}'], check=True)
    sql(f"INSERT INTO challs_files (uuid, chall_uuid, filename, storage_key, sha256, size) VALUES ('f0000000-0000-0000-0000-000000000001', '{web}', 'cap ture.pcap', '{key}', 'abc', 10)")
    s, _, b = p1.get('/challenges/web-101')
    check('cap ture.pcap' in b and '/files/f0000000-0000-0000-0000-000000000001/' in b, 'file listed')
    s, h, b = p1.get('/challenges/web-101/files/f0000000-0000-0000-0000-000000000001/x')
    check(s == 200 and b == 'hello pcap' and 'attachment' in h.get('content-disposition', ''), 'file downloads as attachment')
    s, h, _ = Client().get('/challenges/web-101/files/f0000000-0000-0000-0000-000000000001/x')
    check(s == 303, 'file download needs login')
    s, _, _ = p1.get('/challenges/rsa-easy/files/f0000000-0000-0000-0000-000000000001/x')
    check(s == 404, 'file only through its own challenge')

    # flags
    s, d, _ = p1.api(f'/challenges/{web}/submit', {'flag': 'LH{nope}'})
    check(s == 200 and d['status'] == 'wrong', 'wrong flag')
    s, d, _ = p1.api(f'/challenges/{web}/submit', {'flag': ''})
    check(s == 400, 'empty flag refused')
    s, d, _ = p1.api(f'/challenges/{web}/submit', {'flag': ' LH{flag} '})
    check(s == 200 and d['status'] == 'correct' and d['first_blood'] is True and d['points'] == 100, 'correct flag, first blood')
    s, d, _ = p1.api(f'/challenges/{web}/submit', {'flag': 'LH{flag}'})
    check(d['status'] == 'already_solved', 'already solved')
    s, d, _ = p3.api(f'/challenges/{web}/submit', {'flag': 'LH{flag}'})
    check(d['status'] == 'correct' and d['first_blood'] is False, 'second solver is not first blood')
    s, _, b = p1.get('/challenges/web-101')
    check('chall-status--solved' in b and 'data-then="flag"' not in b and 'first blood' in b, 'detail shows solved state and first blood')
    s, _, b = p1.get('/challenges?state=solved')
    check('Cookie' in b and '1 challenge<' in b, 'filter solved')
    s, _, b = p1.get('/challenges?state=unsolved')
    check('27 challenges' in b, 'filter unsolved')
    rsa = sql("SELECT uuid FROM challs WHERE slug='rsa-easy'")
    s, d, _ = p1.api(f'/challenges/{rsa}/submit', {'flag': 'LH{CASE}'})
    check(d['status'] == 'correct', 'case-insensitive flag')
    s, d, _ = p1.api(f'/challenges/{rx}/submit', {'flag': 'LH{12345}'})
    check(d['status'] == 'wrong', 'regex anchored')
    s, d, _ = p1.api(f'/challenges/{rx}/submit', {'flag': 'LH{2026}'})
    check(d['status'] == 'correct', 'regex flag')
    s, d, _ = p1.api(f'/challenges/{draft}/submit', {'flag': 'LH{flag}'})
    check(s == 404, 'player cannot submit to a draft')
    s, d, _ = admin.api(f'/challenges/{draft}/submit', {'flag': 'LH{flag}'})
    check(d.get('test') is True and d['status'] == 'correct', 'admin tests a draft flag')
    check(sql(f"SELECT COUNT(*) FROM challs_solved WHERE chall_uuid='{draft}'") == '0', 'draft test not recorded')
    check(sql(f"SELECT COUNT(*) FROM challs_submissions WHERE chall_uuid='{web}'") == '3', 'submissions logged (wrong + 2 correct)')
    check(sql("SELECT COUNT(*) FROM challs_submissions WHERE value_hash = 'LH{flag}'") == '0', 'flags stored hashed')
    codes = [p3.api(f'/challenges/{rsa}/submit', {'flag': 'x'})[0] for _ in range(11)]
    check(codes[0] == 200 and codes[-1] == 429, 'flag submissions rate limited')
    s, d, _ = p1.api('/challenges?category=crypto')
    check(s == 200 and d['total'] == 1 and d['challenges'][0]['solved'] is True and 'flag' not in d['challenges'][0], 'API list, no flag leak')


# ---------------------------------------------------------------- 04 challenge management
def make_zip(entries):
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, 'w') as z:
        for name, data in entries.items():
            z.writestr(name, data)
    return buf.getvalue()


def test_manage():
    section('04 challenge management')
    admin = Client(); admin.api('/auth/login', {'login': 'admin', 'password': PASSWORD})
    p1 = Client(); p1.api('/auth/login', {'login': 'player1', 'password': PASSWORD})
    sql("UPDATE users SET role='author' WHERE username IN ('player3', 'player-two')")
    author = Client(); author.api('/auth/login', {'login': 'player3', 'password': PASSWORD})
    co = Client(); co.api('/auth/login', {'login': 'player-two', 'password': PASSWORD})

    s, _, _ = p1.get('/manage/challenges')
    check(s == 404, 'players have no manage page')
    s, h, _ = Client().get('/manage/challenges')
    check(s == 303, 'manage needs login')
    s, d, _ = p1.api('/manage/challenges', {'name': 'x'})
    check(s == 403, 'players cannot create challenges')
    s, _, b = author.get('/')
    check('/manage/challenges' in b and 'href="/admin"' not in b, 'author menu has manage, not admin')

    s, d, _ = author.api('/manage/challenges', {'name': 'Say "hi" <x>'})
    check(s == 201 and d['slug'] == 'say-hi-x', 'author creates a draft')
    cu = d['uuid']
    s, d, _ = author.api('/manage/challenges', {'name': 'Say "hi" <x>'})
    check(d.get('slug') == 'say-hi-x-2', 'slug made unique')
    author.api(f"/manage/challenges/{d['uuid']}/delete", {})
    s, _, b = author.get(f'/manage/challenges/{cu}')
    check(s == 200 and 'value="Say &quot;hi&quot; &lt;x&gt;"' in b, 'editor escapes values in attributes')
    base = {'name': 'Say hi', 'description': '**bold**', 'category': 'Web', 'difficulty': 'hard', 'points': 400,
            'ctype': 'static', 'static_url': 'javascript:alert(1)', 'flag_type': 'static', 'flag': 'LH{hi}'}
    s, d, _ = author.api(f'/manage/challenges/{cu}', base)
    check(s == 400, 'javascript: URL refused')
    s, d, _ = author.api(f'/manage/challenges/{cu}', {**base, 'static_url': 'https://hi.example.com', 'points': 'abc'})
    check(s == 400, 'bad points refused')
    s, d, _ = author.api(f'/manage/challenges/{cu}', {**base, 'static_url': 'https://hi.example.com', 'flag_type': 'regex', 'flag': '('})
    check(s == 400, 'broken regex refused')
    s, d, _ = author.api(f'/manage/challenges/{cu}', {**base, 'static_url': 'https://hi.example.com', 'flag': ''})
    check(s == 200, 'save without flag as draft')
    s, d, _ = author.api(f'/manage/challenges/{cu}/status', {'status': 'published'})
    check(s == 400 and 'flag' in d['error'], 'cannot publish without a flag')
    s, d, _ = author.api(f'/manage/challenges/{cu}', {**base, 'static_url': 'https://hi.example.com'})
    check(s == 200, 'save full challenge')
    check(sql(f"SELECT category FROM challs WHERE uuid='{cu}'") == 'web', 'category normalised')
    s, d, _ = p1.api(f'/challenges/{cu}/submit', {'flag': 'LH{hi}'})
    check(s == 404, 'draft hidden from players')
    s, d, _ = author.api(f'/manage/challenges/{cu}/status', {'status': 'published'})
    check(s == 200, 'publish')
    s, _, b = p1.get('/challenges/say-hi-x')
    check(s == 200 and 'https://hi.example.com' in b and '<strong>bold</strong>' in b, 'published challenge visible to players')
    check(sql(f"SELECT COUNT(*) FROM audit_log WHERE target_uuid='{cu}'") >= '4', 'changes are audited')

    # ownership
    web = sql("SELECT uuid FROM challs WHERE slug='web-101'")
    s, d, _ = author.api(f'/manage/challenges/{web}', base)
    check(s == 404, "author cannot edit someone else's challenge")
    s, _, _ = author.get(f'/manage/challenges/{web}')
    check(s == 404, "author cannot open someone else's editor")
    s, _, b = author.get('/manage/challenges')
    check('Say hi' in b and 'Cookie' not in b, 'author sees only own challenges')
    s, _, b = admin.get('/manage/challenges')
    check('Say hi' in b and 'Cookie' in b, 'admin sees all challenges')
    s, _, b = admin.get('/manage/challenges?status=draft')
    check('Secret draft' in b and 'Say hi' not in b, 'status filter')

    # files
    data = b'\x00\x01pcap-data' * 1000
    s, d = author.raw(f'/manage/challenges/{cu}/files?name=../../evil/dump.pcap', data)
    check(s == 201 and d['name'] == 'dump.pcap' and d['sha256'] == hashlib.sha256(data).hexdigest() and d['size'] == len(data), 'raw upload stored with sha256, name cleaned')
    fid = d['uuid']
    s, h, b = p1.get(f'/challenges/say-hi-x/files/{fid}/dump.pcap')
    check(s == 200 and len(b.encode('latin-1', 'replace')) > 0, 'uploaded file downloadable by players')
    s, d = p1.raw(f'/manage/challenges/{cu}/files?name=x', b'x')
    check(s == 403, 'players cannot upload')
    s, d = author.raw(f'/manage/challenges/{web}/files?name=x', b'x')
    check(s == 404, 'cannot upload to a foreign challenge')

    # co-authors
    s, d, _ = co.api(f'/manage/challenges/{cu}', base)
    check(s == 404, 'non co-author refused')
    s, d, _ = author.api(f'/manage/challenges/{cu}/authors', {'username': 'nobody-here'})
    check(s == 400, 'unknown co-author refused')
    s, d, _ = author.api(f'/manage/challenges/{cu}/authors', {'username': 'player-two'})
    check(s == 200, 'co-author added')
    s, d, _ = co.api(f'/manage/challenges/{cu}', {**base, 'static_url': 'https://hi.example.com', 'points': 450})
    check(s == 200, 'co-author edits')
    s, d, _ = co.api(f'/manage/challenges/{cu}/delete', {})
    check(s == 403, 'co-author cannot delete')

    # export
    req = urllib.request.Request(BASE + f'/api/v1/manage/challenges/{cu}/export')
    s, h, b = author._do(req)
    z = zipfile.ZipFile(io.BytesIO(b))
    names = z.namelist()
    yml = z.read('say-hi-x/challenge.yml').decode()
    check(s == 200 and 'say-hi-x/dist/dump.pcap' in names and 'name: Say hi' in yml and 'value: 450' in yml and 'LH{hi}' in yml, 'export zip in ctfcli format')
    check(z.read('say-hi-x/dist/dump.pcap') == data, 'export contains the file bytes')

    # import (ctfcli bundle with two challenges)
    bundle = make_zip({
        'pkg/heap/challenge.yml': 'name: Heap Party\ncategory: pwn\nvalue: 500\ndescription: smash it\nflags:\n  - {type: static, content: "LH{Heap}", data: case_insensitive}\nfiles:\n  - dist/heap.bin\nstate: visible\n',
        'pkg/heap/dist/heap.bin': b'ELF...',
        'pkg/osint/challenge.yml': 'name: Where am I\ncategory: osint\nvalue: 150\nflags: ["LH{paris}"]\n',
    })
    s, d = author.raw('/manage/import', bundle, 'application/zip')
    check(s == 200 and len(d['imported']) == 2 and all(i['created'] for i in d['imported']), 'zip import creates challenges')
    heap = next(i['uuid'] for i in d['imported'] if i['slug'] == 'heap-party')
    check(sql(f"SELECT status FROM challs WHERE uuid='{heap}'") == 'published', 'state: visible publishes')
    check(sql("SELECT status FROM challs WHERE slug='where-am-i'") == 'draft', 'no state stays draft')
    s, d, _ = p1.api(f'/challenges/{heap}/submit', {'flag': 'lh{heap}'})
    check(d.get('status') == 'correct', 'imported flag works (case-insensitive)')
    heap_v2 = 'name: Heap Party\ncategory: pwn\nvalue: 501\nflags:\n  - {type: static, content: "LH{Heap}", data: case_insensitive}\nfiles:\n  - dist/heap.bin\nstate: visible\n'
    s, d = author.raw('/manage/import', make_zip({'pkg/heap/challenge.yml': heap_v2, 'pkg/heap/dist/heap.bin': b'ELF2'}), 'application/zip')
    check(s == 200 and not any(i['created'] for i in d['imported']), 're-import updates in place')
    check(sql(f"SELECT points FROM challs WHERE uuid='{heap}'") == '501' and sql(f"SELECT COUNT(*) FROM challs_solved WHERE chall_uuid='{heap}'") == '1', 'update keeps solves')
    check(sql(f"SELECT COUNT(*) FROM challs_files WHERE chall_uuid='{heap}'") == '1', 'files replaced, not duplicated')
    s, d = co.raw('/manage/import', bundle, 'application/zip')
    check(s == 400 and 'not one of its authors' in d['error'], "cannot overwrite others' challenges by import")
    evil = make_zip({'c/challenge.yml': 'name: Evil\nflags: ["x"]\nfiles:\n  - ../../etc/passwd\n'})
    s, d = author.raw('/manage/import', evil, 'application/zip')
    check(s == 400 and sql("SELECT COUNT(*) FROM challs WHERE slug='evil'") == '0', 'path traversal in bundle refused, nothing created')
    s, d = author.raw('/manage/import', b'not a zip', 'application/zip')
    check(s == 400, 'non-zip refused')
    s, d = author.raw('/manage/import', make_zip({'readme.txt': 'x'}), 'application/zip')
    check(s == 400 and 'No challenge.yml' in d['error'], 'zip without challenge.yml refused')

    # preview + delete
    s, d, _ = author.api('/manage/preview', {'markdown': '# T\n<script>x</script>'})
    check(s == 200 and '<script>' not in d['html'] and 'T</h1>' in d['html'], 'markdown preview is safe')
    s, d, _ = author.api(f'/manage/challenges/{cu}/delete', {})
    check(s == 200 and sql(f"SELECT COUNT(*) FROM challs WHERE uuid='{cu}'") == '0', 'owner deletes')
    s, _, _ = p1.get(f'/challenges/say-hi-x/files/{fid}/dump.pcap')
    check(s == 404, 'deleted challenge files gone')

    # CLI import
    t = subprocess.run(['docker', 'compose', '-p', PROJECT, 'exec', '-T', 'app', 'sh', '-c',
                        'mkdir -p /app/data/bundle/crypto-1/dist && printf "name: CLI crypto\\ncategory: crypto\\nvalue: 75\\nflags: [\\"LH{cli}\\"]\\nfiles: [dist/key.pem]\\n" > /app/data/bundle/crypto-1/challenge.yml && printf "KEY" > /app/data/bundle/crypto-1/dist/key.pem && ./app_live_hack import /app/data/bundle --publish'],
                       capture_output=True, text=True)
    check(t.returncode == 0 and 'created cli-crypto (1 file(s))' in t.stdout, f'CLI import ({t.stdout.strip()[-80:]} {t.stderr.strip()[-200:]})')
    check(sql("SELECT status FROM challs WHERE slug='cli-crypto'") == 'published', 'CLI --publish')


# ---------------------------------------------------------------- 05 scoreboard
def test_scoreboard():
    section('05 scoreboard & profiles')
    admin = Client(); admin.api('/auth/login', {'login': 'admin', 'password': PASSWORD})
    p1 = Client(); p1.api('/auth/login', {'login': 'player1', 'password': PASSWORD})
    time.sleep(5.2)  # rankings are cached 5 s; earlier pages may have warmed it
    s, d, _ = p1.api('/scoreboard')
    st = d.get('standings', [])
    names = [e['name'] for e in st]
    check(s == 200 and names[:1] == ['player1'] and st[0]['score'] == 951, f'practice ranking by points ({names[:3]})')
    check('player3' not in names and 'admin' not in names, 'organizers stay off the practice scoreboard')
    check(d['me']['rank'] == 1, 'API returns my rank')

    a = Client(); a.api('/auth/register', {'username': 'tie-a', 'password': PASSWORD})
    b = Client(); b.api('/auth/register', {'username': 'tie-b', 'password': PASSWORD})
    rsa = sql("SELECT uuid FROM challs WHERE slug='rsa-easy'")
    a.api(f'/challenges/{rsa}/submit', {'flag': 'lh{case}'})
    time.sleep(0.05)
    b.api(f'/challenges/{rsa}/submit', {'flag': 'lh{case}'})
    time.sleep(5.2)  # cache TTL
    s, d, _ = p1.api('/scoreboard')
    names = [e['name'] for e in d['standings']]
    check(names.index('tie-a') < names.index('tie-b'), 'ties broken by earliest solve')
    check('tie-a' in names, 'new solves appear after the cache window')
    s, d, _ = p1.api('/scoreboard?category=crypto')
    check([e['name'] for e in d['standings']][:3] == ['player1', 'tie-a', 'tie-b'] and d['standings'][0]['score'] == 300, 'category scoreboard')

    s, _, b_ = p1.get('/scoreboard')
    check(s == 200 and 'class="is-me"' in b_ and 'tie-a' in b_ and '/users/player1' in b_, 'scoreboard page highlights me')
    s, _, b_ = p1.get('/scoreboard?category=crypto')
    check('is-active" href="/scoreboard?category=crypto"' in b_, 'category tab active')

    sql("UPDATE users SET banned_at=NOW() WHERE username='tie-b'")
    time.sleep(5.2)
    s, d, _ = p1.api('/scoreboard')
    check('tie-b' not in [e['name'] for e in d['standings']], 'banned players removed from the board')
    s, _, _ = p1.get('/users/tie-b')
    check(s == 404, 'banned profile hidden')
    sql("UPDATE users SET banned_at=NULL WHERE username='tie-b'")

    s, _, b_ = p1.get('/users/player1')
    check(s == 200 and '#1' in b_ and '951' in b_ and 'RSA baby' in b_ and 'bars__track' in b_, 'profile with rank, points, solves, categories')
    check('Edit account' in b_, 'own profile links to settings')
    s, _, b_ = p1.get('/users/tie-a')
    check(s == 200 and 'Edit account' not in b_, 'other profile')
    s, _, _ = p1.get('/users/nobody-at-all')
    check(s == 404, 'unknown profile 404')
    s, d, _ = p1.api('/users/player1')
    check(s == 200 and d['score'] == 951 and len(d['solves']) == 4, 'profile API')

    s, _, b_ = p1.get('/')
    check('Welcome back, player1' in b_ and 'Up next' in b_ and 'Latest solves' in b_ and 'Top 10' in b_, 'signed-in dashboard')
    check('tie-b' in b_ and 'solved' in b_, 'activity feed lists recent solves')
    s, _, b_ = Client().get('/')
    check('home__title' in b_ and 'Welcome back' not in b_, 'anonymous home is the landing page')

    admin.api('/admin/settings', {'scoreboard': 'hidden'})
    s, _, b_ = p1.get('/scoreboard')
    check('hidden the scoreboard' in b_, 'hidden scoreboard page for players')
    s, d, _ = p1.api('/scoreboard')
    check(s == 403, 'hidden scoreboard API for players')
    s, _, b_ = p1.get('/users/tie-a')
    check('951' not in b_ and '>—<' in b_, "others' ranks hidden too")
    s, d, _ = admin.api('/scoreboard')
    check(s == 200, 'admin still sees the scoreboard')
    admin.api('/admin/settings', {'scoreboard': 'visible'})


# ---------------------------------------------------------------- 06 events & teams
def iso(dt):
    return dt.strftime('%Y-%m-%dT%H:%M:%SZ')


def test_events():
    import datetime as _dt
    section('06 events & teams')
    now = _dt.datetime.now(_dt.timezone.utc).replace(microsecond=0)
    H = _dt.timedelta(hours=1)
    admin = Client(); admin.api('/auth/login', {'login': 'admin', 'password': PASSWORD})
    p1 = Client(); p1.api('/auth/login', {'login': 'player1', 'password': PASSWORD})
    ta = Client(); ta.api('/auth/login', {'login': 'tie-a', 'password': PASSWORD})
    tb = Client(); tb.api('/auth/login', {'login': 'tie-b', 'password': PASSWORD})
    p5 = Client(); p5.api('/auth/register', {'username': 'player5', 'password': PASSWORD})

    s, d, _ = p1.api('/manage/events', {'title': 'x'})
    check(s == 403, 'players cannot create events')
    s, d, _ = admin.api('/manage/events', {'title': 'Bad', 'starts_at': iso(now + H), 'ends_at': iso(now)})
    check(s == 400, 'end before start refused')
    live = {'title': 'Winter <CTF>', 'description': 'Have **fun**', 'starts_at': iso(now - _dt.timedelta(minutes=5)), 'ends_at': iso(now + H),
            'team_mode': True, 'max_team_size': 2, 'registration': 'open', 'after_end': 'practice', 'status': 'published'}
    s, d, _ = admin.api('/manage/events', live)
    check(s == 201 and d['slug'] == 'winter-ctf', 'admin creates a live team event')
    ev, slug = d['uuid'], d['slug']
    s, d, _ = admin.api('/manage/events', {**live, 'title': 'Spring CTF', 'starts_at': iso(now + 24 * H), 'ends_at': iso(now + 26 * H)})
    up_slug = d['slug']
    s, d, _ = admin.api('/manage/events', {**live, 'title': 'Hidden draft', 'status': 'draft'})
    s, d, _ = admin.api('/manage/events', {**live, 'title': 'Solo cup', 'team_mode': False, 'registration': 'code', 'join_code': 'S3CRET'})
    solo, solo_slug = d['uuid'], d['slug']

    c1 = add_chall('ev-web', 'Event web', points=100, flag='LH{ev}')
    c2 = add_chall('ev-pwn', 'Event pwn', category='pwn', points=200, flag='LH{pwn}')
    s, d, _ = admin.api(f'/manage/events/{ev}/challenges', {'chall': c1, 'points': 777})
    check(s == 200, 'add challenge with points override')
    admin.api(f'/manage/events/{ev}/challenges', {'chall': c2})
    admin.api(f'/manage/events/{solo}/challenges', {'chall': c2})
    check(sql(f"SELECT practice FROM challs WHERE uuid='{c1}'") == '0', 'event challenge leaves practice')
    s, _, b = p1.get('/challenges')
    check('Event web' not in b, 'event challenge not in the practice list')
    s, _, _ = p1.get('/challenges/ev-web')
    check(s == 404, 'unregistered player cannot open an event challenge')
    s, d, _ = p1.api(f'/challenges/{c1}/submit', {'flag': 'LH{ev}'})
    check(s == 403, 'unregistered player cannot submit')

    # pages
    s, _, b = p1.get('/events')
    check(s == 200 and 'Live now' in b and 'Winter &lt;CTF&gt;' in b and 'Spring CTF' in b and 'Hidden draft' not in b, 'events list (live first, drafts hidden)')
    s, _, _ = p1.get('/events/hidden-draft')
    check(s == 404, 'draft event hidden from players')
    s, _, b = p1.get(f'/events/{slug}')
    check('<title>Winter &lt;CTF&gt; · Test CTF</title>' in b, 'event page keeps the layout <title>')
    check(s == 200 and 'Ends in' in b and 'Create team' in b and 'Register to play' in b and '<strong>fun</strong>' in b, 'live event page before joining')

    # teams
    s, d, _ = p1.api(f'/events/{slug}/teams', {'name': 'Red <Team>'})
    check(s == 200 and len(d['team']['invite_code']) == 16, 'create team')
    code = d['team']['invite_code']
    s, d, _ = ta.api(f'/events/{slug}/teams', {'name': 'red <team>'})
    check(s == 409, 'team names unique (case-insensitive)')
    s, d, _ = p1.api(f'/events/{slug}/teams', {'name': 'Other'})
    check(s == 409, 'one team per player per event')
    s, d, _ = ta.api(f'/events/{slug}/teams/join', {'invite': 'NOPE'})
    check(s == 404, 'bad invite code')
    s, d, _ = ta.api(f'/events/{slug}/teams/join', {'invite': code.lower()})
    check(s == 200, 'join with code (case-insensitive)')
    s, d, _ = tb.api(f'/events/{slug}/teams/join', {'invite': code})
    check(s == 409 and 'full' in d['error'], 'team size enforced')
    s, d, _ = tb.api(f'/events/{slug}/teams', {'name': 'Blue'})
    s, d, _ = ta.api(f'/events/{slug}/teams/kick', {'username': 'player1'})
    check(s == 403, 'only the captain can kick')
    s, _, b = p1.get(f'/events/{slug}')
    check('Red &lt;Team&gt;' in b and code in b and 'Event web' in b and '777 pts' in b, 'team panel, invite code and challenges with event points')

    # play
    s, _, b = ta.get('/challenges/ev-web')
    check(s == 200 and 'notice--event' in b and '777' in b, 'event challenge page for a member')
    s, d, _ = ta.api(f'/challenges/{c1}/submit', {'flag': 'LH{ev}'})
    check(d.get('status') == 'correct' and d['points'] == 777 and d['event'] is True and d['first_blood'] is True, 'solve counts for the event with override points')
    s, d, _ = p1.api(f'/challenges/{c1}/submit', {'flag': 'LH{ev}'})
    check(d.get('status') == 'already_solved', 'team solves are shared')
    check(sql(f"SELECT COUNT(*) FROM challs_solved WHERE chall_uuid='{c1}' AND event_uuid='{ev}'") == '1', 'one solve row per team')
    s, d, _ = ta.api(f'/events/{slug}/teams/leave', {})
    check(s == 409, 'cannot leave a team that scored')
    s, d, _ = tb.api(f'/challenges/{c2}/submit', {'flag': 'LH{pwn}'})
    check(d.get('status') == 'correct', 'other team solves')
    time.sleep(5.2)
    s, d, _ = p1.api(f'/events/{slug}/scoreboard')
    st = d['standings']
    check(s == 200 and d['teams'] is True and [x['name'] for x in st] == ['Red <Team>', 'Blue'] and st[0]['score'] == 777, 'team scoreboard')
    s, _, b = p1.get(f'/events/{slug}/scoreboard')
    check('Red &lt;Team&gt;' in b and 'class="is-me"' in b, 'team scoreboard page escapes names, highlights my team')
    s, d, _ = p1.api('/scoreboard')
    check(all(x['score'] < 1700 for x in d['standings']) and next(x for x in d['standings'] if x['name'] == 'player1')['score'] == 951, 'event solves stay out of practice')

    # freeze
    admin.api(f'/manage/events/{ev}', {**live, 'starts_at': live['starts_at'], 'freeze_at': iso(_dt.datetime.now(_dt.timezone.utc) - _dt.timedelta(seconds=1))})
    time.sleep(1.2)
    s, d, _ = tb.api(f'/challenges/{c1}/submit', {'flag': 'LH{ev}'})
    time.sleep(5.2)
    s, d, _ = p1.api(f'/events/{slug}/scoreboard')
    blue = next(x for x in d['standings'] if x['name'] == 'Blue')
    check(d['frozen'] is True and blue['score'] == 200, 'frozen board hides solves after the freeze')
    s, d, _ = admin.api(f'/events/{slug}/scoreboard')
    blue = next(x for x in d['standings'] if x['name'] == 'Blue')
    check(blue['score'] == 977, 'organizers see the live board')
    admin.api(f'/manage/events/{ev}', {**live, 'freeze_at': ''})

    # solo event with code
    s, d, _ = p5.api(f'/events/{solo_slug}/register', {})
    check(s == 403, 'solo event needs its code')
    s, d, _ = p5.api(f'/events/{solo_slug}/register', {'code': 'S3CRET'})
    check(s == 200, 'register with code')
    s, d, _ = p5.api(f'/events/{solo_slug}/teams', {'name': 'x', 'code': 'S3CRET'})
    check(s == 400, 'no teams in a solo event')
    s, d, _ = p5.api(f'/challenges/{c2}/submit', {'flag': 'LH{pwn}'})
    check(d.get('status') == 'correct' and d['event'] is True, 'solo event solve')

    # disqualification
    blue_uuid = sql("SELECT uuid FROM teams WHERE name='Blue'")
    s, d, _ = ta.api(f'/manage/events/{ev}/disqualify/{blue_uuid}', {'disqualified': True})
    check(s == 403, 'players cannot disqualify')
    s, d, _ = admin.api(f'/manage/events/{ev}/disqualify/{blue_uuid}', {'disqualified': True})
    check(s == 200, 'admin disqualifies a team')
    time.sleep(5.2)
    s, d, _ = p1.api(f'/events/{slug}/scoreboard')
    check('Blue' not in [x['name'] for x in d['standings']], 'disqualified team off the board')
    s, d, _ = tb.api(f'/challenges/{c2}/submit', {'flag': 'LH{pwn}'})
    check(s == 403, 'disqualified team cannot submit')
    s, _, b = tb.get(f'/events/{slug}')
    check('disqualified' in b, 'disqualified team sees why')
    p5_uuid = sql("SELECT uuid FROM users WHERE username='player5'")
    s, d, _ = admin.api(f'/manage/events/{solo}/disqualify/{p5_uuid}', {'disqualified': True})
    check(s == 200, 'admin disqualifies a solo player')

    # CTFtime
    s, d, _ = p1.api(f'/events/{slug}/ctftime')
    check(s == 403, 'CTFtime feed closed while live')
    s, d, _ = admin.api(f'/events/{slug}/ctftime')
    check(s == 200 and d['standings'][0] == {'pos': 1, 'team': 'Red <Team>', 'score': 777}, 'CTFtime format')

    # upcoming event
    s, _, b = p1.get(f'/events/{up_slug}')
    check('Starts in' in b and 'open when the event starts' in b, 'upcoming event page with countdown')
    s, d, _ = p1.api(f'/events/{up_slug}/teams', {'name': 'Early birds'})
    check(s == 200, 'register before the start')

    # end of event: challenges move to practice, submissions become practice solves
    admin.api(f'/manage/events/{ev}', {**live, 'ends_at': iso(_dt.datetime.now(_dt.timezone.utc) - _dt.timedelta(seconds=1)), 'starts_at': iso(now - 2 * H)})
    deadline = time.time() + 40
    while time.time() < deadline and sql(f"SELECT practice FROM challs WHERE uuid='{c1}'") != '1':
        time.sleep(2)
    check(sql(f"SELECT practice FROM challs WHERE uuid='{c1}'") == '1', 'ended event moves its challenges to practice')
    s, d, _ = p1.api(f'/challenges/{c1}/submit', {'flag': 'LH{ev}'})
    check(d.get('status') == 'correct' and d.get('event') is False, 'after the end solves count for practice')
    s, d, _ = Client().api(f'/events/{slug}/ctftime')
    check(s == 200 and d['standings'][0]['team'] == 'Red <Team>', 'public CTFtime feed after the end')
    s, d, _ = tb.api(f'/events/{slug}/teams', {'name': 'Late'})
    check(s == 403, 'no registration after the end')


# ---------------------------------------------------------------- 07 labs
def docker(*args):
    return subprocess.run(['docker', *args], capture_output=True, text=True)


def wait_running(c, cu, timeout=90):
    deadline = time.time() + timeout
    d = {}
    while time.time() < deadline:
        s, d, _ = c.api(f'/challenges/{cu}/instance')
        if d.get('state') in ('running', 'failed', 'none'):
            return d
        time.sleep(1)
    return d


def fetch(url):
    try:
        with urllib.request.urlopen(url, timeout=5) as r:
            return r.read().decode()
    except Exception as e:
        return f'ERR {e}'


def test_labs():
    section('07 labs')
    admin = Client(); admin.api('/auth/login', {'login': 'admin', 'password': PASSWORD})
    p1 = Client(); p1.api('/auth/login', {'login': 'player1', 'password': PASSWORD})
    ta = Client(); ta.api('/auth/login', {'login': 'tie-a', 'password': PASSWORD})
    yml = open('examples/labs/web-hello/challenge.yml').read()
    s, d = admin.raw('/manage/import', make_zip({'labs/web-hello/challenge.yml': yml}), 'application/zip')
    check(s == 200 and d['imported'][0]['slug'] == 'hello-lab', 'example lab imports with its instance settings')
    cu = d['imported'][0]['uuid']
    check(sql(f"SELECT CONCAT(image, ':', port, ':', ttl_minutes, ':', readonly_rootfs) FROM challs_runtime WHERE chall_uuid='{cu}'") == 'livehack/example-web:latest:8080:30:1', 'runtime stored')
    s, d, _ = p1.api(f'/challenges/{cu}/instance', {})
    check(s == 404, 'no lab on a draft for players')
    s, d, _ = admin.api(f'/manage/challenges/{cu}/status', {'status': 'published'})
    check(s == 200, 'publish per-instance lab without a static flag')

    s, d, _ = admin.api('/manage/challenges', {'name': 'Lab without image'})
    nocfg = d['uuid']
    admin.api(f'/manage/challenges/{nocfg}', {'name': 'Lab without image', 'ctype': 'instance', 'flag': 'LH{x}', 'points': 10})
    s, d, _ = admin.api(f'/manage/challenges/{nocfg}/status', {'status': 'published'})
    check(s == 400 and 'lab' in d['error'], 'cannot publish a lab challenge without its image')
    s, d, _ = admin.api(f'/manage/challenges/{nocfg}/runtime', {'image': 'bad image', 'port': 80})
    check(s == 400, 'invalid image reference refused')
    s, d, _ = admin.api(f'/manage/challenges/{nocfg}/runtime', {'image': 'nginx', 'port': 80, 'env': 'FLAG=x'})
    check(s == 400, 'FLAG cannot be overridden in env')

    s, _, b = p1.get('/labs')
    check(s == 200 and 'Hello lab' in b, 'labs page lists lab challenges')
    s, _, b = p1.get('/challenges/hello-lab')
    check('data-lab="/api/v1/challenges/' + cu + '/instance"' in b, 'challenge page has the lab widget')
    s, d, _ = p1.api(f'/challenges/{cu}/submit', {'flag': 'LH{guess}'})
    check(s == 400 and 'lab' in d['error'], 'per-instance flag needs a lab first')
    s, d, _ = p1.api(f'/challenges/{cu}/instance')
    check(d.get('state') == 'none', 'no lab yet')

    s, d, _ = p1.api(f'/challenges/{cu}/instance', {})
    check(s == 201 and d['state'] == 'starting', 'start lab')
    s, d, _ = p1.api(f'/challenges/{cu}/instance', {})
    check(d.get('state') in ('starting', 'running'), 'second start returns the same lab')
    d1 = wait_running(p1, cu)
    check(d1.get('state') == 'running' and d1['url'].startswith('http://localhost:'), f"lab running ({d1})")
    page1 = fetch(d1.get('url', 'http://localhost:1'))
    flag1 = page1.split('flag: ')[1].split('<')[0] if 'flag: ' in page1 else ''
    check(flag1.startswith('LH{') and len(flag1) == 28, f'lab reachable and serves its own flag ({page1[:80]})')
    check(sql(f"SELECT COUNT(*) FROM instances WHERE chall_uuid='{cu}' AND status IN ('starting','running')") == '1', 'one live lab for the player')

    ta.api(f'/challenges/{cu}/instance', {})
    d2 = wait_running(ta, cu)
    page2 = fetch(d2.get('url', 'http://localhost:1'))
    flag2 = page2.split('flag: ')[1].split('<')[0] if 'flag: ' in page2 else ''
    check(d2.get('port') != d1.get('port') and flag2 and flag2 != flag1, 'second player gets another port and another flag')

    c1 = sql(f"SELECT container_id FROM instances WHERE chall_uuid='{cu}' AND user_uuid=(SELECT uuid FROM users WHERE username='player1') AND status='running'")
    c2 = sql(f"SELECT container_id FROM instances WHERE chall_uuid='{cu}' AND user_uuid=(SELECT uuid FROM users WHERE username='tie-a') AND status='running'")
    i1 = json.loads(docker('inspect', c1).stdout or '[{}]')[0]
    i2 = json.loads(docker('inspect', c2).stdout or '[{}]')[0]
    n1 = set((i1.get('NetworkSettings') or {}).get('Networks', {}).keys())
    n2 = set((i2.get('NetworkSettings') or {}).get('Networks', {}).keys())
    check(n1 and n2 and not (n1 & n2), f'each lab on its own network ({n1} / {n2})')
    hc = i1.get('HostConfig', {})
    check(hc.get('Memory') == 64 * 1024 * 1024 and hc.get('NanoCpus') == 250_000_000 and hc.get('PidsLimit') == 256, 'resource limits applied')
    check(hc.get('ReadonlyRootfs') is True and 'NET_RAW' in (hc.get('CapDrop') or []) and any('no-new-privileges' in x for x in hc.get('SecurityOpt') or []), 'hardening applied')
    check('FLAG=' + flag1 in (i1.get('Config', {}).get('Env') or []), 'flag injected as $FLAG')

    s, d, _ = ta.api(f'/challenges/{cu}/submit', {'flag': flag1})
    check(d.get('status') == 'wrong', "another player's lab flag is refused")
    check(sql(f"SELECT COUNT(*) FROM audit_log WHERE action='flag.shared' AND JSON_UNQUOTE(JSON_EXTRACT(meta, '$.chall'))='hello-lab'") == '1', 'flag sharing is logged for the organizers')
    s, d, _ = p1.api(f'/challenges/{cu}/submit', {'flag': flag1})
    check(d.get('status') == 'correct', 'own lab flag accepted')

    # one lab at a time
    s, d = admin.raw('/manage/import', make_zip({'l2/challenge.yml': yml.replace('Hello lab', 'Second lab').replace('# Import', '#') + ''}), 'application/zip')
    cu2 = d['imported'][0]['uuid']
    admin.api(f'/manage/challenges/{cu2}/status', {'status': 'published'})
    s, d, _ = p1.api(f'/challenges/{cu2}/instance', {})
    check(d.get('state') == 'elsewhere', 'one live lab per player across challenges')

    # extend / stop
    before = sql(f"SELECT expires_at FROM instances WHERE container_id='{c1}'")
    s, d, _ = p1.api(f'/challenges/{cu}/instance/extend', {})
    after = sql(f"SELECT expires_at FROM instances WHERE container_id='{c1}'")
    check(s == 200 and after > before, 'extend pushes the expiry')
    s, d, _ = p1.api(f'/challenges/{cu}/instance/stop', {})
    check(d.get('state') == 'none' and docker('inspect', c1).returncode != 0, 'stop removes the container')
    check(sql(f"SELECT status FROM instances WHERE container_id='{c1}'") == 'stopped', 'stopped in the database')

    # expiry and orphan cleanup by the sweeper
    sql(f"UPDATE instances SET expires_at = UTC_TIMESTAMP() - INTERVAL 1 MINUTE WHERE container_id='{c2}'")
    docker('run', '-d', '--name', 'lh-orphan-test', '--label', 'org.livehack.instance=deadbeef', '--label', f'org.livehack.ns={PROJECT}', 'busybox:1.36', 'sleep', '600')
    docker('run', '-d', '--name', 'lh-foreign-test', '--label', 'org.livehack.instance=deadbeef', '--label', 'org.livehack.ns=someone-else', 'busybox:1.36', 'sleep', '600')
    deadline = time.time() + 30
    while time.time() < deadline and (docker('inspect', c2).returncode == 0 or docker('inspect', 'lh-orphan-test').returncode == 0):
        time.sleep(2)
    check(docker('inspect', c2).returncode != 0 and sql(f"SELECT status FROM instances WHERE container_id='{c2}'") == 'stopped', 'expired lab removed by the sweeper')
    check(docker('inspect', 'lh-orphan-test').returncode != 0, 'orphan lab container removed')
    check(docker('inspect', 'lh-foreign-test').returncode == 0, "another deployment's labs are left alone")
    docker('rm', '-f', 'lh-foreign-test')
    s, d, _ = p1.api(f'/challenges/{nocfg}/instance')
    check(s in (404, 503), 'unconfigured lab answers clearly')


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
    for fn in [test_foundations, test_auth, test_shell, test_challenges, test_manage, test_scoreboard, test_events, test_labs, test_tenancy]:
        try:
            fn()
        except Exception as e:  # a crash in a section is a failure, keep going
            check(False, f'{fn.__name__} crashed: {e!r}')
    print(f"\n{results['pass']} passed, {results['fail']} failed in {time.time() - t0:.1f}s")
    sys.exit(1 if results['fail'] else 0)
