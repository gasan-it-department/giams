import { useQuery, type UseQueryResult } from '@tanstack/react-query'
import { api } from '@/lib/api'
import type { Amount } from '@/lib/money'

/**
 * The summary screen's data.
 *
 * Polled rather than pushed. A LAN of a dozen workstations does not need a
 * websocket to feel live, and polling degrades to "slightly stale" when the
 * office server is unreachable rather than to a dead socket nobody notices is
 * dead. `refetchIntervalInBackground` is deliberately on: a dashboard left up
 * on a second monitor is exactly the case that has to stay current.
 */

export const dashboardKeys = {
  all:    ['dashboard'] as const,
  period: (from: string, to: string, fundId?: string) =>
    [...dashboardKeys.all, from, to, fundId ?? 'all'] as const,
}

export interface Figures {
  cash:     Amount
  received: Amount
  paid:     Amount
  surplus:  Amount
}

export interface Attention {
  key:    string
  label:  string
  count:  number
  route:  string
  urgent: boolean
}

export interface BudgetUse {
  expense_class:    string
  budget:           Amount
  obligated:        Amount
  disbursed:        Amount
  utilisation_rate: string
}

export interface Dashboard {
  period_label: string
  as_of:        string
  figures:      Figures
  attention:    Attention[]
  budget:       BudgetUse[]
}

export function useDashboard(
  from: string,
  to: string,
  fundId?: string,
): UseQueryResult<Dashboard, Error> {
  return useQuery({
    queryKey: dashboardKeys.period(from, to, fundId),
    queryFn:  () =>
      api.get<Dashboard>(
        `/api/dashboard?from=${from}&to=${to}${fundId ? `&fund_id=${fundId}` : ''}`,
      ),
    refetchInterval: 30_000,
    refetchIntervalInBackground: true,
  })
}
