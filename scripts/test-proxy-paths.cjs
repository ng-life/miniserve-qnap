#!/usr/bin/env node
// Execute the shipped page script as if QTS displayed it at different URLs.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const html = fs.readFileSync(path.join(__dirname, '../shared/index.html'), 'utf8');
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];
const fields = ['share_dir', 'listen_address', 'port', 'title', 'route_prefix', 'username', 'password', 'color_scheme', 'sorting_method', 'sorting_order', 'index_file'];
const checkboxes = ['upload', 'mkdir', 'hidden', 'follow_symlinks', 'pretty_urls'];

async function checkPage(pagePath) {
  const elements = new Map([...html.matchAll(/id="([^"]+)"/g)].map(match => [match[1], {
    value: '', placeholder: '', textContent: '', innerHTML: '', checked: false,
    type: checkboxes.includes(match[1]) ? 'checkbox' : 'text',
    classList: { toggle() {} }, listeners: {},
    addEventListener(event, handler) { this.listeners[event] = handler; },
  }]));
  const config = Object.fromEntries(fields.map(field => [field, '']));
  Object.assign(config, {share_dir: '/share/Public', listen_address: '127.0.0.1', port: 18080, title: 'Original title'});
  const requests = [];
  const payload = () => ({running: true, started_at: 1, miniserve_version: '0.35.0', password_set: false, service_url: 'http://NAS-IP:18080', config: {...config}, logs: []});
  const context = vm.createContext({
    location: {pathname: pagePath, hostname: 'nas.example'},
    document: {
      getElementById(id) { assert.ok(elements.has(id), `Missing page element: ${id}`); return elements.get(id); },
      querySelectorAll() { return []; },
    },
    async fetch(url, options) {
      const pathname = new URL(url, `https://nas.example${pagePath}`).pathname;
      requests.push({pathname, options});
      const allowed = ['/miniserve/api/status', '/miniserve/api/config'];
      if (!allowed.includes(pathname)) return {ok: false, status: 404, async json() { return {message: 'Proxy route not found'}; }};
      assert.equal(options.credentials, 'same-origin');
      if (pathname.endsWith('/config')) {
        assert.equal(options.method, 'PUT');
        Object.assign(config, JSON.parse(options.body));
      }
      return {ok: true, status: 200, async json() { return payload(); }};
    },
    Intl, Date, setTimeout() { return 0; }, clearTimeout() {}, setInterval() {},
  });
  vm.runInContext(script, context);
  await vm.runInContext('refresh(true)', context);
  assert.equal(elements.get('status-title').textContent, '服务运行中');
  assert.equal(elements.get('title').value, 'Original title');
  elements.get('title').value = 'Updated title';
  await elements.get('config-form').listeners.submit({preventDefault() {}});
  assert.equal(config.title, 'Updated title');
  assert.equal(elements.get('toast').textContent, '配置已保存，Miniserve 已重启');
  await vm.runInContext('refresh(false)', context);
  assert.equal(elements.get('status-title').textContent, '服务运行中');
  assert.ok(requests.some(request => request.pathname === '/miniserve/api/status'));
  assert.ok(requests.some(request => request.pathname === '/miniserve/api/config'));
  assert.ok(requests.every(request => request.pathname.startsWith('/miniserve/api/')));
  console.log(`PASS ${pagePath}: status, save/restart and refresh use /miniserve/api/`);
}

(async () => {
  for (const pagePath of ['/miniserve', '/miniserve/', '/', '/index.html', '/cgi-bin/', '/cgi-bin/qpkg.cgi?name=miniserve-qnap']) await checkPage(pagePath);
})().catch(error => { console.error(error); process.exitCode = 1; });
