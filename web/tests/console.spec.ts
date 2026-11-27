import { test, expect, type Page } from '@playwright/test'
import { readFile } from 'node:fs/promises'
import { randomUUID } from 'node:crypto'
async function login(page: Page) {
  await page.goto('/')
  await page.getByLabel('管理员密码').fill(process.env.WBP_E2E_PASSWORD!)
  await page.getByRole('button', { name: '登录', exact: true }).click()
  await expect(page.getByRole('heading', { name: '号池管理' })).toBeVisible()
}
test('登录、退出和会话失效', async ({ page }) => {
  await login(page)
  await page.getByRole('button', { name: '退出登录' }).click()
  await expect(page.getByRole('heading', { name: '管理员登录' })).toBeVisible()
  await page.reload()
  await expect(page.getByRole('heading', { name: '管理员登录' })).toBeVisible()
})
test('新建、筛选、编辑、导出和删除', async ({ page }) => {
  await login(page)
  const label = '验收' + randomUUID().slice(0, 8)
  const token = 'fixture-' + randomUUID()
  await page.getByRole('button', { name: '新建账号' }).click()
  await page.getByLabel('账号名称').fill(label)
  await page.getByLabel('访问凭证').fill(token)
  await page.getByLabel('所属号池').selectOption('intl')
  await page.getByRole('button', { name: '保存' }).click()
  await expect(page.getByText(label, { exact: true })).toBeVisible()
  await page.getByRole('button', { name: '国际版', exact: true }).click()
  await page.getByLabel('搜索账号').fill(label)
  await expect(page).toHaveURL(/realm=intl/)
  await page.reload()
  await expect(page.getByLabel('搜索账号')).toHaveValue(label)
  await page.getByRole('button', { name: '编辑 ' + label }).click()
  await page.getByLabel('启用账号').uncheck()
  await page.getByRole('button', { name: '保存' }).click()
  await expect(page.getByText('已停用')).toBeVisible()
  await page.getByRole('button', { name: '导出', exact: true }).click()
  await expect(page.getByText(/备份包含明文/)).toBeVisible()
  await page.getByRole('checkbox').check()
  const dl = page.waitForEvent('download')
  await page.getByRole('button', { name: '确认导出' }).click()
  const path = await (await dl).path()
  expect(path).toBeTruthy()
  const backup = JSON.parse(await readFile(path!, 'utf8'))
  expect(backup.accounts).toHaveLength(1)
  expect(backup.accounts[0].credentials.access_token).toBe(token)
  await page.getByRole('button', { name: '删除 ' + label }).click()
  await page.getByRole('button', { name: '确认删除' }).click()
  await expect(page.getByRole('heading', { name: '没有匹配的账号' })).toBeVisible()
})
test('文件导入、重复凭证合并及分页', async ({ page }) => {
  await login(page)
  const prefix = '批量' + randomUUID().slice(0, 6)
  const items = Array.from({ length: 23 }, (_, i) => ({
    label: prefix + i,
    accessToken: 'fixture-' + randomUUID(),
  }))
  await page.getByRole('button', { name: '导入', exact: true }).click()
  await page
    .getByLabel('选择文件')
    .setInputFiles({
      name: 'fixtures.json',
      mimeType: 'application/json',
      buffer: Buffer.from(JSON.stringify({ items })),
    })
  await page.getByRole('button', { name: '开始导入' }).click()
  await page.getByLabel('搜索账号').fill(prefix)
  await expect(page.getByText('共 23 个账号')).toBeVisible()
  await page.getByRole('button', { name: '下一页' }).click()
  await expect(page.locator('.rows li')).toHaveCount(3)
  await page.getByRole('button', { name: '导入', exact: true }).click()
  await page
    .getByLabel('账号 JSON 或 token')
    .fill(JSON.stringify({ ...items[0], label: prefix + '更新' }))
  await page.getByRole('button', { name: '开始导入' }).click()
  await expect(page.getByText('共 23 个账号')).toBeVisible()
  await page.getByLabel('搜索账号').fill(prefix + '更新')
  await expect(page.locator('.rows li')).toHaveCount(1)
})
test('移动端布局和实时服务状态', async ({ page }) => {
  await page.setViewportSize({ width: 375, height: 812 })
  await login(page)
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBeTruthy()
  await page.getByRole('button', { name: '打开菜单' }).click()
  await page.getByRole('button', { name: '服务状态' }).click()
  await expect(page.getByText('服务与 Redis 均已就绪')).toBeVisible()
  await page.getByRole('button', { name: '关闭', exact: true }).click()
  await expect(page.getByRole('dialog')).toHaveCount(0)
})
