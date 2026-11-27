import { request } from './client'
export type Session = {
  authenticated: boolean
  needs_setup: boolean
  setup_code_required: boolean
  site_name: string
  version: string
}
export type Account = {
  id: string
  label: string
  realm: 'cn' | 'intl'
  enabled: boolean
  health: string
  kind: string
  updated_at: string
  note?: string
}
export type Page = { items: Account[]; total: number; page: number; page_size: number }
export const status = () => request<Session>('/api/auth/status')
export const authenticate = (setup: boolean, password: string, setup_code?: string) =>
  request('/api/auth/' + (setup ? 'setup' : 'login'), 'POST', { password, setup_code })
export const logout = () => request('/api/auth/logout', 'POST')
export const health = () => request<{ status: string; version: string }>('/health')
export const list = (filters: URLSearchParams) => request<Page>('/api/accounts?' + filters)
export const detail = (id: string) => request<Account>('/api/accounts/' + encodeURIComponent(id))
export const update = (id: string, body: unknown) =>
  request<Account>('/api/accounts/' + encodeURIComponent(id), 'PATCH', body)
export const remove = (id: string) => request('/api/accounts/' + encodeURIComponent(id), 'DELETE')
export const importAccounts = (body: unknown, realm: string) =>
  request<{ imported: number }>(
    '/api/accounts/import' + (realm ? '?realm=' + encodeURIComponent(realm) : ''),
    'POST',
    body,
  )

export type Receipt = {
  id: string
  status: 'processing' | 'failed' | 'completed'
  uploaded: number
  imported: number
  failed: number
  errors: string[]
}
export function upload(files: File[], realm: string) {
  const form = new FormData()
  for (const file of files) form.append('document', file)
  return request<{ batch_id: string }>(
    '/api/accounts/uploads?realm=' + encodeURIComponent(realm || 'auto'),
    'POST',
    form,
  )
}
export const receipt = (id: string) =>
  request<Receipt>('/api/accounts/uploads/' + encodeURIComponent(id))
