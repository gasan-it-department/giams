import {
  useMutation, useQuery, useQueryClient,
  type UseMutationResult, type UseQueryResult,
} from '@tanstack/react-query'
import { api } from '@/lib/api'
import type { Amount } from '@/lib/money'

export const budgetKeys = {
  all:            ['budget'] as const,
  obligations:    (filter: ObligationFilter) => [...budgetKeys.all, 'obligations', filter] as const,
  raod:           (year: number) => [...budgetKeys.all, 'raod', year] as const,
  allotments:     (year: number) => [...budgetKeys.all, 'allotments', year] as const,
  appropriations: (year: number) => [...budgetKeys.all, 'appropriations', year] as const,
}

/** Every amount is a string. Nothing here parses one. */
export interface Obligation {
  id:                        string
  ors_no:                    string
  fund_id:                   string
  fund_name:                 string
  fiscal_year:               number
  expense_class:             string
  obligation_date:           string
  requesting_office_id:      string
  office_name:               string
  payee_id:                  string | null
  payee_name:                string | null
  particulars:               string
  amount:                    Amount
  payable_amount:            Amount
  paid_amount:               Amount
  balance_amount:            Amount
  status:                    string
  allotment_id:              string
  allotment_no:              string
  prepared_by_name:          string | null
  certified_budget_name:     string | null
  certified_accounting_name: string | null
}

export interface ObligationFilter {
  fiscal_year?: number
  fund_id?:     string
  status?:      string
  search?:      string
}

export interface RaodEntry {
  allotment_id:        string
  allotment_no:        string
  release_date:        string
  expense_class:       string
  fund_name:           string
  office_name:         string
  allotment_amount:    Amount
  obligation_amount:   Amount
  disbursement_amount: Amount
  unobligated_balance: Amount
}

/**
 * Builds a query string, dropping empty values.
 *
 * Takes an object with unknown values rather than `Record<string, unknown>`,
 * because an interface without an index signature is not assignable to that —
 * and widening the parameter is better than widening every filter type.
 */
function query(filter: object): string {
  const params = new URLSearchParams()
  for (const [key, value] of Object.entries(filter)) {
    if (value !== undefined && value !== null && value !== '') {
      params.set(key, String(value))
    }
  }
  const q = params.toString()
  return q ? `?${q}` : ''
}

export function useObligations(filter: ObligationFilter = {}): UseQueryResult<Obligation[], Error> {
  return useQuery({
    queryKey: budgetKeys.obligations(filter),
    queryFn:  () => api.get<Obligation[]>(`/api/obligations${query(filter)}`),
  })
}

export function useRaod(fiscalYear: number): UseQueryResult<RaodEntry[], Error> {
  return useQuery({
    queryKey: budgetKeys.raod(fiscalYear),
    queryFn:  () => api.get<RaodEntry[]>(`/api/registries/raod${query({ fiscal_year: fiscalYear })}`),
  })
}

export type CertificationBox = 'budget' | 'accounting'

/**
 * Signs Box B or Box C.
 *
 * Invalidates the whole budget branch on success: certifying changes the
 * obligation's status, and the RAOD registry counts only uncancelled
 * obligations, so both views can be affected.
 */
export function useCertifyObligation(): UseMutationResult<
  Obligation, Error, { id: string; box: CertificationBox }
> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ id, box }) =>
      api.post<Obligation>(`/api/obligations/${id}/certify`, { box }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: budgetKeys.all })
    },
  })
}
