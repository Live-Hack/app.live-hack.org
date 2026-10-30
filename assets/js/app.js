// Live Hack front: tiny, dependency-free. Behaviours are wired through data attributes.
(() => {
  'use strict';
  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => Array.from(r.querySelectorAll(s));

  // ---- API ----
  async function api(path, body) {
    const opts = { headers: { 'X-Requested-With': 'fetch', 'Accept': 'application/json' }, credentials: 'same-origin' };
    if (body !== undefined) {
      opts.method = 'POST';
      opts.headers['Content-Type'] = 'application/json';
      opts.body = JSON.stringify(body);
    }
    let res, data = {};
    try {
      res = await fetch(path, opts);
      data = await res.json().catch(() => ({}));
    } catch (e) {
      return { ok: false, status: 0, data: { error: 'Network error. Check your connection and try again.' } };
    }
    return { ok: res.ok, status: res.status, data };
  }

  // ---- toasts ----
  function toast(text, kind = 'ok', ms = 4000) {
    const box = $('.toasts') || document.body.appendChild(Object.assign(document.createElement('div'), { className: 'toasts' }));
    const t = document.createElement('div');
    t.className = `toast toast--${kind}`;
    t.textContent = text;
    box.appendChild(t);
    setTimeout(() => { t.classList.add('is-leaving'); setTimeout(() => t.remove(), 260); }, ms);
  }
  // message carried across a redirect
  try {
    const flash = sessionStorage.getItem('lh-flash');
    if (flash) { sessionStorage.removeItem('lh-flash'); document.addEventListener('DOMContentLoaded', () => toast(flash)); }
  } catch (e) { /* storage blocked */ }
  function go(url, flash) {
    if (flash) try { sessionStorage.setItem('lh-flash', flash); } catch (e) {}
    location.assign(url);
  }

  function formData(form) {
    const out = {};
    for (const el of form.elements) {
      if (!el.name || el.disabled) continue;
      if (el.type === 'checkbox') out[el.name] = el.checked;
      else if (el.type === 'radio') { if (el.checked) out[el.name] = el.value; }
      else if (el.type === 'number') out[el.name] = el.value === '' ? null : Number(el.value);
      else if (el.type === 'datetime-local') out[el.name] = el.value ? new Date(el.value).toISOString() : '';
      else out[el.name] = el.value;
    }
    return out;
  }

  function setBusy(btn, busy) {
    if (!btn) return;
    btn.disabled = busy;
    btn.classList.toggle('is-loading', busy);
  }

  const after = {}; // named hooks: data-then="name"

  // ---- forms: <form data-api="/api/v1/..." data-redirect="/" | data-reload | data-reset> ----
  document.addEventListener('submit', async (e) => {
    const form = e.target.closest('form[data-api]');
    if (!form) return;
    e.preventDefault();
    const msg = $('[data-msg]', form) || $('[data-msg]', form.parentElement);
    const btn = $('[type=submit]', form);
    if (msg) { msg.textContent = ''; msg.className = 'form__msg'; }
    setBusy(btn, true);
    const { ok, data } = await api(form.dataset.api, formData(form));
    setBusy(btn, false);
    if (!ok) {
      if (msg) { msg.textContent = data.error || 'Something went wrong.'; msg.classList.add('is-error'); }
      else toast(data.error || 'Something went wrong.', 'error');
      form.classList.remove('is-shake'); void form.offsetWidth; form.classList.add('is-shake');
      return;
    }
    const note = form.dataset.toast || data.message;
    if (form.dataset.redirect !== undefined) return go(form.dataset.redirect || '/', note);
    if (form.dataset.reload !== undefined) return go(location.href, note);
    if (form.dataset.reset !== undefined) form.reset();
    // a named hook renders its own feedback
    if (form.dataset.then && after[form.dataset.then]) return after[form.dataset.then](data, form);
    if (note && msg) { msg.textContent = note; msg.classList.add('is-ok'); }
    else if (note) toast(note);
  });

  // ---- one-click actions: <a|button data-post="/api/v1/..." data-redirect | data-then> ----
  document.addEventListener('click', async (e) => {
    const el = e.target.closest('[data-post]');
    if (!el) return;
    e.preventDefault();
    if (el.dataset.confirm && !el.classList.contains('is-armed')) {
      el.classList.add('is-armed');
      el.dataset.label = el.textContent;
      el.textContent = el.dataset.confirm;
      setTimeout(() => { el.classList.remove('is-armed'); el.textContent = el.dataset.label; }, 3000);
      return;
    }
    setBusy(el, true);
    let body = {};
    try { if (el.dataset.postJson) body = JSON.parse(el.dataset.postJson); } catch (err) { /* keep {} */ }
    const { ok, data } = await api(el.dataset.post, body);
    setBusy(el, false);
    if (!ok) return toast(data.error || 'Something went wrong.', 'error');
    if (el.dataset.redirect !== undefined) return go(el.dataset.redirect || '/', data.message);
    if (el.dataset.then && after[el.dataset.then]) after[el.dataset.then](data, el);
    if (data.message) toast(data.message);
  });

  // ---- header: burger + dropdowns ----
  document.addEventListener('click', (e) => {
    const burger = e.target.closest('.header__btn');
    if (burger) {
      const open = !burger.classList.contains('header__btn--active');
      burger.classList.toggle('header__btn--active', open);
      burger.setAttribute('aria-expanded', open);
      $('.header__menu')?.classList.toggle('header__menu--active', open);
      return;
    }
    const toggle = e.target.closest('[data-dropdown]');
    $$('.dropdown-menu.show').forEach((m) => { if (!toggle || m !== toggle.nextElementSibling) { m.classList.remove('show'); m.previousElementSibling?.setAttribute('aria-expanded', 'false'); } });
    if (toggle) {
      const menu = toggle.nextElementSibling;
      const open = !menu.classList.contains('show');
      menu.classList.toggle('show', open);
      toggle.setAttribute('aria-expanded', open);
    }
  });
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') $$('.dropdown-menu.show').forEach((m) => m.classList.remove('show'));
  });

  // ---- active nav link ----
  document.addEventListener('DOMContentLoaded', () => {
    const p = location.pathname;
    $$('.header__nav-link').forEach((a) => {
      const h = a.getAttribute('href');
      if (h !== '/' && (p === h || p.startsWith(h + '/'))) a.classList.add('is-active');
    });
  });

  // ---- copy buttons: <button data-copy="#input"> ----
  document.addEventListener('click', async (e) => {
    const b = e.target.closest('[data-copy]');
    if (!b) return;
    const src = $(b.dataset.copy);
    const text = src ? (src.value ?? src.textContent) : '';
    try { await navigator.clipboard.writeText(text); toast('Copied.'); }
    catch (err) { src?.select?.(); toast('Press Ctrl+C to copy.', 'error'); }
  });

  // ---- settings: session list ----
  // "Chrome on macOS" from a user agent string
  function device(ua) {
    if (!ua) return 'Unknown device';
    const b = /Edg\//.test(ua) ? 'Edge' : /OPR\//.test(ua) ? 'Opera' : /Firefox\//.test(ua) ? 'Firefox'
      : /Chrome\//.test(ua) ? 'Chrome' : /Safari\//.test(ua) ? 'Safari' : ua.split(/[\/ ]/)[0];
    const os = /Windows/.test(ua) ? 'Windows' : /Android/.test(ua) ? 'Android' : /iPhone|iPad/.test(ua) ? 'iOS'
      : /Mac OS X/.test(ua) ? 'macOS' : /Linux/.test(ua) ? 'Linux' : '';
    return os ? `${b} on ${os}` : b;
  }
  const fmt = (d) => new Date(d.replace(' ', 'T') + 'Z').toLocaleString();
  async function loadSessions() {
    const list = $('[data-sessions]');
    if (!list) return;
    const { ok, data } = await api('/api/v1/account/sessions');
    list.textContent = '';
    if (!ok) return;
    for (const s of data.sessions) {
      const li = document.createElement('li');
      const left = document.createElement('div');
      const b = document.createElement('b');
      b.textContent = device(s.user_agent);
      b.title = s.user_agent;
      left.append(b, document.createTextNode(`${s.ip} · since ${fmt(s.created_at)}`));
      li.append(left);
      if (s.current) li.append(Object.assign(document.createElement('span'), { className: 'badge badge--ok', textContent: 'This device' }));
      list.append(li);
    }
  }
  after.sessions = loadSessions;
  document.addEventListener('DOMContentLoaded', loadSessions);

  // ---- flag submission result ----
  after.flag = (data, form) => {
    const box = form.closest('.chall-flag-submit');
    const out = $('[data-msg]', box);
    out.className = 'chall-flag-result';
    out.textContent = data.message || '';
    if (data.status === 'correct' || data.status === 'already_solved') {
      out.classList.add('is-correct');
      box.classList.add('is-correct');
      if (!data.test) setTimeout(() => location.reload(), 1100);
    } else {
      out.classList.add('is-error');
      form.classList.remove('is-shake'); void form.offsetWidth; form.classList.add('is-shake');
      const input = $('input[name=flag]', form);
      input?.select();
    }
  };

  // ---- filter toolbars submit themselves (debounced for text) ----
  document.addEventListener('DOMContentLoaded', () => {
    $$('form[data-autosubmit]').forEach((f) => {
      let t;
      f.addEventListener('change', () => f.requestSubmit());
      f.addEventListener('input', (e) => {
        if (e.target.type !== 'search') return;
        clearTimeout(t);
        t = setTimeout(() => f.requestSubmit(), 450);
      });
    });
    // keep focus in the search box after an auto-submit reload
    const q = $('form[data-autosubmit] input[type=search]');
    if (q && q.value) { q.focus(); q.setSelectionRange(q.value.length, q.value.length); }
  });

  // ---- server times (UTC) shown in the viewer's timezone ----
  document.addEventListener('DOMContentLoaded', () => {
    $$('time[datetime]').forEach((t) => {
      const d = new Date(t.getAttribute('datetime').replace(' ', 'T'));
      if (!isNaN(d)) t.textContent = d.toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' });
    });
  });

  // ---- organizer hooks ----
  after.reload = (data) => go(location.href, data.message);
  after.open = (data) => go(data.edit || location.href, data.message);

  // raw-body upload with progress (no multipart)
  function upload(url, file, onProgress) {
    return new Promise((resolve) => {
      const x = new XMLHttpRequest();
      x.open('POST', url);
      x.setRequestHeader('X-Requested-With', 'fetch');
      x.setRequestHeader('Content-Type', 'application/octet-stream');
      x.upload.onprogress = (e) => e.lengthComputable && onProgress?.(e.loaded / e.total);
      x.onload = () => { let d = {}; try { d = JSON.parse(x.responseText); } catch (e) {} resolve({ ok: x.status < 300, data: d }); };
      x.onerror = () => resolve({ ok: false, data: { error: 'Network error during upload.' } });
      x.send(file);
    });
  }

  document.addEventListener('DOMContentLoaded', () => {
    // file drop zones
    $$('[data-upload]').forEach((zone) => {
      const input = $('input[type=file]', zone);
      const bar = $('progress', zone);
      const send = async (files) => {
        if (!files.length) return;
        bar.hidden = false;
        let done = 0;
        for (const f of files) {
          const url = zone.dataset.upload + '?name=' + encodeURIComponent(f.name);
          const r = await upload(url, f, (p) => { bar.value = ((done + p) / files.length) * 100; });
          if (!r.ok) { toast(`${f.name}: ${r.data.error || 'upload failed'}`, 'error', 6000); bar.hidden = true; return; }
          done++;
        }
        go(location.href, `${done} file(s) uploaded.`);
      };
      input.addEventListener('change', () => send(Array.from(input.files)));
      zone.addEventListener('dragover', (e) => { e.preventDefault(); zone.classList.add('is-over'); });
      zone.addEventListener('dragleave', () => zone.classList.remove('is-over'));
      zone.addEventListener('drop', (e) => { e.preventDefault(); zone.classList.remove('is-over'); send(Array.from(e.dataTransfer.files)); });
    });

    // zip import
    $$('input[data-import]').forEach((input) => input.addEventListener('change', async () => {
      const f = input.files[0];
      if (!f) return;
      toast(`Importing ${f.name}…`);
      const r = await upload('/api/v1/manage/import', f);
      input.value = '';
      if (!r.ok) return toast(r.data.error || 'Import failed.', 'error', 8000);
      go(location.href, r.data.message);
    }));

    // markdown preview
    $$('[data-md-tab]').forEach((b) => b.addEventListener('click', async () => {
      const field = b.closest('.field');
      const src = $('[data-md-src]', field), out = $('[data-md-out]', field);
      $$('[data-md-tab]', field).forEach((x) => x.classList.toggle('is-active', x === b));
      const preview = b.dataset.mdTab === 'preview';
      if (preview) {
        out.innerHTML = '<div class="skeleton" style="height:120px"></div>';
        const { ok, data } = await api('/api/v1/manage/preview', { markdown: src.value });
        out.innerHTML = ok ? data.html : '';
      }
      src.hidden = preview;
      out.hidden = !preview;
    }));

    // challenge type → show the URL field only for "static"
    $$('[data-ctype]').forEach((sel) => {
      const sync = () => $$('[data-show-for]', sel.form).forEach((el) => { el.hidden = el.dataset.showFor !== sel.value; });
      sel.addEventListener('change', sync);
      sync();
    });

    // unsaved-changes marker
    $$('form[data-dirty]').forEach((f) => {
      const note = $('[data-dirty-note]', f);
      let dirty = false;
      f.addEventListener('input', () => { dirty = true; if (note) note.hidden = false; });
      f.addEventListener('submit', () => { dirty = false; setTimeout(() => { if (note) note.hidden = true; }, 300); });
      window.addEventListener('beforeunload', (e) => { if (dirty) { e.preventDefault(); e.returnValue = ''; } });
    });
  });

  // ---- dates: inputs show local time, the server stores UTC ----
  const pad = (n) => String(n).padStart(2, '0');
  document.addEventListener('DOMContentLoaded', () => {
    $$('input[type=datetime-local][data-utc]').forEach((el) => {
      const d = new Date(el.dataset.utc);
      if (!el.dataset.utc || isNaN(d)) return;
      el.value = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
    });
  });

  // ---- countdowns: <div data-countdown="ISO"><span data-countdown-value> ; reloads at zero ----
  document.addEventListener('DOMContentLoaded', () => {
    $$('[data-countdown]').forEach((box) => {
      const target = Date.parse(box.dataset.countdown);
      const out = $('[data-countdown-value]', box);
      if (isNaN(target) || !out) return;
      const tick = () => {
        let s = Math.max(0, Math.floor((target - Date.now()) / 1000));
        const d = Math.floor(s / 86400); s %= 86400;
        const h = Math.floor(s / 3600); s %= 3600;
        const m = Math.floor(s / 60); s %= 60;
        out.textContent = (d ? `${d}d ` : '') + `${pad(h)}:${pad(m)}:${pad(s)}`;
        if (target <= Date.now()) { clearInterval(timer); setTimeout(() => location.reload(), 1500); }
      };
      const timer = setInterval(tick, 1000);
      tick();
    });
  });

  // ---- lab widget: <div data-lab="/api/v1/challenges/<id>/instance"> ----
  function labWidget(box) {
    const url = box.dataset.lab;
    const body = $('[data-lab-body]', box);
    const dot = $('.lab__dot', box);
    let poll = null, tick = null;
    const el = (tag, cls, text) => Object.assign(document.createElement(tag), cls ? { className: cls } : {}, text != null ? { textContent: text } : {});
    const button = (label, cls, path) => {
      const b = el('button', `btn ${cls}`, label);
      b.type = 'button';
      b.addEventListener('click', async () => {
        b.disabled = true; b.classList.add('is-loading');
        const r = await api(url + path, {});
        b.disabled = false; b.classList.remove('is-loading');
        if (!r.ok) return toast(r.data.error || 'Something went wrong.', 'error', 6000);
        if (r.data.message) toast(r.data.message);
        render(r.data);
      });
      return b;
    };
    function render(d) {
      clearTimeout(poll); clearInterval(tick);
      body.textContent = '';
      dot.className = 'lab__dot';
      if (d.state === 'running') {
        dot.classList.add('is-on');
        const row = el('div', 'lab__url');
        const input = el('input', 'input'); input.value = d.url; input.readOnly = true; input.id = 'lab-url';
        row.append(input);
        const copy = el('button', 'btn btn--ghost', 'Copy'); copy.type = 'button'; copy.dataset.copy = '#lab-url';
        row.append(copy);
        if (d.protocol === 'http') { const open = el('a', 'btn', 'Open'); open.href = d.url; open.target = '_blank'; open.rel = 'noopener'; row.append(open); }
        const meta = el('div', 'lab__meta');
        const left = el('b', null, '');
        meta.append(document.createTextNode('Stops in '), left);
        const end = Date.parse(d.expires_at);
        const upd = () => {
          let s = Math.max(0, Math.floor((end - Date.now()) / 1000));
          left.textContent = `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
          if (s === 0) { clearInterval(tick); setTimeout(load, 2000); }
        };
        upd(); tick = setInterval(upd, 1000);
        const sp = el('span', 'spacer');
        meta.append(sp);
        if (d.can_extend) meta.append(button('Extend', 'btn--ghost btn--sm', '/extend'));
        meta.append(button('Stop', 'btn--ghost btn--sm', '/stop'));
        body.append(row, meta);
      } else if (d.state === 'starting') {
        dot.classList.add('is-busy');
        body.append(el('p', 'muted', 'Starting your lab… the first start can take a minute while the image downloads.'));
        poll = setTimeout(load, 1500);
      } else if (d.state === 'elsewhere') {
        body.append(el('p', 'muted', d.message));
      } else {
        if (d.state === 'failed') body.append(el('p', 'form__msg is-error', d.message));
        body.append(button('Start lab', '', ''));
      }
    }
    async function load() {
      const r = await api(url);
      if (!r.ok) { body.textContent = ''; body.append(el('p', 'muted', r.data.error || 'Lab unavailable.')); return; }
      render(r.data);
    }
    load();
  }
  document.addEventListener('DOMContentLoaded', () => {
    $$('[data-lab]').forEach(labWidget);
    // editor: lab settings only for instance challenges
    const ct = $('[data-ctype]');
    if (ct) {
      const sync = () => $$('[data-show-for-type]').forEach((f) => { f.hidden = f.dataset.showForType !== ct.value; });
      ct.addEventListener('change', sync); sync();
    }
  });

  // ---- admin ----
  after.link = (data, el) => {
    const box = $('[data-link-box]');
    if (!box) return toast(data.url || data.message || 'Done.');
    $('#link-out').value = data.url;
    box.hidden = false;
    box.scrollIntoView({ behavior: 'smooth', block: 'center' });
    if (data.message) toast(data.message);
    if (el && el.tagName === 'FORM') el.reset();
  };
  document.addEventListener('change', async (e) => {
    const sel = e.target.closest('select[data-role-for]');
    if (!sel) return;
    const { ok, data } = await api(`/api/v1/admin/users/${sel.dataset.roleFor}/role`, { role: sel.value });
    toast(ok ? data.message : (data.error || 'Could not change the role.'), ok ? 'ok' : 'error');
  });
  document.addEventListener('click', async (e) => {
    const b = e.target.closest('[data-logs]');
    if (!b) return;
    const out = $('[data-logs-out]');
    out.hidden = false;
    out.textContent = 'Loading…';
    const { ok, data } = await api(b.dataset.logs);
    out.textContent = ok ? (data.logs || '(no output)') : (data.error || 'Unavailable.');
    out.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
  });
  document.addEventListener('DOMContentLoaded', () => {
    const p = location.pathname;
    $$('.admin-nav a').forEach((a) => { if (a.getAttribute('href') === p) a.classList.add('is-active'); });
  });

  window.LH = { api, toast, go, after, $, $$ };
})();
