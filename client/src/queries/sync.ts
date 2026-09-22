import { useMutation, useQuery, useQueryClient, type UseQueryResult } from '@tanstack/react-query'
import { api } from '@/lib/api'

/**
 * Uploading to the cloud.
 *
 * Polled while the screen is open, because the interesting states — a backlog
 * clearing, a connection returning — change without anybody pressing anything.
 */

export const syncKeys = { status: ['sync', 'status'] as const }

export interface SyncStatus {
  automatic:       boolean
  pending:         number
  last_attempt_at: string | null
  last_success_at: string | null
  /** Set only when something is wrong, already in plain language. */
  problem:         string | null
  /** Uploading has stopped and needs a person, not a retry. */
  halted:          boolean
  /** Whether a cloud service has been set up for this municipality at all. */
  configured:      boolean
}

export function useSyncStatus(): UseQueryResult<SyncStatus, Error> {
  return useQuery({
    queryKey: syncKeys.status,
    queryFn:  () => api.get<SyncStatus>('/api/sync/status'),
    refetchInterval: 15_000,
  })
}

export function useUploadNow() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: () => api.post<SyncStatus>('/api/sync/upload'),
    onSuccess: (status) => queryClient.setQueryData(syncKeys.status, status),
  })
}

export function useSetAutomatic() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (automatic: boolean) =>
      api.post<SyncStatus>('/api/sync/automatic', { automatic }),
    onSuccess: (status) => queryClient.setQueryData(syncKeys.status, status),
  })
}
