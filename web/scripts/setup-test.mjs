import assert from 'node:assert/strict'
const base = process.env.WBP_E2E_URL || 'http://127.0.0.1:3040'
const password = process.env.WBP_E2E_PASSWORD
if (!password) throw Error('测试需随机密码')
const call = (path, method = 'GET', body, headers = {}) =>
  fetch(base + path, {
    method,
    headers: { 'Content-Type': 'application/json', Origin: base, ...headers },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
const forwarded = { 'X-Forwarded-For': '192.0.2.10' }
const status = async () => (await call('/api/auth/status', 'GET', undefined, forwarded)).json()
const initial = await status()
assert.equal(initial.needs_setup, true, '仅对未初始化的独立测试实例运行')
assert.equal(initial.setup_code_required, false, '首次设置不应要求来自服务器日志的验证码')
const setKey = (key, confirm, headers = forwarded) =>
  call('/api/auth/setup', 'POST', { password: key, password_confirm: confirm }, headers)
assert.equal((await call('/api/accounts')).status, 401)
assert.equal((await setKey(password, password, { Origin: 'https://invalid.example' })).status, 403)
assert.equal((await setKey(password, undefined)).status, 422)
assert.equal((await setKey(password, password + '!')).status, 400)
assert.equal((await setKey('short', 'short')).status, 400)
const oversized = '密'.repeat(25)
assert.equal((await setKey(oversized, oversized)).status, 400)
assert.equal((await status()).needs_setup, true)
// Even on the Docker bridge, only one concurrent claimant may get a session.
const attempts = await Promise.all([setKey(password, password), setKey(password, password)])
assert.deepEqual(attempts.map(r => r.status).sort(), [200, 409])
assert.equal(attempts.find(r => r.status === 409).headers.get('set-cookie'), null)
const setup = attempts.find(r => r.status === 200)
assert.equal((await setup.json()).authenticated, true)
const raw = setup.headers.get('set-cookie')
assert.match(raw, /HttpOnly/)
assert.match(raw, /SameSite=Strict/)
const cookie = raw.split(';')[0]
assert.equal((await status()).needs_setup, false)
assert.equal((await setKey(password + '!', password + '!')).status, 409)
assert.equal((await call('/api/accounts', 'GET', undefined, { Cookie: cookie })).status, 200)
assert.equal((await call('/api/auth/logout', 'POST', {}, { Cookie: cookie })).status, 200)
assert.equal((await call('/api/accounts', 'GET', undefined, { Cookie: cookie })).status, 401)
assert.equal((await call('/api/auth/login', 'POST', { password: password + '!' })).status, 401)
assert.equal((await call('/api/auth/login', 'POST', { password })).status, 200)
for (const p of ['/access_account/test.json', '/docker-compose.yaml', '/data/staging/'])
  assert.equal((await call(p)).status, 404)
console.log('无验证码的双密钥确认、并发一次性初始化、同源保护、单密钥登录和私有目录隔离已验证')
