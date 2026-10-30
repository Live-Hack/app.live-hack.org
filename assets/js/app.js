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
    const msg = $('[data-msg]', form);
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
    if (form.dataset.then && after[form.dataset.then]) after[form.dataset.then](data, form);
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
    const { ok, data } = await api(el.dataset.post, {});
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

  window.LH = { api, toast, go, after, $, $$ };
})();
