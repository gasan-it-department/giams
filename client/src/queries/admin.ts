import {
  useMutation, useQuery, useQueryClient,
  type UseMutationResult, type UseQueryResult,
} from '@tanstack/react-query'
import { api } from '@/lib/api'

/**
 * The admin console: accounts on this server, and licence profiles.
 *
 * Every endpoint re-checks its permission on the server. Hiding the section
 * from people without it is a convenience, not the control.
 */

export const adminKeys = {
  all:       ['admin'] as const,
  roles:     () => [...adminKeys.all, 'roles'] as const,
  users:     () => [...adminKeys.all, 'users'] as const,
  licensing: () => [...adminKeys.all, 'licensing'] as const,
  licensees: () => [...adminKeys.all, 'licensees'] as const,
  licensee:  (id: string) => [...adminKeys.all, 'licensee', id] as const,
}

// ── Accounts ─────────────────────────────────────────────────────────────────

export interface Account {
  id:                   string
  username:             string
  full_name:            string
  position_title:       string | null
  role_codes:           string[]
  role_names:           string[]
  is_active:            boolean
  must_change_password: boolean
  locked:               boolean
  last_login_at:        string | null
  created_at:           string
}

export interface RoleOption {
  code:        string
  name:        string
  description: string
}

/** The password is in this response and nowhere else. */
export interface AccountWithPassword {
  account:  Account
  password: string
}

export interface NewAccountInput {
  username:        string
  full_name:       string
  position_title?: string
  role:            string
}

export function useRoles(): UseQueryResult<RoleOption[], Error> {
  return useQuery({
    queryKey:  adminKeys.roles(),
    queryFn:   () => api.get<RoleOption[]>('/api/admin/roles'),
    staleTime: 5 * 60_000,
  })
}

export function useUserAccounts(): UseQueryResult<Account[], Error> {
  return useQuery({
    queryKey: adminKeys.users(),
    queryFn:  () => api.get<Account[]>('/api/admin/users'),
  })
}

export function useCreateUser(): UseMutationResult<AccountWithPassword, Error, NewAccountInput> {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input) => api.post<AccountWithPassword>('/api/admin/users', input),
    onSuccess:  () => void queryClient.invalidateQueries({ queryKey: adminKeys.users() }),
  })
}

export function useResetPassword(): UseMutationResult<AccountWithPassword, Error, string> {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id) => api.post<AccountWithPassword>(`/api/admin/users/${id}/password`, {}),
    onSuccess:  () => void queryClient.invalidateQueries({ queryKey: adminKeys.users() }),
  })
}

export function useSetActive(): UseMutationResult<Account, Error, { id: string; active: boolean }> {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, active }) => api.post<Account>(`/api/admin/users/${id}/active`, { active }),
    onSuccess:  () => void queryClient.invalidateQueries({ queryKey: adminKeys.users() }),
  })
}

// ── Licensing ────────────────────────────────────────────────────────────────

export interface LicensingCapability {
  /** Whether this server holds the licensing key and can sign. */
  can_issue: boolean
  /** Why not, already written for a person, when it cannot. */
  reason:    string | null
  this_server: {
    tenant_id:    string
    tenant_code:  string
    municipality: string
  }
}

export interface LicenseeSummary {
  id:                 string
  municipality_name:  string
  target_tenant_code: string
  for_this_server:    boolean
  accountant_name:    string | null
  people_count:       number
  keys_issued:        number
  latest_plan:        string | null
  latest_seats:       number | null
  latest_expires_at:  string | null
  created_at:         string
}

export interface LicenseePerson {
  full_name:      string
  username:       string
  role:           'ACCOUNTANT' | 'STAFF'
  position_title: string | null
}

export interface IssuedKey {
  id:             string
  token:          string
  plan:           string
  seats:          number
  not_before:     string
  expires_at:     string
  grace_days:     number
  issued_at:      string
  issued_by_name: string | null
}

