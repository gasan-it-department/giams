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
  state:          'full' | 'grace' | 'read_only'
  plan:           string
  seats:          number
  expires_at:     string | null
  days_remaining: number | null
  /** Already written for the office. Shown verbatim. */
  notice:         string | null
  can_record:     boolean
  /** A clock problem, which needs a different fix from a renewal. */
  clock_problem:  boolean
}

export function useLicence(): UseQueryResult<LicenceStatus, Error> {
  return useQuery({
    queryKey: licenceKeys.status,
    queryFn:  () => api.get<LicenceStatus>('/api/licence'),
    // Hourly: a subscription crosses into grace or read-only on a date, not on
    // a minute, and nobody needs to watch it tick.
    refetchInterval: 3_600_000,
    staleTime: 600_000,
  })
}

export function useInstallLicence() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (token: string) => api.post<LicenceStatus>('/api/licence', { token }),
    onSuccess: (status) => {
      queryClient.setQueryData(licenceKeys.status, status)
      // Anything refused while read-only can now succeed.
      void queryClient.invalidateQueries()
    },
  })
}
