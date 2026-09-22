import { useQuery, type UseQueryResult } from '@tanstack/react-query'
import { api } from '@/lib/api'

// ── Query keys ────────────────────────────────────────────────────────────
// One factory per domain. Everything that touches health invalidates through
// here, so there is never a stale key spelled slightly differently elsewhere.
export const healthKeys = {
  all:   ['health'] as const,
  node:  () => [...healthKeys.all, 'node'] as const,
  audit: () => [...healthKeys.all, 'audit'] as const,
}

export interface NodeHealth {
  service:         string
  version:         string
  database:        string
  node_code:       string
  municipality_id: string
  server_time:     string
}

export interface AuditIntegrity {
  intact:   boolean
  summary:  string
  problems: Array<{ audit_id: number; seq: number; at: string; problem: string }>
}

/**
 * Whether the office node is reachable, and who it is.
 *
 * Polled rather than fetched once: this is what drives the connection
 * indicator, and a clerk needs to see the connection come back without
 * reloading. Retries are disabled because the offline state is a legitimate
 * answer here, not a failure to paper over.
 *
 * `refetchIntervalInBackground` is deliberate. By default the poll pauses
 * while the window is hidden, and this window is very often behind another
 * application. Two things depend on noticing the node's return without anyone
 * clicking: the connection indicator must be correct the moment it is looked
 * at, and queued offline drafts must start flushing on their own.
 */
export function useNodeHealth(): UseQueryResult<NodeHealth, Error> {
  return useQuery({
    queryKey:                   healthKeys.node(),
    queryFn:                    () => api.get<NodeHealth>('/api/health'),
    refetchInterval:            15_000,
    refetchIntervalInBackground: true,
    retry:                      false,
    staleTime:                  10_000,
  })
}

/**
 * Verifies the audit trail has not been altered.
 *
 * Deliberately not polled — it walks the whole chain. Run on demand from the
 * administration screen and on a schedule server-side.
 */
export function useAuditIntegrity(enabled = false): UseQueryResult<AuditIntegrity, Error> {
  return useQuery({
    queryKey: healthKeys.audit(),
    queryFn:  () => api.get<AuditIntegrity>('/api/health/audit'),
    enabled,
    retry:    false,
  })
}
