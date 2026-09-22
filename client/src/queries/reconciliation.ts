import {
  useMutation, useQuery, useQueryClient,
  type UseMutationResult, type UseQueryResult,
} from '@tanstack/react-query'
import { api } from '@/lib/api'
import type { Amount } from '@/lib/money'

export const reconcileKeys = {
  all:         ['reconciliation'] as const,
  banks:       () => [...reconcileKeys.all, 'banks'] as const,
  statements:  (bankId?: string) => [...reconcileKeys.all, 'statements', bankId ?? 'all'] as const,
  lines:       (statementId: string) => [...reconcileKeys.all, 'lines', statementId] as const,
  suggestions: (lineId: string) => [...reconcileKeys.all, 'suggestions', lineId] as const,
}

export interface BankAccount {
  id: string
  bank_name: string
  account_no: string
  account_name: string
  fund_name: string
}

export interface Statement {
  id: string
  bank_name: string
  account_no: string
  statement_date: string
  period_start: string
  period_end: string
  opening_balance: Amount
  closing_balance: Amount
  line_count: number
  unmatched_count: number
}

export interface StatementLine {
  id: string
  line_no: number
  txn_date: string
  description: string
  reference: string | null
  withdrawal: Amount
  deposit: Amount
  matched_kind: string | null
  matched_id: string | null
  /** `exact`, `amount`, or `manual` — how much to trust this match. */
  match_tier: string | null
  matched_reference: string | null
  matched_counterparty: string | null
}

export interface Suggestion {
  kind: string
  record_id: string
  record_ref: string | null
  record_date: string
  record_amount: Amount
  counterparty: string | null
  score: string
  /** Why this was suggested, in words. */
  why: string | null
}

export function useBankAccounts(): UseQueryResult<BankAccount[], Error> {
  return useQuery({
    queryKey:  reconcileKeys.banks(),
    queryFn:   () => api.get<BankAccount[]>('/api/bank-accounts'),
    staleTime: 5 * 60_000,
  })
}

export function useStatements(bankId?: string): UseQueryResult<Statement[], Error> {
  return useQuery({
    queryKey: reconcileKeys.statements(bankId),
    queryFn:  () =>
      api.get<Statement[]>(
        `/api/statements${bankId ? `?bank_account_id=${bankId}` : ''}`,
      ),
  })
}

export function useStatementLines(statementId: string | null): UseQueryResult<StatementLine[], Error> {
  return useQuery({
    queryKey: reconcileKeys.lines(statementId ?? ''),
    queryFn:  () => api.get<StatementLine[]>(`/api/statements/${statementId}/lines`),
    enabled:  statementId !== null,
  })
}

/**
 * Ranked candidates for an unmatched line.
 *
 * Fetched only when a line is actually selected — computing suggestions scans
 * payments across a four-month window, which is not something to do for every
 * row of a statement on load.
 */
export function useSuggestions(lineId: string | null): UseQueryResult<Suggestion[], Error> {
  return useQuery({
    queryKey: reconcileKeys.suggestions(lineId ?? ''),
    queryFn:  () => api.get<Suggestion[]>(`/api/statement-lines/${lineId}/suggestions`),
    enabled:  lineId !== null,
  })
}

export function useMatchLine(): UseMutationResult<
  unknown, Error, { lineId: string; kind: string; recordId: string }
> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ lineId, kind, recordId }) =>
      api.post(`/api/statement-lines/${lineId}/match`, { kind, record_id: recordId }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: reconcileKeys.all })
    },
  })
}

export function useUnmatchLine(): UseMutationResult<unknown, Error, string> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (lineId: string) => api.post(`/api/statement-lines/${lineId}/unmatch`, {}),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: reconcileKeys.all })
    },
  })
}

export interface RematchResult {
  matched_exact: number
  matched_by_amount: number
  still_unmatched: number
  message: string
}

export function useRematch(): UseMutationResult<RematchResult, Error, string> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (statementId: string) =>
      api.post<RematchResult>(`/api/statements/${statementId}/rematch`, {}),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: reconcileKeys.all })
    },
  })
}
