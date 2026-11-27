import assert from 'node:assert/strict'
const origin = process.env.WBP_E2E_URL,
  url = origin + '/api/accounts'
const h = { Origin: origin, 'Content-Type': 'application/json' }
const login = await fetch(origin + '/api/auth/login', {
  method: 'POST',
  headers: h,
  body: JSON.stringify({ password: process.env.WBP_E2E_PASSWORD }),
})
assert.equal(login.status, 200)
const cookie = login.headers.get('set-cookie').split(';')[0],
  auth = { Origin: origin, Cookie: cookie }
const batch = new FormData()
const padding = ' '.repeat(256 * 1024)
for (let i = 0; i < 36; i++) {
  const raw = { label: '批次验证-' + i, access_token: 'fixture-batch-' + i, enabled: false }
  batch.append('document', new Blob([JSON.stringify(raw) + padding]), 'same.json')
}
batch.append('document', new Blob(['{"schema":2}']), 'bad.json')
const r = await fetch(url + '/uploads?realm=auto', { method: 'POST', headers: auth, body: batch })
assert.equal(r.status, 202)
const ack = await r.json()
assert.equal(ack.id, ack.batch_id)
let obj
for (let n = 0; n < 120; n++) {
  const res = await fetch(url + '/uploads/' + ack.id, { headers: auth })
  assert.equal(res.status, 200)
  obj = await res.json()
  if (obj.status !== 'processing') break
  await new Promise((r) => setTimeout(r, 250))
}
assert.equal(obj.status, 'failed')
assert.equal(obj.uploaded, 37)
assert.equal(obj.imported, 36)
assert.equal(obj.failed, 1)
assert(!JSON.stringify(obj).includes('fixture-batch-'))
const filtered = await (
  await fetch(url + '?q=' + encodeURIComponent('批次验证') + '&view=ids', { headers: auth })
).json()
assert.equal(filtered.total, 36)
console.log('大于 8 MiB 的批次、后台回执、错误隔离和分页索引已验证')
