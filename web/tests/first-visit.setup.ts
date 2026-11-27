import { test, expect } from '@playwright/test'

test('首访只需两次管理员密钥，确认后直接进入控制台', async ({ page, browser }) => {
  const password = process.env.WBP_E2E_PASSWORD
  if (!password) throw new Error('请为独立测试实例指定随机测试密钥')
  let submissions = 0
  page.on('request', req => {
    if (req.method() === 'POST' && new URL(req.url()).pathname === '/api/auth/setup') submissions++
  })
  await page.goto('/')
  await expect(page.getByRole('heading', { name: '设置管理员密钥' })).toBeVisible()
  const key = page.getByLabel('管理员密钥', { exact: true })
  const confirm = page.getByLabel('确认管理员密钥', { exact: true })
  await expect(page.locator('form input')).toHaveCount(2)
  for (const field of [key, confirm]) {
    await expect(field).toHaveAttribute('type', 'password')
    await expect(field).toHaveAttribute('autocomplete', 'new-password')
  }
  await key.fill(password)
  await confirm.fill(password + '!')
  const submit = page.getByRole('button', { name: '设置并登录' })
  await submit.click()
  await expect(page.getByRole('alert')).toContainText('两次输入的管理员密钥不一致')
  expect(submissions).toBe(0)
  expect((await (await page.request.get('/api/auth/status')).json()).needs_setup).toBe(true)
  // A second visitor opened the setup form before the first submitted it.
  const other = await browser.newPage({ viewport: { width: 375, height: 812 } })
  try {
    await other.goto(page.url())
    await other.getByLabel('管理员密钥', { exact: true }).fill(password + '!')
    await other.getByLabel('确认管理员密钥').fill(password + '!')
    expect(await other.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
    await confirm.fill(password)
    await submit.click()
    await expect(page.getByRole('heading', { name: '号池管理' })).toBeVisible()
    expect(submissions).toBe(1)
    await page.reload()
    await expect(page.getByRole('heading', { name: '号池管理' })).toBeVisible()
    await other.getByRole('button', { name: '设置并登录' }).click()
    await expect(other.getByRole('heading', { name: '管理员登录' })).toBeVisible()
    await expect(other.locator('form input')).toHaveCount(1)
    await expect(other.getByLabel('管理员密钥', { exact: true })).toHaveValue('')
  } finally {
    await other.close()
  }
})
