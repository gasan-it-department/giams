import { useMutation, useQuery, useQueryClient, type UseQueryResult } from '@tanstack/react-query'
import { api } from '@/lib/api'

/**
 * The subscription.
 *
 * Read by every signed-in user, not just administrators: a clerk who cannot
 * record a voucher needs to find out why without waiting for somebody with
 * more permissions to come and explain it.
 */

export const licenceKeys = { status: ['licence'] as const }

export interface LicenceStatus {
  /**
   * `expiring` from seven days before the end, `grace` for the days after it,
   * then `locked`. `read_only` only ever means the computer's clock is wrong.
   */
  state:          'full' | 'expiring' | 'grace' | 'locked' | 'read_only'
  plan:           string
  seats:          number
  expires_at:     string | null
  /** When the grace period ends and the system locks. */
  locks_at:       string | null
  /** Days to the end while `expiring`; days to the lock during `grace`. */
  days_remaining: number | null
  /** Already written for the office. Shown verbatim. */
  notice:         string | null
  can_record:     boolean
  /** False only when locked. */
  can_use:        boolean
  /** A clock problem, which needs a different fix from a renewal. */
  clock_problem:  boolean
}

export function useLicence(enabled = true): UseQueryResult<LicenceStatus, Error> {
  return useQuery({
    queryKey: licenceKeys.status,
    queryFn:  () => api.get<LicenceStatus>('/api/licence'),
    enabled,
    // Hourly: a subscription changes stage on a date, not on a minute.
    refetchInterval: 3_600_000,
    staleTime: 600_000,
  })
}

/** What asking the licensing server for a renewal found. */
export interface CheckIn {
  outcome: 'renewed' | 'current' | 'no_licence' | 'not_configured' | 'unreachable' | 'refused'
  /** Written for the office. */
  message: string
  status:  LicenceStatus
}

/**
 * Collects any renewal from the licensing server now.
 *
 * The subscription is renewed on the licensing server; this server picks it
 * up by itself every few hours. This is for the office that has just paid and
 * would rather not wait.
 */
export function useCheckRenewal() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: () => api.post<CheckIn>('/api/licence/check', {}),
    onSuccess: (check) => {
      queryClient.setQueryData(licenceKeys.status, check.status)
      if (check.outcome === 'renewed') void queryClient.invalidateQueries()
    },
  })
}

/** An account a licence key brought with it when it was installed. */
export interface ProvisionedAccount {
  username:  string
  full_name: string
  role:      string
  /** `existing` for someone already on this server, left untouched. */
  status:    'created' | 'existing'
  /** Only for a new account, and only in this one response. */
  password:  string | null
}

export interface InstallOutcome extends LicenceStatus {
  provisioned: ProvisionedAccount[]
}

export function useInstallLicence() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (token: string) => api.post<InstallOutcome>('/api/licence', { token }),
    onSuccess: ({ provisioned: _provisioned, ...status }) => {
      queryClient.setQueryData(licenceKeys.status, status)
      // Anything refused while read-only can now succeed, and new accounts
      // appear in the admin list.
      void queryClient.invalidateQueries()
    },
  })
}