/** A recorded renewal: another month or year on the same key. */
export interface Renewal {
  id:              string
  period_start:    string
  paid_through:    string
  notes:           string | null
  renewed_at:      string
  renewed_by_name: string | null
}

export interface LicenseeDetail {
  id:                 string
  municipality_name:  string
  target_tenant_id:   string
  target_tenant_code: string
  for_this_server:    boolean
  notes:              string | null
  created_at:         string
  created_by_name:    string | null
  people:             LicenseePerson[]
  /** Newest first. The first is the one worth installing. */
  keys:               IssuedKey[]
  /** `monthly`, `annual`, or `lifetime`. */
  access:             string | null
  /** The later of the key's end and the latest renewal. */
  paid_through:       string | null
  /** Newest first. */
  renewals:           Renewal[]
}

export interface PersonInput {
  full_name:       string
  username:        string
  position_title?: string
}

/** How long a key gives access for. */
export type Access = 'monthly' | 'annual' | 'lifetime'

export interface TermsInput {
  access:     Access
  seats:      number
  starts_on?: string
}

/** Step one of issuing: who the key is for, and on what terms. */
export interface LicenceDetailsInput {
  municipality_name: string
  for_this_server:   boolean
  accountant:        PersonInput
  staff:             PersonInput[]
  terms:             TermsInput
}

/** Step two: a signed key, not yet saved to any profile. */
export interface GeneratedKey {
  token:              string
  municipality_name:  string
  for_this_server:    boolean
  target_tenant_code: string
  access:             Access
  seats:              number
  not_before:         string
  expires_at:         string
  grace_days:         number
  people:             { full_name: string; username: string; role: 'ACCOUNTANT' | 'STAFF'; position_title: string | null }[]
}

export function useLicensing(enabled = true): UseQueryResult<LicensingCapability, Error> {
  return useQuery({
    queryKey: adminKeys.licensing(),
    queryFn:  () => api.get<LicensingCapability>('/api/admin/licensing'),
    enabled,
  })
}

export function useLicensees(enabled = true): UseQueryResult<LicenseeSummary[], Error> {
  return useQuery({
    queryKey: adminKeys.licensees(),
    queryFn:  () => api.get<LicenseeSummary[]>('/api/admin/licensees'),
    enabled,
  })
}

export function useLicensee(id: string): UseQueryResult<LicenseeDetail, Error> {
  return useQuery({
    queryKey: adminKeys.licensee(id),
    queryFn:  () => api.get<LicenseeDetail>(`/api/admin/licensees/${id}`),
  })
}

/** Step two: signs a key and returns it. Saves nothing. */
export function useGenerateLicence(): UseMutationResult<GeneratedKey, Error, LicenceDetailsInput> {
  return useMutation({
    mutationFn: (input) => api.post<GeneratedKey>('/api/admin/licensees/generate', input),
  })
}

/**
 * Step three: saves the profile for a generated key.
 *
 * Sends the key itself; the server reads the profile out of it, so what is
 * saved is exactly what was generated and copied.
 */
export function useSaveLicensee(): UseMutationResult<LicenseeDetail, Error, { token: string; notes?: string }> {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input) => api.post<LicenseeDetail>('/api/admin/licensees', input),
    onSuccess:  (detail) => {
      queryClient.setQueryData(adminKeys.licensee(detail.id), detail)
      void queryClient.invalidateQueries({ queryKey: adminKeys.licensees() })
    },
  })
}

/**
 * Records a paid renewal: another month or year on the same key.
 *
 * No key is issued. The municipality's server collects the new end date by
 * itself when it checks in.
 */
export function useRenew(id: string): UseMutationResult<LicenseeDetail, Error, { notes?: string }> {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input) => api.post<LicenseeDetail>(`/api/admin/licensees/${id}/renew`, input),
    onSuccess:  (detail) => {
      queryClient.setQueryData(adminKeys.licensee(id), detail)
      void queryClient.invalidateQueries({ queryKey: adminKeys.licensees() })
    },
  })
}
